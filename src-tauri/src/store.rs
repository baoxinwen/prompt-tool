use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Manager};

use crate::creds::{CredentialBackend, CredMemo, KeyringBackend};
use crate::models::{AppData, ClipboardItem};

/// 共享数据仓库：内存数据 + 本地 JSON 持久化
pub struct Store {
    pub data: AppData,
    path: PathBuf,
    /// 凭据后端：WebDAV 密码/Gist Token 真实值存这里，data.json 只落哨兵
    creds: Arc<dyn CredentialBackend>,
    /// 各账号最近一次成功写入凭据库的值：save 时比对，值未变跳过重复 keyring 写
    cred_memo: Mutex<CredMemo>,
    /// 置为 true 时剪贴板监听线程跳过采样（避免把程序自己写入的粘贴内容记入历史）
    pub suppress_clipboard: bool,
    /// 呼出快捷面板时的前台窗口句柄，用于粘贴时恢复焦点
    pub paste_target: Option<isize>,
    /// 自上次同步后有数据变更（内存态，不持久化），供自动同步判断
    pub dirty_unsynced: bool,
    /// 本地变更计数：每次 mutate 自增。同步收尾时比对快照时刻的计数，
    /// 判断上传期间是否有新变更，防止把未上传的内容误标为已同步
    pub mutations: u64,
    /// 启动时发现 data.json 损坏并已隔离的提示信息（供前端 toast 展示）
    pub recovered_notice: Option<String>,
    /// 剪贴板写入会话的代际计数：每次程序写入剪贴板（粘贴/复制）自增。
    /// 后台延时收尾线程（恢复原剪贴板/解除抑制）只在代际未变时执行，
    /// 防止快速连续两次操作时旧会话覆盖新会话的剪贴板内容
    pub paste_generation: u64,
    /// 最近一次「清空剪贴板历史」的条目（内存态，不持久化），供撤销恢复。
    /// Store 整体已在 Mutex 之后，无需再嵌套锁；被覆盖或应用退出时
    /// 才对其中的图片文件执行延迟删除
    pub stash: Option<Vec<ClipboardItem>>,
    /// 只读降级模式：data.json 存在但瞬态不可读（杀毒/备份工具占用等）时置位。
    /// 此模式下 save() 一律拒绝——内存里是默认数据，任何落盘都会用默认数据
    /// 覆盖原完好文件，等于销毁用户数据（评审 2026-09-10 C3）
    pub read_only: bool,
}

pub type SharedStore = Mutex<Store>;

/// 从 data.json 载入数据并跑迁移；解析损坏时隔离坏文件并以空数据启动；
/// 文件存在但读取失败（瞬态原因）不隔离、原文件保持原样，并置 read_only 旗标
/// （短暂重试后仍失败则进入只读降级）；载入后经凭据后端恢复哨兵字段/迁移旧版明文凭据。
/// 返回 (数据, 恢复提示, 是否瞬态读失败)
fn load_or_recover_with(
    path: &Path,
    creds: &dyn CredentialBackend,
    memo: &mut CredMemo,
) -> (AppData, Option<String>, bool) {
    let mut recovered_notice = None;
    let mut read_failed = false;
    let mut data = if path.exists() {
        // 瞬态占用往往几百毫秒内自行解除：短暂重试再宣布失败
        let mut raw = None;
        for attempt in 0..3 {
            match std::fs::read_to_string(path) {
                Ok(text) => {
                    raw = Some(text);
                    break;
                }
                Err(e) if attempt < 2 => {
                    eprintln!("[prompt-tool] data.json 读取失败（第 {} 次重试前）: {e}", attempt + 1);
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                Err(e) => {
                    // 「读不出来」≠「内容坏了」：瞬态读取失败绝不能把完好的
                    // data.json 改名隔离或用默认数据覆盖——那等于把好数据当坏数据扔掉。
                    // 置只读降级：原文件保持原样，本次运行不落盘（评审 2026-09-10 C3）
                    eprintln!("[prompt-tool] data.json 读取失败（文件未被改动，进入只读模式）: {e}");
                    recovered_notice = Some(format!(
                        "数据文件暂时无法读取（{e}），原文件未被改动。本次以只读模式运行，改动将无法保存；请关闭可能占用该文件的程序（如杀毒/备份工具）后重启"
                    ));
                    read_failed = true;
                }
            }
        }
        match raw {
            Some(text) => match serde_json::from_str::<AppData>(&text) {
                Ok(d) => d,
                Err(e) => {
                    // 内容确实损坏才隔离：改名保留现场，空数据继续运行。
                    // 隔离失败（占用/权限）必须进入只读保护现场（评审 2026-09-10 I12）：
                    // 静默吞掉失败后，启动 save 会用新数据覆盖原地坏文件，
                    // 用户本可抢救的数据永久丢失
                    let dir = path.parent().unwrap_or_else(|| Path::new("."));
                    let quarantined =
                        dir.join(format!("data.json.corrupt-{}", crate::models::now_ms()));
                    match std::fs::rename(path, &quarantined) {
                        Ok(()) => {
                            eprintln!(
                                "[prompt-tool] data.json 解析失败已隔离到 {}，以空数据启动: {e}",
                                quarantined.display()
                            );
                            recovered_notice = Some(format!(
                                "数据文件损坏，已隔离为 {}，本次以空数据启动。若存在 data.json.bak 可手动改名为 data.json 恢复",
                                quarantined
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_default()
                            ));
                        }
                        Err(re) => {
                            eprintln!(
                                "[prompt-tool] data.json 解析失败且隔离失败（{re}），进入只读模式: {e}"
                            );
                            recovered_notice = Some(format!(
                                "数据文件损坏且无法隔离（{re}），本次以只读模式启动以保护原文件；请手动处理该文件后重启"
                            ));
                            read_failed = true;
                        }
                    }
                    AppData::default()
                }
            },
            None => AppData::default(),
        }
    } else {
        AppData::default()
    };
    crate::models::migrate(&mut data);
    // 只读模式下不碰凭据库：内存数据是默认值，任何恢复/迁移写入都可能
    // 在后续被拒绝的落盘路径里产生半套状态；保持 keyring 原样最安全
    if !read_failed {
        crate::creds::restore(&mut data, creds, memo);
    }
    (data, recovered_notice, read_failed)
}

/// 对 Mutex 中毒场景的容错取锁
pub fn lock(app: &tauri::AppHandle) -> std::sync::MutexGuard<'_, Store> {
    use tauri::Manager;
    let store: &SharedStore = app.state::<SharedStore>().inner();
    store.lock().unwrap_or_else(|p| p.into_inner())
}

/// 应用数据目录。自动化测试可通过环境变量 PROMPTMATE_DATA_DIR 覆盖，
/// 避免真机 E2E 读写用户真实数据；正常启动不受影响
pub(crate) fn resolve_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    if let Ok(dir) = std::env::var("PROMPTMATE_DATA_DIR") {
        if !dir.trim().is_empty() {
            return Ok(PathBuf::from(dir));
        }
    }
    app.path()
        .app_data_dir()
        .map_err(|e| format!("无法确定数据目录: {e}"))
}

impl Store {
    pub fn load(app: &AppHandle) -> Result<Self, String> {
        let dir = resolve_data_dir(app)?;
        std::fs::create_dir_all(&dir).map_err(|e| format!("创建数据目录失败: {e}"))?;
        let path = dir.join("data.json");

        let creds: Arc<dyn CredentialBackend> = Arc::new(KeyringBackend);
        let mut cred_memo = CredMemo::default();
        let (data, recovered_notice, read_only) =
            load_or_recover_with(&path, creds.as_ref(), &mut cred_memo);

        Ok(Self {
            data,
            path,
            creds,
            cred_memo: Mutex::new(cred_memo),
            suppress_clipboard: false,
            paste_target: None,
            dirty_unsynced: false,
            mutations: 0,
            recovered_notice,
            paste_generation: 0,
            stash: None,
            read_only,
        })
    }

    pub fn is_first_run(&self) -> bool {
        !self.path.exists() && !self.data.seeded
    }

    /// 启动收尾：种子示例数据 + 首次落盘。只读降级模式（data.json 瞬态不可读）
    /// 必须整体跳过——此时内存是默认数据，seed+save 会用默认数据覆盖原完好
    /// 文件（评审 2026-09-10 C3）；图片 GC 同理由调用方按 read_only 跳过
    pub fn startup_seed_and_persist(&mut self) -> Result<(), String> {
        if self.read_only {
            return Ok(());
        }
        self.data.seed_if_empty();
        self.save()
    }

    pub fn save(&self) -> Result<(), String> {
        // 只读降级（data.json 瞬态不可读）：内存里是默认数据，任何落盘都会
        // 覆盖原完好文件（评审 2026-09-10 C3），必须整体拒绝
        if self.read_only {
            return Err(
                "数据文件暂时无法读取，本次启动为只读模式，改动未保存；请关闭占用该文件的程序后重启"
                    .to_string(),
            );
        }
        // 凭据先写凭据库、data.json 只落哨兵：凭据写入失败必须放弃落盘，
        // 否则磁盘上是哨兵、凭据库里没有值，重启后凭据就丢了
        let mut payload = self.data.clone();
        // 锁中毒容错与 store 主锁（lock()）口径一致：cred_memo 曾 panic
        // 不应让此后每次保存都跟着 panic（评审 M2）
        crate::creds::persist(
            &mut payload,
            self.creds.as_ref(),
            &mut self.cred_memo.lock().unwrap_or_else(|p| p.into_inner()),
        )?;
        let tmp = self.path.with_extension("json.tmp");
        let json = serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?;
        {
            use std::io::Write;
            let mut f = std::fs::File::create(&tmp).map_err(|e| format!("写入数据失败: {e}"))?;
            f.write_all(json.as_bytes())
                .map_err(|e| format!("写入数据失败: {e}"))?;
            // 落盘后再改名，避免断电时 rename 出一个空壳文件
            f.sync_all().map_err(|e| format!("写入数据失败: {e}"))?;
        }
        // 保留上一份完好数据；本次写入若在之后损坏，可手动改回恢复
        if self.path.exists() {
            let _ = std::fs::copy(&self.path, self.path.with_extension("json.bak"));
        }
        std::fs::rename(&tmp, &self.path).map_err(|e| format!("保存数据失败: {e}"))?;
        Ok(())
    }

    /// 修改数据并落盘（用户数据变更，标记待同步）。闭包可有返回值
    /// （如 upsert_prompt 回传有效 id），与落盘同成功/失败语义
    pub fn mutate<F, R>(&mut self, f: F) -> Result<R, String>
    where
        F: FnOnce(&mut AppData) -> R,
    {
        let ret = f(&mut self.data);
        self.commit().map(|()| ret)
    }

    /// 对已就地修改过 data 的变更收尾：计数 +1、置待同步标记并落盘。
    /// 供无法用 mutate 闭包表达的场景（如导入循环中逐文件修改后统一收尾），
    /// 保证与 mutate 相同的同步记账语义，落盘失败也不得被静默吞掉
    pub fn commit(&mut self) -> Result<(), String> {
        self.mutations = self.mutations.wrapping_add(1);
        self.dirty_unsynced = true;
        self.save()
    }

    pub fn data_dir(&self) -> PathBuf {
        self.path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    }

    /// 仅供同 crate 单元测试构造 Store：字段私有，sync.rs 等模块无法直接
    /// 构造。凭据后端固定为内存实现，不触碰系统 keyring
    #[cfg(test)]
    pub(crate) fn test_store(dir: &Path) -> Store {
        let path = dir.join("data.json");
        let creds: Arc<dyn CredentialBackend> = Arc::new(crate::creds::MemoryBackend::default());
        let mut memo = CredMemo::default();
        let (data, notice, read_only) = load_or_recover_with(&path, creds.as_ref(), &mut memo);
        Store {
            data,
            path,
            creds,
            cred_memo: Mutex::new(memo),
            suppress_clipboard: false,
            paste_target: None,
            dirty_unsynced: false,
            mutations: 0,
            recovered_notice: notice,
            paste_generation: 0,
            stash: None,
            read_only,
        }
    }

    /// 后台收尾线程专用：仅当代际未变（期间没有新的剪贴板写入会话）时
    /// 解除抑制。旧会话提前解除会把新会话写入的内容误记进历史（评审 I2）。
    /// 返回是否实际解除
    pub fn release_suppress_if_current(&mut self, generation: u64) -> bool {
        if self.paste_generation == generation {
            self.suppress_clipboard = false;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Prompt;
    use std::sync::Arc;

    fn stub_prompt(id: &str, title: &str) -> Prompt {
        Prompt {
            id: id.to_string(),
            title: title.to_string(),
            content: format!("正文-{title}"),
            category: "开发".into(),
            tags: vec![],
            pinned: false,
            hotkey: String::new(),
            use_count: 0,
            last_used_at: 0,
            created_at: 1,
            updated_at: 1,
        }
    }

    fn store_in(dir: &Path) -> Store {
        Store::test_store(dir)
    }

    fn store_in_with(creds: Arc<dyn CredentialBackend>, dir: &Path) -> Store {
        let path = dir.join("data.json");
        let mut memo = CredMemo::default();
        let (data, notice, read_only) = load_or_recover_with(&path, creds.as_ref(), &mut memo);
        Store {
            data,
            path,
            creds,
            cred_memo: Mutex::new(memo),
            suppress_clipboard: false,
            paste_target: None,
            dirty_unsynced: false,
            mutations: 0,
            recovered_notice: notice,
            paste_generation: 0,
            stash: None,
            read_only,
        }
    }

    // ---------- load / 损坏隔离 ----------

    #[test]
    fn missing_file_starts_with_defaults_after_migrate() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(dir.path());
        assert!(store.recovered_notice.is_none(), "无文件不算恢复场景");
        assert_eq!(store.data.settings.hotkey, "alt+q");
        assert!(store.data.settings.hotkey_migrated, "载入时必须执行迁移");
        assert!(store.is_first_run());
    }

    #[test]
    fn valid_file_is_loaded_and_not_first_run() {
        let dir = tempfile::tempdir().unwrap();
        let mut original = AppData::default();
        original.prompts.push(stub_prompt("p1", "已有条目"));
        std::fs::write(
            dir.path().join("data.json"),
            serde_json::to_string(&original).unwrap(),
        )
        .unwrap();

        let store = store_in(dir.path());
        assert!(store.recovered_notice.is_none());
        assert_eq!(store.data.prompts.len(), 1);
        assert_eq!(store.data.prompts[0].title, "已有条目");
        assert!(!store.is_first_run());
    }

    #[test]
    fn corrupt_file_is_quarantined_with_notice() {
        let dir = tempfile::tempdir().unwrap();
        let bad = "{{{ 不是合法 JSON".to_string();
        std::fs::write(dir.path().join("data.json"), &bad).unwrap();

        let store = store_in(dir.path());
        let notice = store
            .recovered_notice
            .as_ref()
            .expect("损坏场景必须返回恢复提示");
        assert!(notice.contains("data.json.corrupt-"), "提示应包含隔离文件名: {notice}");
        assert!(!dir.path().join("data.json").exists(), "坏文件必须被改名隔离");
        assert!(store.data.prompts.is_empty(), "损坏后以空数据启动");

        let quarantined: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.starts_with("data.json.corrupt-"))
            .collect();
        assert_eq!(quarantined.len(), 1, "只应隔离出一个文件");
        let saved = std::fs::read_to_string(dir.path().join(&quarantined[0])).unwrap();
        assert_eq!(saved, bad, "隔离文件必须原样保留坏内容供抢救");
    }

    // ---------- 评审 2026-09-10 C3：启动瞬态读失败必须只读保全 ----------

    #[test]
    fn transient_read_failure_flags_read_only_and_leaves_file_untouched() {
        let dir = tempfile::tempdir().unwrap();
        // 用「同名目录」模拟 path.exists()==true 但 read_to_string 失败的瞬态场景
        //（杀毒/备份工具短暂占用的等价物）
        let path = dir.path().join("data.json");
        std::fs::create_dir(&path).unwrap();
        let creds: Arc<dyn CredentialBackend> = Arc::new(crate::creds::MemoryBackend::default());
        let mut memo = CredMemo::default();
        let (_data, notice, read_failed) = load_or_recover_with(&path, creds.as_ref(), &mut memo);
        assert!(read_failed, "读取失败必须置 read_only 旗标");
        assert!(notice.is_some(), "必须向用户说明进入降级模式");
        assert!(path.is_dir(), "瞬态读失败不得改名/隔离/删除原文件");
    }

    #[test]
    fn read_only_store_refuses_save_and_preserves_file() {
        let dir = tempfile::tempdir().unwrap();
        let original = r#"{"version":1,"seeded":true,"prompts":[],"categories":[],"clipboard":[],"tombstones":[],"settings":{}}"#;
        std::fs::write(dir.path().join("data.json"), original).unwrap();
        let mut store = store_in(dir.path());
        store.read_only = true;

        let res = store.save();
        assert!(res.is_err(), "只读模式必须拒绝任何落盘，防止默认数据覆盖原文件");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("data.json")).unwrap(),
            original,
            "原文件必须逐字节保留"
        );
    }

    #[test]
    fn startup_seed_and_persist_skips_seeding_in_read_only() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = store_in(dir.path());
        store.read_only = true;

        store.startup_seed_and_persist().unwrap();
        assert!(!store.data.seeded, "只读模式不得 seed 示例数据");
        assert!(store.data.prompts.is_empty());
        assert!(!dir.path().join("data.json").exists(), "只读模式不得首次落盘");
    }

    #[test]
    fn startup_seed_and_persist_normal_path_still_seeds() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = store_in(dir.path());
        assert!(!store.read_only);
        store.startup_seed_and_persist().unwrap();
        assert!(store.data.seeded, "正常路径保持原有 seed 行为");
        assert!(dir.path().join("data.json").exists(), "正常路径保持首次落盘");
    }

    // ---------- save / .bak ----------

    #[test]
    fn save_persists_deserializable_data_json() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = store_in(dir.path());
        store.data.prompts.push(stub_prompt("p1", "条目"));
        store.save().expect("save 应成功");

        let raw = std::fs::read_to_string(dir.path().join("data.json")).unwrap();
        let parsed: AppData = serde_json::from_str(&raw).expect("落盘文件必须可反序列化");
        assert_eq!(parsed.prompts[0].title, "条目");
    }

    #[test]
    fn save_keeps_previous_version_as_bak() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = store_in(dir.path());
        store.data.prompts.push(stub_prompt("p1", "第一版"));
        store.save().unwrap();

        store
            .mutate(|d| d.prompts.push(stub_prompt("p2", "第二版")))
            .unwrap();

        let bak_path = dir.path().join("data.json.bak");
        assert!(bak_path.exists(), "二次保存后必须存在 .bak");
        let bak: AppData =
            serde_json::from_str(&std::fs::read_to_string(&bak_path).unwrap()).unwrap();
        assert_eq!(bak.prompts.len(), 1, ".bak 应保存上一版（仅 1 条）");
        assert_eq!(bak.prompts[0].title, "第一版");
        assert_eq!(store.data.prompts.len(), 2);
    }

    #[test]
    fn mutate_updates_counters_flags_and_disk() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = store_in(dir.path());
        assert_eq!(store.mutations, 0);
        assert!(!store.dirty_unsynced);

        store
            .mutate(|d| d.prompts.push(stub_prompt("p1", "变更一")))
            .unwrap();
        assert_eq!(store.mutations, 1);
        assert!(store.dirty_unsynced);

        store
            .mutate(|d| d.prompts.push(stub_prompt("p2", "变更二")))
            .unwrap();
        assert_eq!(store.mutations, 2);
        let raw = std::fs::read_to_string(dir.path().join("data.json")).unwrap();
        let parsed: AppData = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed.prompts.len(), 2, "每次 mutate 都应落盘");
    }

    // ---------- 并发 mutate ----------

    #[test]
    fn unreadable_file_is_not_quarantined() {
        // 读取失败 ≠ 内容损坏：瞬态不可读（这里用目录占位模拟被占用/权限问题）
        // 不得把原文件改名隔离，否则用户重启后完好数据找不回来
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("data.json")).unwrap();

        let store = store_in(dir.path());
        assert!(store.recovered_notice.is_some(), "读取失败必须提示用户");
        let notice = store.recovered_notice.as_ref().unwrap();
        assert!(notice.contains("未被改动"), "提示必须说明原文件未被动过: {notice}");
        assert!(notice.contains("重启"), "提示应说明重启可恢复: {notice}");
        assert!(
            dir.path().join("data.json").is_dir(),
            "读取失败时原文件必须原样保留（未改名未删除）"
        );
        let quarantined: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("data.json.corrupt-"))
            .collect();
        assert!(quarantined.is_empty(), "读取失败不得触发损坏隔离");
    }

    #[test]
    fn save_persists_sentinel_not_plaintext_credentials() {
        let dir = tempfile::tempdir().unwrap();
        let creds = Arc::new(crate::creds::MemoryBackend::default());
        let mut store = store_in_with(creds.clone(), dir.path());
        store.data.settings.webdav.password = "webdav-secret".into();
        store.data.settings.gist.token = "gist-secret".into();

        store.save().expect("save 应成功");

        // 落盘文件只含哨兵，不含明文凭据
        let raw = std::fs::read_to_string(dir.path().join("data.json")).unwrap();
        assert!(raw.contains(crate::creds::CRED_SENTINEL), "必须写哨兵占位: {raw}");
        assert!(!raw.contains("webdav-secret"), "明文密码不得进 data.json");
        assert!(!raw.contains("gist-secret"), "明文 token 不得进 data.json");
        // 真实值在凭据后端里
        assert_eq!(creds.get(crate::creds::ACCT_WEBDAV).unwrap().as_deref(), Some("webdav-secret"));
        assert_eq!(creds.get(crate::creds::ACCT_GIST).unwrap().as_deref(), Some("gist-secret"));
    }

    #[test]
    fn load_restores_credentials_from_backend() {
        let dir = tempfile::tempdir().unwrap();
        let creds = Arc::new(crate::creds::MemoryBackend::default());
        let mut store = store_in_with(creds.clone(), dir.path());
        store.data.settings.webdav.password = "webdav-secret".into();
        store.save().unwrap();

        // 重新载入：哨兵被后端真实值替换，内存态仍是明文（供前端/命令使用）
        let (data, _, _) = crate::store::load_or_recover_with(
            &dir.path().join("data.json"),
            creds.as_ref(),
            &mut CredMemo::default(),
        );
        assert_eq!(data.settings.webdav.password, "webdav-secret");
    }

    #[test]
    fn legacy_plaintext_is_migrated_to_backend_on_load() {
        let dir = tempfile::tempdir().unwrap();
        // 旧版本 data.json：凭据是明文
        let mut original = AppData::default();
        original.settings.webdav.password = "old-plain".into();
        std::fs::write(
            dir.path().join("data.json"),
            serde_json::to_string(&original).unwrap(),
        )
        .unwrap();

        let creds = Arc::new(crate::creds::MemoryBackend::default());
        let (data, _, _) = crate::store::load_or_recover_with(
            &dir.path().join("data.json"),
            creds.as_ref(),
            &mut CredMemo::default(),
        );
        // 内存不丢值，且已搬进凭据后端（下次 save 起即为哨兵）
        assert_eq!(data.settings.webdav.password, "old-plain");
        assert_eq!(creds.get(crate::creds::ACCT_WEBDAV).unwrap().as_deref(), Some("old-plain"));
    }

    #[test]
    fn credential_backend_failure_fails_the_save() {
        let dir = tempfile::tempdir().unwrap();
        let creds = Arc::new(crate::creds::MemoryBackend::default());
        creds.fail_sets.store(true, std::sync::atomic::Ordering::SeqCst);
        let mut store = store_in_with(creds, dir.path());
        store.data.settings.webdav.password = "secret".into();

        let err = store.save().expect_err("凭据后端失败必须让保存失败");
        assert!(err.contains("凭据"), "错误信息应说明是凭据写入失败: {err}");
        // data.json 不得在凭据写入失败时落盘（否则磁盘上是哨兵、后端里没有值）
        assert!(!dir.path().join("data.json").exists(), "凭据写失败时不能写出哨兵文件");
    }

    #[test]
    fn commit_matches_mutate_bookkeeping() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = store_in(dir.path());

        // 就地修改后用 commit 收尾，必须与 mutate 有相同的记账与落盘语义
        store.data.prompts.push(stub_prompt("p1", "就地修改"));
        store.commit().expect("commit 应成功");
        assert_eq!(store.mutations, 1, "commit 必须自增变更计数");
        assert!(store.dirty_unsynced, "commit 必须置待同步标记");
        let raw = std::fs::read_to_string(dir.path().join("data.json")).unwrap();
        let parsed: AppData = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed.prompts.len(), 1, "commit 必须真实落盘");
    }

    #[test]
    fn release_suppress_only_when_generation_current() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = store_in(dir.path());
        store.suppress_clipboard = true;
        store.paste_generation = 5;

        // 旧会话（代际已过）：不得解除抑制，否则新会话写入的内容会被
        // 剪贴板监听误记进历史（评审 I2）
        assert!(!store.release_suppress_if_current(4), "旧会话不得解除抑制");
        assert!(store.suppress_clipboard, "抑制必须保持");

        // 当前会话：正常解除
        assert!(store.release_suppress_if_current(5), "当前会话应解除抑制");
        assert!(!store.suppress_clipboard);
    }

    #[test]
    fn concurrent_mutates_count_exactly_and_keep_file_valid() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(std::sync::Mutex::new(store_in(dir.path())));

        let handles: Vec<_> = (0..4)
            .map(|t| {
                let store = Arc::clone(&store);
                std::thread::spawn(move || {
                    for i in 0..25 {
                        let mut s = store.lock().unwrap();
                        s.mutate(|d| d.prompts.push(stub_prompt(&format!("p{t}-{i}"), &format!("t{t}-{i}"))))
                            .unwrap();
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }

        let s = store.lock().unwrap();
        assert_eq!(s.mutations, 100, "4 线程 × 25 次变更，计数必须精确");
        assert_eq!(s.data.prompts.len(), 100);
        let raw = std::fs::read_to_string(dir.path().join("data.json")).unwrap();
        let parsed: AppData = serde_json::from_str(&raw).expect("并发保存后的文件必须始终合法");
        assert_eq!(parsed.prompts.len(), 100);
        let mut ids: Vec<&str> = parsed.prompts.iter().map(|p| p.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 100, "所有变更都必须真实落盘、无丢失");
    }
}

//! 凭据存储：WebDAV 密码与 Gist Token 存 Windows 凭据管理器，
//! data.json 里只写哨兵占位，明文不再落盘（评审 M8）。
//! 后端抽象成 trait 以便单测注入内存实现/失败实现。

use crate::models::AppData;

#[cfg(test)]
use std::collections::HashMap;
#[cfg(test)]
use std::sync::Mutex;

/// data.json 中凭据字段的哨兵占位值（非空即代表真实值在凭据后端）
pub const CRED_SENTINEL: &str = "\u{ab}keyring\u{bb}";
const SERVICE: &str = "Prompt Tool";
pub const ACCT_WEBDAV: &str = "webdav-password";
pub const ACCT_GIST: &str = "gist-token";

/// 凭据后端。实现必须线程安全（Store 会被多线程共享）
pub trait CredentialBackend: Send + Sync {
    fn set(&self, account: &str, value: &str) -> Result<(), String>;
    /// None 表示凭据库中没有该条目
    fn get(&self, account: &str) -> Result<Option<String>, String>;
    fn delete(&self, account: &str) -> Result<(), String>;
}

/// 生产实现：OS 凭据库
pub struct KeyringBackend;

impl CredentialBackend for KeyringBackend {
    fn set(&self, account: &str, value: &str) -> Result<(), String> {
        keyring::Entry::new(SERVICE, account)
            .and_then(|e| e.set_password(value))
            .map_err(|e| format!("凭据库写入失败: {e}"))
    }

    fn get(&self, account: &str) -> Result<Option<String>, String> {
        match keyring::Entry::new(SERVICE, account).and_then(|e| e.get_password()) {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(format!("凭据库读取失败: {e}")),
        }
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        match keyring::Entry::new(SERVICE, account).and_then(|e| e.delete_credential()) {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(format!("凭据库删除失败: {e}")),
        }
    }
}

/// 测试实现：内存哈希表，可注入 set 失败（仅测试构建参与编译）
#[cfg(test)]
#[derive(Default)]
pub struct MemoryBackend {
    map: Mutex<HashMap<String, String>>,
    pub fail_sets: std::sync::atomic::AtomicBool,
    pub fail_gets: std::sync::atomic::AtomicBool,
    /// set 成功次数（含迁移路径），用于验证「值未变化跳过重写」
    pub set_count: std::sync::atomic::AtomicUsize,
}

#[cfg(test)]
impl MemoryBackend {
    pub fn value_of(&self, account: &str) -> Option<String> {
        self.map.lock().unwrap().get(account).cloned()
    }
}

#[cfg(test)]
impl CredentialBackend for MemoryBackend {
    fn set(&self, account: &str, value: &str) -> Result<(), String> {
        if self.fail_sets.load(std::sync::atomic::Ordering::SeqCst) {
            return Err("凭据库写入失败（注入的测试故障）".into());
        }
        self.map
            .lock()
            .unwrap()
            .insert(account.to_string(), value.to_string());
        self.set_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    fn get(&self, account: &str) -> Result<Option<String>, String> {
        if self.fail_gets.load(std::sync::atomic::Ordering::SeqCst) {
            return Err("凭据库读取失败（注入的测试故障）".into());
        }
        Ok(self.map.lock().unwrap().get(account).cloned())
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        self.map.lock().unwrap().remove(account);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data_with_creds(webdav: &str, gist: &str) -> AppData {
        let mut d = AppData::default();
        d.settings.webdav.password = webdav.to_string();
        d.settings.gist.token = gist.to_string();
        d
    }

    // ---------- 评审 I1 回归守卫：读取失败不得触发删除 ----------

    #[test]
    fn restore_keeps_sentinel_when_backend_get_fails_and_persist_never_deletes() {
        let backend = MemoryBackend::default();
        backend
            .set(ACCT_WEBDAV, "真密码")
            .expect("预置凭据库真值");
        // 读取故障只影响 get（模拟凭据管理器瞬时不可用），delete 照常可判成功
        backend.fail_gets.store(true, std::sync::atomic::Ordering::SeqCst);

        let mut memo = CredMemo::default();
        let mut data = data_with_creds(CRED_SENTINEL, CRED_SENTINEL);
        restore(&mut data, &backend, &mut memo);

        assert_eq!(
            data.settings.webdav.password, CRED_SENTINEL,
            "读取失败必须保留哨兵原样"
        );

        // 启动保存链路：persist 遇到保留的哨兵必须原样放行
        persist(&mut data, &backend, &mut memo).expect("含保留哨兵的落盘必须成功");

        assert_eq!(
            backend.value_of(ACCT_WEBDAV).as_deref(),
            Some("真密码"),
            "凭据库中仍完好的真值绝不能被删除或覆盖"
        );
        assert_eq!(
            data.settings.webdav.password, CRED_SENTINEL,
            "data.json 侧保持哨兵，不落明文"
        );
    }

    #[test]
    fn restore_clears_field_only_when_backend_confirms_absent() {
        let backend = MemoryBackend::default();
        let mut memo = CredMemo::default();
        let mut data = data_with_creds(CRED_SENTINEL, "");
        restore(&mut data, &backend, &mut memo);
        assert_eq!(
            data.settings.webdav.password, "",
            "后端确认 NoEntry 才允许清空"
        );
    }

    // ---------- 评审 M3：值未变化跳过 keyring 重写 ----------

    #[test]
    fn persist_skips_keyring_write_when_value_unchanged() {
        let backend = MemoryBackend::default();
        let mut memo = CredMemo::default();
        let mut data = data_with_creds("pw", "tok");

        persist(&mut data, &backend, &mut memo).expect("首次落盘成功");
        assert_eq!(
            backend.set_count.load(std::sync::atomic::Ordering::SeqCst),
            2,
            "首次必须真实写入两个账号"
        );
        assert_eq!(data.settings.webdav.password, CRED_SENTINEL);

        // 模拟重启后 restore 还原真值，再连续两次 save：值未变不得重写
        restore(&mut data, &backend, &mut memo);
        persist(&mut data, &backend, &mut memo).unwrap();
        persist(&mut data, &backend, &mut memo).unwrap();
        assert_eq!(
            backend.set_count.load(std::sync::atomic::Ordering::SeqCst),
            2,
            "值未变化的 save 不应再触发 keyring 写入"
        );
        assert_eq!(backend.value_of(ACCT_GIST).as_deref(), Some("tok"));
    }

    #[test]
    fn persist_rewrites_when_value_changes_and_deletes_when_cleared() {
        let backend = MemoryBackend::default();
        let mut memo = CredMemo::default();
        let mut data = data_with_creds("pw", "tok");
        persist(&mut data, &backend, &mut memo).unwrap();

        data.settings.webdav.password = "新密码".to_string();
        data.settings.gist.token = String::new();
        persist(&mut data, &backend, &mut memo).unwrap();

        assert_eq!(backend.value_of(ACCT_WEBDAV).as_deref(), Some("新密码"));
        assert_eq!(
            backend.value_of(ACCT_GIST),
            None,
            "用户清空的凭据必须从凭据库删除"
        );
    }

    // ---------- 既有语义：迁移路径 ----------

    #[test]
    fn restore_migrates_legacy_plaintext_into_backend() {
        let backend = MemoryBackend::default();
        let mut memo = CredMemo::default();
        let mut data = data_with_creds("旧明文", "");
        restore(&mut data, &backend, &mut memo);
        assert_eq!(
            backend.value_of(ACCT_WEBDAV).as_deref(),
            Some("旧明文"),
            "迁移应写入凭据库"
        );
        assert_eq!(
            data.settings.webdav.password, "旧明文",
            "迁移失败之外内存保持明文"
        );
        // 迁移成功后 memo 已记录，紧随的 save 不应重复写入
        persist(&mut data, &backend, &mut memo).unwrap();
        assert_eq!(
            backend.set_count.load(std::sync::atomic::Ordering::SeqCst),
            1
        );
    }

    #[test]
    fn persist_fails_when_backend_set_fails() {
        let backend = MemoryBackend::default();
        backend.fail_sets.store(true, std::sync::atomic::Ordering::SeqCst);
        let mut memo = CredMemo::default();
        let mut data = data_with_creds("pw", "");
        assert!(persist(&mut data, &backend, &mut memo).is_err());
    }
}

fn cred_fields(data: &mut AppData) -> [(&'static str, &mut String); 2] {
    [
        (ACCT_WEBDAV, &mut data.settings.webdav.password),
        (ACCT_GIST, &mut data.settings.gist.token),
    ]
}

/// 各账号最近一次成功写入凭据库的明文值。persist 时与内存真值比对，
/// 未变化则跳过 set——剪贴板捕获每次都触发 save，逐次重写 keyring
/// 既慢又放大凭据后端故障面（评审 M3）
#[derive(Default)]
pub struct CredMemo {
    webdav: Option<String>,
    gist: Option<String>,
}

impl CredMemo {
    fn slot(&mut self, account: &str) -> &mut Option<String> {
        if account == ACCT_WEBDAV {
            &mut self.webdav
        } else {
            &mut self.gist
        }
    }
}

/// 落盘前调用：把凭据写入后端，data 副本中的字段换成哨兵。
/// 任一写入失败即返回 Err，调用方必须放弃本次落盘——
/// 否则磁盘上是哨兵、凭据库里没有值，重启后凭据就丢了
pub fn persist(
    data: &mut AppData,
    backend: &dyn CredentialBackend,
    memo: &mut CredMemo,
) -> Result<(), String> {
    for (account, field) in cred_fields(data) {
        let slot = memo.slot(account);
        if field.is_empty() {
            // 清空的凭据：顺带从凭据库删除（失败忽略，条目残留无泄漏风险）。
            // memo 已是 None 说明此前已删除过，跳过（评审 2026-09-10 M7#7）：
            // 剪贴板捕获每次都 save，无条件 delete 会持续放大凭据库调用面
            if slot.is_some() {
                let _ = backend.delete(account);
            }
            *slot = None;
        } else if *field == CRED_SENTINEL {
            // restore 时凭据库读取失败会保留哨兵：真值仍在凭据库里，
            // 这里既不能把哨兵字符串当真值写进去覆盖它，也不能删除——
            // 原样落盘，等下次启动重试还原（评审 I1）
        } else if slot.as_deref() == Some(field.as_str()) {
            // 值未变：跳过重写，只把载荷字段换成哨兵
            *field = CRED_SENTINEL.to_string();
        } else {
            backend.set(account, field)?;
            *slot = Some(field.clone());
            *field = CRED_SENTINEL.to_string();
        }
    }
    Ok(())
}

/// 载入后调用：哨兵换成凭据库真实值；旧版明文（非哨兵非空）视为
/// 待迁移数据——搬进凭据库，本次内存保持明文，下次 save 起即为哨兵。
/// 凭据库读不到（确认不存在）时清空字段并记日志；读取失败时保留
/// 哨兵原样，绝不清空——那会让随后的启动保存把凭据库里仍完好的
/// 真值删掉（评审 I1）。两种情况都不让启动失败
pub fn restore(
    data: &mut AppData,
    backend: &dyn CredentialBackend,
    memo: &mut CredMemo,
) {
    for (account, field) in cred_fields(data) {
        if *field == CRED_SENTINEL {
            match backend.get(account) {
                Ok(Some(v)) => {
                    *memo.slot(account) = Some(v.clone());
                    *field = v;
                }
                Ok(None) => {
                    eprintln!("[prompt-tool] 凭据库中缺少 {account}，对应凭据已清空，请重新填写");
                    *field = String::new();
                    *memo.slot(account) = None;
                }
                Err(e) => {
                    eprintln!("[prompt-tool] 凭据库读取失败（{account}），保留哨兵待下次启动重试: {e}");
                }
            }
        } else if !field.is_empty() {
            if let Err(e) = backend.set(account, field) {
                // 迁移失败保留内存明文，下次 save 再试；不阻断启动
                eprintln!("[prompt-tool] 旧版明文凭据迁移到凭据库失败（{account}）: {e}");
            } else {
                *memo.slot(account) = Some(field.clone());
            }
        }
    }
}

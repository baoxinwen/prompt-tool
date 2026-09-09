use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tauri::Wry;

/// 变量值记忆文件：<app_data_dir>/var-memory.json
/// 结构：{ [promptId]: { [变量名]: 上次填写的值 } }
/// 个性化数据，独立文件、不参与云同步。
fn file_path(app: &tauri::AppHandle<Wry>) -> PathBuf {
    crate::store::resolve_data_dir(app)
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("var-memory.json")
}

type Memory = BTreeMap<String, PromptMemory>;

const MAX_VALUE_LEN: usize = 4000;
const MAX_PROMPTS: usize = 500;

/// 单个提示词的变量记忆：值表 + 最后一次确认变量窗的时间。
/// saved_at 用于超限淘汰（最久未用先淘汰），文件格式 v2
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
struct PromptMemory {
    values: BTreeMap<String, String>,
    saved_at: u64,
}

/// 读取兼容：v2 为 { values, saved_at }；v1 旧格式直接是值表
/// （{"变量": "值"}），迁移时 saved_at 记 0（视为最旧，随保存自然刷新）。
/// 字段不带默认值保证 v2 匹配失败时才落入 v1，两种形态互不误判
#[derive(serde::Deserialize)]
#[serde(untagged)]
enum PromptMemoryRepr {
    V2 {
        values: BTreeMap<String, String>,
        saved_at: u64,
    },
    V1(BTreeMap<String, String>),
}

impl From<PromptMemoryRepr> for PromptMemory {
    fn from(r: PromptMemoryRepr) -> Self {
        match r {
            PromptMemoryRepr::V2 { values, saved_at } => PromptMemory { values, saved_at },
            PromptMemoryRepr::V1(values) => PromptMemory { values, saved_at: 0 },
        }
    }
}

fn parse_memory(raw: &str) -> Result<Memory, String> {
    let reprs: BTreeMap<String, PromptMemoryRepr> =
        serde_json::from_str(raw).map_err(|e| format!("变量记忆解析失败（文件可能已损坏）: {e}"))?;
    Ok(reprs.into_iter().map(|(k, v)| (k, v.into())).collect())
}

/// save_var_memory 是「读全量 → 改 → 写回」流程，必须串行化：
/// 并发保存时后写者会以自己读到的旧快照覆盖前者的变更（评审 I17）
static MEMORY_LOCK: Mutex<()> = Mutex::new(());

/// 读取记忆。「文件不存在」= 尚无数据（返回空）；
/// 其它读取失败或解析失败必须报错而不是静默归零——
/// 否则 save_var_memory 会以空记忆为底写回，把全部提示词的
/// 记忆静默清空（评审 I17）
fn read_memory_at(path: &Path) -> Result<Memory, String> {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Memory::default()),
        Err(e) => return Err(format!("读取变量记忆失败: {e}")),
    };
    parse_memory(&raw)
}

fn write_memory(path: &Path, mem: &Memory) -> Result<(), String> {
    let tmp = path.with_extension("json.tmp");
    {
        let f = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
        serde_json::to_writer_pretty(&f, mem).map_err(|e| e.to_string())?;
    }
    std::fs::rename(&tmp, path).map_err(|e| format!("保存变量记忆失败: {e}"))
}

/// 单值超限不记忆，避免把大段代码长期固化进文件
fn clean_values(values: BTreeMap<String, String>) -> BTreeMap<String, String> {
    values
        .into_iter()
        .filter(|(_, v)| v.len() <= MAX_VALUE_LEN)
        .collect()
}

/// 超上限时丢弃 saved_at 最旧的条目：saved_at 是最后一次确认变量窗的
/// 时间，越久未用越先淘汰；迁移自旧格式的条目 saved_at=0 视为最旧。
/// 同时间戳按键名决胜负，保证淘汰结果确定（评审 rs-M2：
/// 旧实现按 uuid 键序丢弃，等价于随机扔）
fn cap_prompts(mut mem: Memory) -> Memory {
    while mem.len() > MAX_PROMPTS {
        let victim = mem
            .iter()
            .min_by(|a, b| a.1.saved_at.cmp(&b.1.saved_at).then_with(|| a.0.cmp(b.0)))
            .map(|(k, _)| k.clone());
        let Some(k) = victim else { break };
        mem.remove(&k);
    }
    mem
}

pub fn get_var_memory(app: tauri::AppHandle<Wry>, prompt_id: String) -> BTreeMap<String, String> {
    // 读取是尽力而为：失败（含损坏）按空处理，只影响本次预填，不写回
    read_memory_at(&file_path(&app))
        .ok()
        .and_then(|m| m.get(&prompt_id).cloned())
        .map(|pm| pm.values)
        .unwrap_or_default()
}

pub fn save_var_memory(
    app: tauri::AppHandle<Wry>,
    prompt_id: String,
    values: BTreeMap<String, String>,
) -> Result<(), String> {
    if prompt_id.is_empty() {
        return Ok(());
    }
    save_memory_at(&file_path(&app), prompt_id, values)
}

fn save_memory_at(path: &Path, prompt_id: String, values: BTreeMap<String, String>) -> Result<(), String> {
    let _guard = MEMORY_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    // 现有记忆读不出来（被占用/损坏）时拒绝保存：以空为底写回会
    // 静默清空全部提示词的记忆，且该文件没有备份（评审 I17）
    let mut mem = read_memory_at(path)?;
    mem.insert(
        prompt_id,
        PromptMemory {
            values: clean_values(values),
            saved_at: crate::models::now_ms(),
        },
    );
    let mem = cap_prompts(mem);
    write_memory(path, &mem)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_reads_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        let mem = read_memory_at(&dir.path().join("var-memory.json")).unwrap();
        assert!(mem.is_empty());
    }

    #[test]
    fn unreadable_existing_memory_is_an_error_not_empty() {
        // 文件存在但读不出来（损坏/被锁）≠ 尚无数据：
        // 静默归零会让后续 save 以空为底写回，清空全部记忆（评审 I17）
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("var-memory.json");
        std::fs::write(&path, "not json{{").unwrap();
        let err = read_memory_at(&path).unwrap_err();
        assert!(err.contains("解析失败"), "实际错误: {err}");
    }

    #[test]
    fn save_rejects_instead_of_wiping_when_existing_memory_unreadable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("var-memory.json");
        std::fs::write(&path, "not json{{").unwrap();

        let mut vals = BTreeMap::new();
        vals.insert("name".to_string(), "小明".to_string());
        let err = save_memory_at(&path, "p1".to_string(), vals).unwrap_err();
        assert!(!err.is_empty());
        // 原文件保持原样：绝不能用空记忆覆盖掉（评审 I17）
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "not json{{");
    }

    #[test]
    fn concurrent_saves_do_not_lose_updates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("var-memory.json");
        let mut handles = Vec::new();
        for i in 0..16u32 {
            let p = path.clone();
            handles.push(std::thread::spawn(move || {
                let mut vals = BTreeMap::new();
                vals.insert(format!("k{i}"), "v".to_string());
                save_memory_at(&p, format!("p{i:02}"), vals)
            }));
        }
        for h in handles {
            h.join().unwrap().unwrap();
        }
        let mem = read_memory_at(&path).unwrap();
        assert_eq!(mem.len(), 16, "并发的 16 次保存必须全部保留（评审 I17）");
    }

    #[test]
    fn memory_roundtrip_via_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("var-memory.json");

        let mut mem: Memory = BTreeMap::new();
        let mut vals = BTreeMap::new();
        vals.insert("name".to_string(), "小明".to_string());
        mem.insert("p1".to_string(), PromptMemory { values: vals, saved_at: 1234 });
        write_memory(&path, &mem).unwrap();

        let loaded = read_memory_at(&path).unwrap();
        assert_eq!(loaded["p1"].values["name"], "小明");
        assert_eq!(loaded["p1"].saved_at, 1234);
        let missing = loaded.get("p2").cloned().unwrap_or_default();
        assert!(missing.values.is_empty(), "不存在的提示词返回空记忆");
    }

    #[test]
    fn legacy_v1_format_migrates_with_zero_timestamp() {
        // 旧格式：条目直接是值表。读取必须兼容且 saved_at 记 0（视为最旧）
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("var-memory.json");
        std::fs::write(&path, r#"{"p1":{"name":"小明","city":"上海"}}"#).unwrap();

        let mem = read_memory_at(&path).unwrap();
        assert_eq!(mem["p1"].values["name"], "小明");
        assert_eq!(mem["p1"].values["city"], "上海");
        assert_eq!(mem["p1"].saved_at, 0);

        // 迁移后再保存（模拟用户再次确认变量窗）：落盘为 v2 形态
        save_memory_at(
            &path,
            "p1".to_string(),
            [("name".to_string(), "小红".to_string())].into_iter().collect(),
        )
        .unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.contains("saved_at"), "新写入必须是 v2 形态: {raw}");
        let mem = read_memory_at(&path).unwrap();
        assert_eq!(mem["p1"].values["name"], "小红");
    }

    #[test]
    fn cap_prompts_evicts_oldest_by_saved_at_not_key_order() {
        // 键序与新旧刻意相反：键名最小的反而是最新的。
        // 淘汰必须按 saved_at 最旧，而不是键序靠前（评审 rs-M2 随机丢弃的根因）
        let mut mem: Memory = BTreeMap::new();
        for i in 0..510u32 {
            // k000 最旧(saved_at=1)，k509 最新(saved_at=510)
            let mut vals = BTreeMap::new();
            vals.insert("k".to_string(), "v".to_string());
            mem.insert(
                format!("k{i:03}"),
                PromptMemory { values: vals, saved_at: (i + 1) as u64 },
            );
        }
        let capped = cap_prompts(mem);
        assert_eq!(capped.len(), MAX_PROMPTS);
        // 键序靠前的 k000..k009 恰好也是最旧的 10 条，全部被淘汰；
        // 再验证一个"键序靠前但 saved_at 最新"的构造不会被误删
        assert!(!capped.contains_key("k000"));
        assert!(!capped.contains_key("k009"));
        assert!(capped.contains_key("k509"));

        let mut mem2: Memory = BTreeMap::new();
        for i in 0..510u32 {
            let mut vals = BTreeMap::new();
            vals.insert("k".to_string(), "v".to_string());
            // 反转：k000 最新(saved_at=510)，k509 最旧(saved_at=1)
            mem2.insert(
                format!("k{i:03}"),
                PromptMemory { values: vals, saved_at: (510 - i) as u64 },
            );
        }
        let capped2 = cap_prompts(mem2);
        assert!(capped2.contains_key("k000"), "键序靠前但最新使用必须保留");
        assert!(!capped2.contains_key("k509"), "键序靠后但最久未用必须淘汰");
        assert!(capped2.contains_key("k300"), "迁移条目之外的中位条目保留");
    }

    #[test]
    fn clean_values_drops_oversized_but_keeps_normal() {
        let mut values = BTreeMap::new();
        values.insert("ok".to_string(), "短值".to_string());
        values.insert("huge".to_string(), "x".repeat(MAX_VALUE_LEN + 1));
        values.insert("edge".to_string(), "y".repeat(MAX_VALUE_LEN));
        let cleaned = clean_values(values);
        assert!(!cleaned.contains_key("huge"), "超过上限的值必须被丢弃");
        assert!(cleaned.contains_key("ok"));
        assert!(cleaned.contains_key("edge"), "恰好等于上限的值保留");
    }

    #[test]
    fn cap_prompts_noop_under_limit() {
        let mut mem: Memory = BTreeMap::new();
        mem.insert(
            "only".to_string(),
            PromptMemory { values: BTreeMap::new(), saved_at: 1 },
        );
        let capped = cap_prompts(mem);
        assert_eq!(capped.len(), 1);
    }
}

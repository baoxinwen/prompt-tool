use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Runtime};

use crate::models::{new_id, now_ms, ClipboardItem, ImageRef, MAX_CLIPBOARD_ITEMS};
use crate::store::{lock, Store};

/// 剪贴板监听轮询周期（毫秒）。各写入路径的抑制窗口由它派生，
/// 改动此值会同步改变全部抑制窗口
pub const POLL_INTERVAL_MS: u64 = 700;
pub const POLL_INTERVAL: Duration = Duration::from_millis(POLL_INTERVAL_MS);

/// 剪贴板抑制窗口：程序写入剪贴板后至少跨过一个完整轮询周期再加余量，
/// 让监听线程把本次写入吸收进基线后才解除抑制，否则程序自写的内容会被
/// 记入剪贴板历史。全部写入路径（复制/文本粘贴/图片粘贴/快速捕获）的
/// 后台收尾必须共用这一个常量
pub const SUPPRESS_WINDOW: Duration = Duration::from_millis(POLL_INTERVAL_MS + 100);

const MAX_IMAGES: usize = 50;

/// 单条文本记录的字节上限（评审 I-11）：超大复制（IDE 全选/日志工具输出
/// 数 MB）会让 data.json 暴涨同等体量，其后每次全量序列化+fsync 的成本
/// 线性放大。图片路径有尺寸/数量上限、变量记忆有 4000 字符上限，文本
/// 此前无对应物。超限拒收（历史本就是「最近的」语义，非归档）
pub const MAX_TEXT_RECORD_BYTES: usize = 1024 * 1024;

/// 序号预检：序号可用且与上次相同 ⇒ 内容未变，跳过读取；序号不可用的
/// 平台退回全量采样（始终探测）。抽成纯函数便于直接测试（评审 I-10）
fn seq_needs_probe(current: Option<u32>, last: Option<u32>) -> bool {
    match current {
        Some(c) => last != Some(c),
        None => true,
    }
}

/// 剪贴板变化序列号：Win32 GetClipboardSequenceNumber 只在剪贴板内容
/// 变化时递增，读取它无需打开剪贴板、不拷贝内容。纯图片剪贴板若每个
/// 700ms 周期都全量 get_image（4K 图约 33MB），会持续消耗 CPU 与内存
/// 带宽（评审 I16），必须先查序号再决定是否全量读
fn clipboard_sequence() -> Option<u32> {
    use windows_sys::Win32::System::DataExchange::GetClipboardSequenceNumber;
    // 返回 0 表示调用失败（无剪贴板支持），视为不可用
    match unsafe { GetClipboardSequenceNumber() } {
        0 => None,
        n => Some(n),
    }
}

/// 变化判定哈希：全量哈希整张 RGBA（评审 2026-09-10 images#12）。
/// 旧实现的 5 段采样会让「改动恰落在采样缝隙中」的两张图被判同一张，
/// 漏记历史；而序号预检（clipboard_sequence）已保证哈希只在剪贴板
/// 真正变化时执行一次，全量哈希的成本（33MB 约几十毫秒、每事件一次）
/// 可以接受，不再值得为采样引入漏判窗口
fn image_hash(width: usize, height: usize, data: &[u8]) -> u64 {
    let mut h = DefaultHasher::new();
    width.hash(&mut h);
    height.hash(&mut h);
    data.len().hash(&mut h);
    data.hash(&mut h);
    h.finish()
}

/// 后台轮询剪贴板，把系统内新复制的内容记入历史（文本与图片）
pub fn spawn<R: Runtime + 'static>(app: AppHandle<R>) {
    std::thread::spawn(move || {
        let Ok(mut board) = arboard::Clipboard::new() else {
            eprintln!("[clipboard] 无法打开剪贴板，历史记录不可用");
            return;
        };
        // 启动时先采样基线，避免把用户已有的剪贴板内容记进来
        let mut last_text: Option<String> = board.get_text().ok();
        let mut last_image: Option<u64> = board
            .get_image()
            .ok()
            .map(|i| image_hash(i.width, i.height, &i.bytes));
        let mut last_seq = clipboard_sequence();

        loop {
            std::thread::sleep(POLL_INTERVAL);

            // 序号预检（评审 I16）：序号未变 ⇒ 内容未变，跳过本周期全部读取。
            // 序号只在读取成功后推进（评审 I-10）：序号是「已见内容」的标记，
            // 读取失败（剪贴板被截图工具等进程毫秒级独占）的那次变化若提前
            // 消费序号，该内容永久漏记——下周期序号未变直接跳过
            let current_seq = clipboard_sequence();
            if !seq_needs_probe(current_seq, last_seq) {
                continue;
            }

            // 文本优先；剪贴板是图片时 get_text 会失败
            if let Ok(text) = board.get_text() {
                last_seq = current_seq;
                if text.is_empty() || last_text.as_deref() == Some(text.as_str()) {
                    continue;
                }
                last_text = Some(text.clone());
                if record_text(&app, text) {
                    let _ = app.emit("data-changed", ());
                }
                continue;
            }

            if let Ok(img) = board.get_image() {
                last_seq = current_seq;
                let hash = image_hash(img.width, img.height, &img.bytes);
                if last_image == Some(hash) {
                    continue;
                }
                last_image = Some(hash);
                if record_image(&app, img.width as u32, img.height as u32, &img.bytes) {
                    let _ = app.emit("data-changed", ());
                }
            }
            // 两条读取都失败：不推进 last_seq，下个周期重试该变化
        }
    });
}

/// 记录门控（评审 2026-09-10 I20）：抑制中或用户关闭记录开关时不得记录。
/// 抽成纯函数便于直接测试
fn should_record(suppress_clipboard: bool, capture_clipboard: bool) -> bool {
    !suppress_clipboard && capture_clipboard
}

/// 文本条目入库（去重后插到顶部 + 截断 + 落盘）。从 record_text 抽出：
/// 抑制/开关快照在锁外完成，重活（落盘）与插入在同一临界区，逻辑可独立测试
fn insert_text_record(store: &mut Store, text: String) -> bool {
    if text.len() > MAX_TEXT_RECORD_BYTES {
        eprintln!(
            "[clipboard] 文本超 {} 字节上限，不入历史",
            MAX_TEXT_RECORD_BYTES
        );
        return false;
    }
    let now = now_ms();
    store.data.clipboard.retain(|i| i.content != text);
    store.data.clipboard.insert(
        0,
        ClipboardItem {
            id: new_id(),
            content: text,
            copied_at: now,
            kind: "text".into(),
            image: None,
        },
    );
    store.data.clipboard.truncate(MAX_CLIPBOARD_ITEMS);
    if let Err(e) = store.save() {
        eprintln!("[clipboard] 剪贴板文本落盘失败，重启后将丢失该条目: {e}");
        return false;
    }
    true
}

fn record_text<R: Runtime>(app: &AppHandle<R>, text: String) -> bool {
    // 门控快照在锁外读取（评审 2026-09-10 I20：锁持有时间最小化）
    let (suppress, capture_on) = {
        let store = lock(app);
        (store.suppress_clipboard, store.data.settings.capture_clipboard)
    };
    if !should_record(suppress, capture_on) {
        return false;
    }
    let mut store = lock(app);
    // 复检（评审 M9，对齐 record_image 的同款防线）：快照放锁后若被锁竞争
    // 阻塞超过抑制窗口，期间粘贴会话已完整走完并解除抑制，迟到的文本不得入库
    if !should_record(store.suppress_clipboard, store.data.settings.capture_clipboard) {
        return false;
    }
    insert_text_record(&mut store, text)
}

fn record_image<R: Runtime>(app: &AppHandle<R>, width: u32, height: u32, rgba: &[u8]) -> bool {
    // 快照抑制/开关状态后立即放锁（评审 2026-09-10 I20）：PNG 编码（4K 图
    // 数百 ms）绝不能在持有全局 Store 锁的状态下做，否则监听线程持锁期间
    // 所有走 lock() 的命令（get_data/粘贴/设置保存）排队阻塞，界面卡顿
    let (suppressed, capture_on) = {
        let store = lock(app);
        (
            store.suppress_clipboard,
            store.data.settings.capture_clipboard,
        )
    };
    if suppressed || !capture_on {
        return false;
    }
    let id = new_id();
    let Ok(file) = crate::images::save_png(app, &id, width, height, rgba) else {
        eprintln!("[clipboard] 保存图片失败");
        return false;
    };

    let mut store = lock(app);
    // 重新校验：编码期间可能有新的粘贴/复制会话开启了抑制，迟到的图片不入库
    if store.suppress_clipboard || !store.data.settings.capture_clipboard {
        crate::images::delete_files(app, &id);
        return false;
    }
    store.data.clipboard.insert(
        0,
        ClipboardItem {
            id: id.clone(),
            content: String::new(),
            copied_at: now_ms(),
            kind: "image".into(),
            image: Some(ImageRef { file, width, height }),
        },
    );
    store.data.clipboard.truncate(MAX_CLIPBOARD_ITEMS);

    // 图片条目单独限数，超出删最旧（含磁盘文件）
    let image_count = store.data.clipboard.iter().filter(|i| i.is_image()).count();
    if image_count > MAX_IMAGES {
        let oldest = store
            .data
            .clipboard
            .iter()
            .filter(|i| i.is_image())
            .min_by_key(|i| i.copied_at)
            .map(|i| i.id.clone());
        if let Some(old_id) = oldest {
            crate::images::delete_files(app, &old_id);
            store.data.clipboard.retain(|i| i.id != old_id);
        }
    }
    if let Err(e) = store.save() {
        eprintln!("[clipboard] 剪贴板图片落盘失败，重启后将丢失该条目: {e}");
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::MAX_CLIPBOARD_ITEMS;
    use crate::store::Store;

    // 评审 2026-09-10 images#12：全量哈希后，落在旧采样窗口缝隙之外
    // 的内容变化也必须被识别（旧采样实现会误判为同一张而漏记）
    #[test]
    fn image_hash_detects_changes_outside_sampled_windows() {
        let n = 4096;
        let a = vec![0u8; n];
        let mut b = a.clone();
        b[n / 2 + 200] = 1; // 不落在旧实现的 5 个 64 字节采样窗口（0/¼/½/¾/尾）内
        let ha = image_hash(100, 100, &a);
        let hb = image_hash(100, 100, &b);
        assert_ne!(ha, hb, "采样缝隙内的内容变化不得漏判");
        assert_eq!(ha, image_hash(100, 100, &a), "同内容哈希必须稳定");
    }

    // 评审 2026-09-10 I20：记录门控真值表——抑制中或关闭记录开关都不得记录
    #[test]
    fn should_record_gate_truth_table() {
        assert!(!should_record(true, true), "抑制中不得记录");
        assert!(!should_record(false, false), "关闭记录开关不得记录");
        assert!(!should_record(true, false));
        assert!(should_record(false, true));
    }

    // 评审 2026-09-10 I20：文本入库的去重/截断/落盘语义锁定
    #[test]
    fn insert_text_record_dedupes_moves_to_top_and_truncates() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::test_store(dir.path());
        for i in 0..(MAX_CLIPBOARD_ITEMS + 5) {
            let _ = insert_text_record(&mut store, format!("第{i}条"), );
        }
        assert_eq!(
            store.data.clipboard.len(),
            MAX_CLIPBOARD_ITEMS,
            "超出上限必须截断"
        );
        assert_eq!(store.data.clipboard[0].content, "第1004条", "最新条目在顶部");

        // 重复内容：去重并重新置顶，不得出现两条
        assert!(insert_text_record(&mut store, "第1004条".into()));
        let dupes = store
            .data
            .clipboard
            .iter()
            .filter(|i| i.content == "第1004条")
            .count();
        assert_eq!(dupes, 1, "重复内容必须去重");
        assert_eq!(store.data.clipboard[0].content, "第1004条");
    }

    #[test]
    fn insert_text_record_persists_to_disk() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::test_store(dir.path());
        assert!(insert_text_record(&mut store, "落盘".into()));
        assert!(dir.path().join("data.json").exists(), "入库必须落盘");
    }

    // 评审 I-11：单条文本此前无字节上限，一次超大复制（IDE 全选/日志工具）
    // 会让 data.json 暴涨并在其后每次全量落盘时线性放大；数量上限（1000 条）
    // 与字节上限是正交维度
    #[test]
    fn insert_text_record_rejects_oversized_text() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::test_store(dir.path());
        let huge = "x".repeat(MAX_TEXT_RECORD_BYTES + 1);
        assert!(!insert_text_record(&mut store, huge), "超上限文本必须拒收");
        assert!(store.data.clipboard.is_empty(), "拒收不得入库");

        // 恰好等于上限：可收（边界含等于）
        let edge = "y".repeat(MAX_TEXT_RECORD_BYTES);
        assert!(insert_text_record(&mut store, edge), "等于上限的文本应收录");
        assert_eq!(store.data.clipboard.len(), 1);
    }

    // 评审 I-10：序号是「已见内容」的标记，不是「已见变化」——读取失败的那次
    // 变化若提前消费序号，该内容永久漏记（下周期序号未变直接跳过）
    #[test]
    fn seq_needs_probe_truth_table() {
        // 序号不变：跳过
        assert!(!seq_needs_probe(Some(101), Some(101)));
        // 序号前进：探测
        assert!(seq_needs_probe(Some(102), Some(101)));
        // 首次采样（last 为 None）：探测
        assert!(seq_needs_probe(Some(101), None));
        // 序号不可用的平台：退回全量采样，始终探测
        assert!(seq_needs_probe(None, Some(101)));
        assert!(seq_needs_probe(None, None));
    }
}

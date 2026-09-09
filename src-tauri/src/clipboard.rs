use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::Duration;

use tauri::{AppHandle, Emitter};

use crate::models::{new_id, now_ms, ClipboardItem, ImageRef, MAX_CLIPBOARD_ITEMS};
use crate::store::lock;

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

/// 多点采样哈希：只看首尾 32 字节时，中部内容不同的两张图（如仅改中部的截图）
/// 会被误判为同一张而漏记历史
fn image_hash(width: usize, height: usize, data: &[u8]) -> u64 {
    let mut h = DefaultHasher::new();
    width.hash(&mut h);
    height.hash(&mut h);
    data.len().hash(&mut h);
    let n = data.len();
    if n <= 192 {
        data.hash(&mut h);
    } else {
        // 取首/1/4/中/3/4/尾五段各 64 字节
        for start in [0, n / 4, n / 2, 3 * n / 4, n - 64] {
            data[start..start + 64].hash(&mut h);
        }
    }
    h.finish()
}

/// 后台轮询剪贴板，把系统内新复制的内容记入历史（文本与图片）
pub fn spawn(app: AppHandle) {
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

            // 序号未变 ⇒ 内容未变，跳过本周期全部读取（评审 I16）。
            // 序号不可用的平台退回原全量采样行为
            if let Some(seq) = clipboard_sequence() {
                if last_seq == Some(seq) {
                    continue;
                }
                last_seq = Some(seq);
            }

            // 文本优先；剪贴板是图片时 get_text 会失败
            if let Ok(text) = board.get_text() {
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
                let hash = image_hash(img.width, img.height, &img.bytes);
                if last_image == Some(hash) {
                    continue;
                }
                last_image = Some(hash);
                if record_image(&app, img.width as u32, img.height as u32, &img.bytes) {
                    let _ = app.emit("data-changed", ());
                }
            }
        }
    });
}

fn record_text(app: &AppHandle, text: String) -> bool {
    let mut store = lock(app);
    if store.suppress_clipboard || !store.data.settings.capture_clipboard {
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

fn record_image(app: &AppHandle, width: u32, height: u32, rgba: &[u8]) -> bool {
    let mut store = lock(app);
    if store.suppress_clipboard || !store.data.settings.capture_clipboard {
        return false;
    }
    let id = new_id();
    let Ok(file) = crate::images::save_png(app, &id, width, height, rgba) else {
        eprintln!("[clipboard] 保存图片失败");
        return false;
    };

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

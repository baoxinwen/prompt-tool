use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, PhysicalPosition};

/// 捕获流程在途标志：从触发到窗口显示约 280ms（等待 Ctrl+C 生效），
/// 期间窗口仍不可见，仅靠 is_visible 挡不住快速二次触发
static CAPTURE_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

/// 快速捕获：模拟 Ctrl+C 抓取当前选中文本，弹出小窗让用户保存为新提示词
pub fn start(app: &AppHandle) {
    // 已在捕获流程中（窗口已显示）时忽略重复触发
    if let Some(win) = app.get_webview_window("capture") {
        if win.is_visible().unwrap_or(false) {
            return;
        }
    }
    // 复制尚在飞行中的二次触发同样忽略：此时剪贴板里已是上一次抓到的
    // 选中文本，second pass 会得到 latest == original 而捕获到空内容
    if CAPTURE_IN_FLIGHT.swap(true, Ordering::SeqCst) {
        return;
    }

    let original = crate::paste::get_clipboard_text();
    let generation = {
        let mut store = crate::store::lock(app);
        store.suppress_clipboard = true;
        // 捕获写入剪贴板，使仍在飞行的粘贴会话收尾失效（同粘贴互踩的根因）
        store.paste_generation += 1;
        store.paste_generation
    };

    // 等物理修饰键释放：热键回调在按下瞬间触发，物理 Alt 未释放时
    // 注入的 Ctrl+C 会被叠加成 Alt+Ctrl+C，目标应用不当作复制（评审 I14）
    crate::paste::wait_modifiers_released(Duration::from_millis(400));
    crate::paste::press_ctrl_c();

    let handle = app.clone();
    std::thread::spawn(move || {
        // 等目标应用响应 Ctrl+C 并写入剪贴板
        std::thread::sleep(Duration::from_millis(280));
        let latest = crate::paste::get_clipboard_text();
        // 剪贴板没变化 → 没有选中内容，打开空窗口让用户手输
        let selected = match (&latest, &original) {
            (Some(t), Some(o)) if t == o => String::new(),
            (Some(t), _) => t.clone(),
            _ => String::new(),
        };

        show_capture_window(&handle);
        // 窗口已显示：后续重入交给 is_visible 守卫
        CAPTURE_IN_FLIGHT.store(false, Ordering::SeqCst);
        let _ = handle.emit("capture-text", selected);

        // 代际已变：期间用户又发起了复制/粘贴/新捕获，本会话不得再恢复
        // 原剪贴板（会覆盖新会话写入的内容）或解除抑制
        if crate::store::lock(&handle).paste_generation != generation {
            return;
        }
        // 恢复原剪贴板（原内容是图片等非文本时保持捕获文本不动）
        if let Some(o) = original {
            let _ = crate::paste::set_clipboard_text(&o);
        }
        // 保持 suppress 跨过一个完整轮询周期：监听线程会先采样一次、
        // 把捕获/恢复产生的剪贴板变化吸收进基线。若立即解除抑制，
        // 恢复的原内容会被当成“新复制”重复记入历史；原内容是图片时
        // 残留的捕获文本也会被意外收进历史
        std::thread::sleep(crate::clipboard::SUPPRESS_WINDOW);
        crate::store::lock(&handle).release_suppress_if_current(generation);
    });
}

fn show_capture_window(app: &AppHandle) {
    let Some(win) = app.get_webview_window("capture") else {
        return;
    };
    let _ = win.unminimize();
    // 显示在光标所在屏幕的中间偏上位置
    if let Ok(pos) = app.cursor_position() {
        if let Ok(Some(monitor)) = win.monitor_from_point(pos.x, pos.y) {
            let size = win.outer_size().unwrap_or(tauri::PhysicalSize::new(340, 300));
            let mp = monitor.position();
            let ms = monitor.size();
            let x = (mp.x + (ms.width as i32 - size.width as i32) / 2).max(mp.x + 8);
            let y = (mp.y + (ms.height as i32 - size.height as i32) / 3).max(mp.y + 8);
            let _ = win.set_position(PhysicalPosition::new(x, y));
        }
    }
    let _ = win.show();
    let _ = win.set_focus();
}

/// 关闭（隐藏）捕获窗口
pub fn hide(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("capture") {
        let _ = win.hide();
    }
}

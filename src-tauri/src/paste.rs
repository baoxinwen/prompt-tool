use std::time::Duration;

/// 模拟粘贴/复制/回车按键：SendInput(Ctrl+V)。
/// 本项目仅支持 Windows，按键注入直接使用 Win32 API（不保留跨平台桩）
mod keys {
    /// 返回是否全部注入成功：SendInput 返回实际注入的事件数，被系统拒绝
    /// （目标以管理员运行触发 UIPI、安全软件拦截）时少于请求数。静默吞掉
    /// 会让粘贴整体无效果且无任何提示（评审 2026-09-10 I21）
    fn press_combo(vk_modifier: u16, vk_key: u16) -> bool {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
        };

        fn key(vk: u16, keyup: bool) -> INPUT {
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: vk,
                        wScan: 0,
                        dwFlags: if keyup { KEYEVENTF_KEYUP } else { 0 },
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            }
        }

        let inputs = [
            key(vk_modifier, false),
            key(vk_key, false),
            key(vk_key, true),
            key(vk_modifier, true),
        ];
        let sent = unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_ptr(),
                std::mem::size_of::<INPUT>() as i32,
            )
        };
        sent == inputs.len() as u32
    }

    pub fn press_paste() -> bool {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_V;
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_CONTROL;
        press_combo(VK_CONTROL, VK_V)
    }

    /// 单键模拟（无修饰键），用于粘贴后追加回车。
    /// keybd_event 无返回值可查，无法检测失败（Win32 API 限制）
    pub fn press_enter() -> bool {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_RETURN;
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
            keybd_event, KEYEVENTF_KEYUP,
        };
        unsafe {
            keybd_event(VK_RETURN as u8, 0, 0, 0);
            keybd_event(VK_RETURN as u8, 0, KEYEVENTF_KEYUP, 0);
        }
        true
    }

    pub fn press_copy() -> bool {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_C;
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_CONTROL;
        press_combo(VK_CONTROL, VK_C)
    }
}

pub fn press_ctrl_v() -> bool {
    keys::press_paste()
}

pub fn press_enter() -> bool {
    keys::press_enter()
}

pub fn press_ctrl_c() -> bool {
    keys::press_copy()
}

/// 等待用户物理按住的修饰键（Alt/Ctrl/Shift）释放，最多等 timeout。
/// 热键回调在按下瞬间即触发，用户"按住稍久"的习惯会让注入的 Ctrl+C/V
/// 叠加物理 Alt 变成 Alt+Ctrl+C/V，目标应用不当作复制/粘贴处理（评审 I14）。
/// 超时未释放也返回（尽力而为，不无限阻塞捕获/粘贴流程）
pub fn wait_modifiers_released(timeout: Duration) -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_CONTROL, VK_MENU, VK_SHIFT,
    };
    const PRESSED: u16 = 0x8000;
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let held = unsafe {
            [VK_MENU, VK_CONTROL, VK_SHIFT]
                .iter()
                .any(|&vk| (GetAsyncKeyState(vk as i32) as u16) & PRESSED != 0)
        };
        if !held {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// 呼出面板前的前台窗口，粘贴时唤回它，保证粘贴落点正确。
pub mod foreground {
    use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{keybd_event, KEYEVENTF_KEYUP, VK_MENU};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId, SetForegroundWindow,
    };

    pub fn current() -> Option<isize> {
        let hwnd = unsafe { GetForegroundWindow() };
        (!hwnd.is_null()).then_some(hwnd as isize)
    }

    /// 目标窗口当前是否处于前台（发送 Ctrl+V 前校验，防止误粘到别的窗口）
    pub fn is_foreground(hwnd: isize) -> bool {
        let fore = unsafe { GetForegroundWindow() };
        !fore.is_null() && fore as isize == hwnd
    }

    /// 恢复前台窗口。Windows 对后台进程有前台锁定限制，
    /// 通过短暂的 AttachThreadInput 共享输入状态绕过。
    pub fn restore(hwnd: isize) -> bool {
        unsafe {
            let fore = GetForegroundWindow();
            let this_thread = GetCurrentThreadId();
            let fore_thread = if fore.is_null() {
                0
            } else {
                GetWindowThreadProcessId(fore, std::ptr::null_mut())
            };

            // ALT 键击让系统认为有用户输入，解除部分前台锁定
            keybd_event(VK_MENU as u8, 0, 0, 0);
            keybd_event(VK_MENU as u8, 0, KEYEVENTF_KEYUP, 0);

            let attached = fore_thread != 0 && fore_thread != this_thread;
            if attached {
                AttachThreadInput(this_thread, fore_thread, 1);
            }
            let ok = SetForegroundWindow(hwnd as _) != 0;
            if attached {
                AttachThreadInput(this_thread, fore_thread, 0);
            }
            ok
        }
    }
}

/// SendInput 被系统拒绝（目标以管理员运行触发 UIPI、安全软件拦截）时的
/// 用户可操作提示：此时面板已隐藏、剪贴板已被提示词内容覆盖，静默失败
/// 等于用户操作整体丢失（评审 2026-09-10 I21）
const PASTE_INJECTED_ERR: &str =
    "粘贴失败：目标窗口拒绝模拟按键（该应用可能以管理员权限运行）。请以相同权限重启本应用后重试";

pub fn set_clipboard_text(text: &str) -> Result<(), String> {
    arboard::Clipboard::new()
        .and_then(|mut c| c.set_text(text.to_string()))
        .map_err(|e| format!("写入剪贴板失败: {e}"))
}

pub fn get_clipboard_text() -> Option<String> {
    arboard::Clipboard::new().and_then(|mut c| c.get_text()).ok()
}

/// 恢复目标窗口焦点并模拟粘贴（可选追加回车）。
/// Windows 下在发送前校验目标确实回到前台，失败重试一次，仍失败则放弃——
/// 固定延时无法保证焦点切换成功，盲发会把内容粘进恰好在前台的其他应用。
pub fn send_paste(target: Option<isize>, append_enter: bool) -> Result<(), String> {
    match target {
        Some(hwnd) => {
            std::thread::sleep(Duration::from_millis(60));
            foreground::restore(hwnd);
            std::thread::sleep(Duration::from_millis(200));
            if !foreground::is_foreground(hwnd) {
                foreground::restore(hwnd);
                std::thread::sleep(Duration::from_millis(200));
            }
            if !foreground::is_foreground(hwnd) {
                eprintln!("[prompt-tool] 粘贴目标未回到前台，已取消按键以防误粘");
                return Ok(()); // 主动取消不视为注入失败
            }
            // 等物理修饰键释放，防止注入组合被叠加成 Alt+Ctrl+V 等（评审 I14）。
            // 等待期间（最多 400ms）焦点可能被切走，注入前复检一次（M8#13）
            wait_modifiers_released(Duration::from_millis(400));
            if !foreground::is_foreground(hwnd) {
                eprintln!("[prompt-tool] 粘贴目标未回到前台，已取消按键以防误粘");
                return Ok(());
            }
            if !press_ctrl_v() {
                return Err(PASTE_INJECTED_ERR.to_string());
            }
            if append_enter {
                std::thread::sleep(Duration::from_millis(60));
                press_enter();
            }
        }
        None => {
            std::thread::sleep(Duration::from_millis(200));
            wait_modifiers_released(Duration::from_millis(400));
            if !press_ctrl_v() {
                return Err(PASTE_INJECTED_ERR.to_string());
            }
            if append_enter {
                std::thread::sleep(Duration::from_millis(60));
                press_enter();
            }
        }
    }
    Ok(())
}

/// 后台粘贴核心：写入剪贴板（含恢复准备）→ 唤回目标窗口 → 模拟粘贴 → 按设置恢复原剪贴板。
/// 调用方负责在需要时先行隐藏窗口 / 记录 paste_target。
pub fn paste_text(app: &tauri::AppHandle, text: &str) -> Result<(), String> {
    let (restore_clipboard, append_enter, generation) = {
        let mut store = crate::store::lock(app);
        store.suppress_clipboard = true;
        // 开启新会话：让更早的粘贴会话的后台收尾（恢复剪贴板/解除抑制）失效，
        // 否则快速连续两次粘贴时旧会话会覆盖新会话写入的内容
        store.paste_generation += 1;
        (
            store.data.settings.restore_clipboard,
            store.data.settings.paste_append_enter,
            store.paste_generation,
        )
    };

    // 保存原剪贴板文本（原内容是图片等非文本时无法恢复，置 None）
    let original = if restore_clipboard { get_clipboard_text() } else { None };

    // 写入失败必须解除抑制，否则剪贴板历史从此静默失效直到重启
    if let Err(e) = set_clipboard_text(text) {
        crate::store::lock(app).suppress_clipboard = false;
        return Err(e);
    }

    let handle = app.clone();
    std::thread::spawn(move || {
        // 唤回呼出面板前的前台窗口（防止中间焦点被其他窗口抢走）
        let target = crate::store::lock(&handle).paste_target.take();
        if let Err(e) = send_paste(target, append_enter) {
            // 注入失败发生在异步收尾线程：调用方早已收到 Ok，只能经事件
            // 通道把错误送到前端 toast（Manager 监听 paste-failed）
            eprintln!("[prompt-tool] {e}");
            let _ = tauri::Emitter::emit(&handle, "paste-failed", e);
        }
        // 代际已变：期间用户又发起了一次粘贴/复制，本会话不得再动剪贴板或解除抑制
        if crate::store::lock(&handle).paste_generation != generation {
            return;
        }
        // 留出目标应用处理按键的时间，再恢复原剪贴板（过早恢复会粘错内容）；
        // 之后统一保持 suppress 跨过一个剪贴板轮询周期，让监听线程把
        // 本次粘贴/恢复动作吸收进基线，避免把程序写入的内容误记进历史
        std::thread::sleep(Duration::from_millis(300));
        if let Some(orig) = original.filter(|o| !o.is_empty()) {
            let _ = arboard::Clipboard::new().and_then(|mut c| c.set_text(orig));
        }
        std::thread::sleep(crate::clipboard::SUPPRESS_WINDOW);
        crate::store::lock(&handle).release_suppress_if_current(generation);
    });

    Ok(())
}

/// 面板粘贴：先写剪贴板（失败立即返回，面板保持可见、错误 toast 才能被
/// 用户看到），成功后才隐藏面板让焦点回到上一个窗口，然后后台粘贴。
/// 旧实现「先隐藏再写」，写失败时 toast 渲染在已隐藏的窗口上，
/// 操作静默整体丢失，且与图片粘贴路径（成功才 hide）不一致（评审 I1）
pub fn paste_to_previous_window(
    window: &tauri::WebviewWindow,
    app: &tauri::AppHandle,
    text: &str,
) -> Result<(), String> {
    let result = paste_text(app, text);
    if result.is_ok() {
        let _ = window.hide();
    }
    result
}

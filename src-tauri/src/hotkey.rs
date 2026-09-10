use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use crate::models::{now_ms, Prompt};

/// 统一注册全部全局快捷键：面板主键 + 快速捕获键 + 各提示词的独立快捷键。
/// 任一注册失败只记录日志（其余键继续生效），失败的加速键（已归一化）
/// 以返回值反馈，供 save_prompt/save_settings 判断刚保存的键是否生效。
pub fn register_all(app: &AppHandle) -> Vec<String> {
    let mut failed: Vec<String> = Vec::new();
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();

    let (main_hotkey, capture_hotkey) = {
        let store = crate::store::lock(app);
        (
            store.data.settings.hotkey.clone(),
            store.data.settings.capture_hotkey.clone(),
        )
    };

    // 空主键不注册：空 accelerator 每次启动必然注册失败，徒增报错噪音（M7#11）
    if !main_hotkey.trim().is_empty() {
        if let Err(e) = gs.on_shortcut(normalize(&main_hotkey).as_str(), |app, _s, event| {
            if event.state() == ShortcutState::Pressed {
                toggle_quick_window(app);
            }
        }) {
            eprintln!("[prompt-tool] 注册主快捷键 {} 失败: {e}", normalize(&main_hotkey));
            failed.push(normalize(&main_hotkey));
        }
    }

    if !capture_hotkey.trim().is_empty() {
        let accel = normalize(&capture_hotkey);
        if let Err(e) = gs.on_shortcut(accel.as_str(), |app, _s, event| {
            if event.state() == ShortcutState::Pressed {
                // 热键回调在主线程同步执行（评审 2026-09-10 I18）：capture::start
                // 里等待修饰键释放（≤400ms）属重活，内联会卡住整个消息泵
                let app = app.clone();
                std::thread::spawn(move || crate::capture::start(&app));
            }
        }) {
            eprintln!("[prompt-tool] 注册捕获快捷键 {accel} 失败: {e}");
            failed.push(accel);
        }
    }

    let hotkeyed: Vec<(String, String)> = {
        let store = crate::store::lock(app);
        store
            .data
            .prompts
            .iter()
            .filter(|p| !p.hotkey.trim().is_empty())
            .map(|p| (normalize(&p.hotkey), p.id.clone()))
            .collect()
    };
    for (accel, id) in hotkeyed {
        if let Err(e) = gs.on_shortcut(accel.as_str(), move |app, _s, event| {
            if event.state() == ShortcutState::Pressed {
                trigger_prompt(app, &id);
            }
        }) {
            eprintln!("[prompt-tool] 注册提示词快捷键 {accel} 失败（可能被占用或重复）: {e}");
            failed.push(accel);
        }
    }
    failed
}

/// 触发提示词：无变量直接后台粘贴；有变量则呼出面板并让前端弹出变量填写窗。
/// 热键回调在主线程同步执行（评审 2026-09-10 I18）：使用统计要落盘
/// （fsync+.bak+rename，慢盘/杀软扫描时数十 ms），必须移到工作线程，
/// 否则整个消息泵与后续热键事件在落盘期间全部停顿
pub fn trigger_prompt(app: &AppHandle, id: &str) {
    let app = app.clone();
    let id = id.to_string();
    std::thread::spawn(move || trigger_prompt_impl(&app, &id));
}

fn trigger_prompt_impl(app: &AppHandle, id: &str) {
    let prompt: Option<Prompt> = {
        let store = crate::store::lock(app);
        store.data.prompts.iter().find(|p| p.id == id).cloned()
    };
    let Some(p) = prompt else { return };

    if has_vars(&p.content) {
        show_quick_window(app);
        let _ = app.emit("open-prompt", p.id);
        return;
    }

    // 无变量：记录前台窗口后直接粘贴（面板保持隐藏）
    {
        let fg = crate::paste::foreground::current();
        let mut store = crate::store::lock(app);
        store.paste_target = fg;
    }
    {
        let mut store = crate::store::lock(app);
        let _ = store.mutate(|d| {
            if let Some(x) = d.prompts.iter_mut().find(|x| x.id == id) {
                x.use_count += 1;
                x.last_used_at = crate::models::now_ms();
            }
        });
    }
    if let Err(e) = crate::paste::paste_text(app, &p.content) {
        eprintln!("[prompt-tool] 快捷键粘贴失败: {e}");
    }
}

/// 与前端 vars.ts 一致的 {{变量}} 宽松检测（排除 {{clipboard}} 自动变量）
fn has_vars(content: &str) -> bool {
    let mut rest = content;
    while let Some(i) = rest.find("{{") {
        let after = &rest[i + 2..];
        let Some(j) = after.find("}}") else {
            return false;
        };
        let name = after[..j].split('|').next().unwrap_or("").trim().to_lowercase();
        if !name.is_empty() && name != "clipboard" {
            return true;
        }
        rest = &after[j + 2..];
    }
    false
}

/// 快捷键是否含有效修饰键：裸字母/数字键注册成功后会全局拦截该键，
/// 系统级打字被劫持（评审 2026-09-10 M7#10）。与前端 HotkeyInput 口径一致：
/// Shift 单独不算（仅 Shift 不满足绑定要求）
fn has_effective_modifier(normalized: &str) -> bool {
    normalized
        .split('+')
        .any(|part| matches!(part, "alt" | "ctrl" | "super" | "win" | "meta"))
}

/// 导入 / 云同步合并后清理提示词快捷键：
/// 与主键、捕获键或先注册的提示词冲突的键一律清空并 bump updated_at（让清理结果同步出去）。
/// 导入路径没有 save_prompt 的 validate_prompt_hotkey 卡口，必须在此兜底，
/// 否则分享的提示词包可静默抢占/挤掉全局快捷键；
/// 无修饰键的裸键同样在此清空（导入数据绕过了前端 UI 的修饰键强制）
pub fn sanitize_prompt_hotkeys(data: &mut crate::models::AppData) {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let main = normalize(&data.settings.hotkey);
    if !main.is_empty() {
        seen.insert(main);
    }
    let capture = normalize(&data.settings.capture_hotkey);
    if !capture.is_empty() {
        seen.insert(capture);
    }
    for p in data.prompts.iter_mut() {
        if p.hotkey.trim().is_empty() {
            continue;
        }
        let n = normalize(&p.hotkey);
        if n.is_empty() || !has_effective_modifier(&n) || !seen.insert(n) {
            p.hotkey = String::new();
            p.updated_at = now_ms();
        }
    }
}

/// 面板垂直锚点：工作区上 1/3 —— y = 工作区顶 + max(0, (可用高度 - 面板高度) / 3)。
/// 面板高于工作区时负余量兜底为 0（贴工作区顶），多屏时工作区顶自带该屏偏移
fn one_third_top(work_top: i32, work_height: i32, panel_height: i32) -> i32 {
    work_top + ((work_height - panel_height) / 3).max(0)
}

/// 在鼠标当前所在显示器的工作区（排除任务栏）上 1/3 处显示快捷面板并聚焦
pub fn show_quick_window(app: &AppHandle) {
    let Some(win) = app.get_webview_window("main") else {
        eprintln!("[prompt-tool] show_quick_window: main 窗口不存在");
        return;
    };
    let already_visible = win.is_visible().unwrap_or(false);

    // 记录呼出前的前台窗口，供粘贴时恢复焦点
    if !already_visible {
        {
            let fg = crate::paste::foreground::current();
            let mut store = crate::store::lock(app);
            // 前台不是面板自身时才记录
            if fg.map_or(true, |h| Some(h) != win.hwnd().ok().map(|w| w.0 as isize)) {
                store.paste_target = fg;
            }
        }
    }

    let _ = win.unminimize();

    // 让面板出现在光标所在屏幕
    if let Ok(pos) = app.cursor_position() {
        if let Ok(Some(monitor)) = win.monitor_from_point(pos.x, pos.y) {
            let size = win.outer_size().unwrap_or(tauri::PhysicalSize::new(760, 520));
            let mp = monitor.position();
            let ms = monitor.size();
            let x = (mp.x + (ms.width as i32 - size.width as i32) / 2).max(mp.x + 8);
            // 垂直锚定工作区（排除任务栏）上 1/3；水平与多屏逻辑不变。
            // size 为当前面板物理高度（随前端 set_panel_height 联动），
            // .max(mp.y + 8) 保底：极限小屏下面板顶也不贴出屏幕上沿
            let wa = monitor.work_area();
            let y = one_third_top(wa.position.y, wa.size.height as i32, size.height as i32)
                .max(mp.y + 8);
            let _ = win.set_position(tauri::PhysicalPosition::new(x, y));
        }
    } else {
        let _ = win.center();
    }

    let _ = win.show();
    let _ = win.set_focus();
    // 直接向前端注入 DOM 事件重置会话（比 emit 更可靠，无 IPC 时序问题）：
    // 每次呼出都是全新的搜索（清空关键词/模式/选中项）
    let _ = win.eval("window.dispatchEvent(new CustomEvent('pm-panel-shown'));");
}

pub fn toggle_quick_window(app: &AppHandle) {
    let Some(win) = app.get_webview_window("main") else {
        return;
    };
    let visible = win.is_visible().unwrap_or(false);
    let focused = win.is_focused().unwrap_or(false);
    if visible && focused {
        let _ = win.hide();
    } else {
        show_quick_window(app);
    }
}

pub fn open_manager_window(app: &AppHandle) {
    let Some(win) = app.get_webview_window("manager") else {
        return;
    };
    let _ = win.unminimize();
    let _ = win.show();
    let _ = win.set_focus();
}

/// 校验提示词快捷键：与面板主键 / 捕获键 / 其他提示词不得重复
pub fn validate_prompt_hotkey(app: &AppHandle, prompt_id: &str, hotkey: &str) -> Result<(), String> {
    let hk = hotkey.trim();
    if hk.is_empty() {
        return Ok(());
    }
    let normalized = normalize(hk);
    let (main_hotkey, capture_hotkey) = {
        let store = crate::store::lock(app);
        (
            store.data.settings.hotkey.clone(),
            store.data.settings.capture_hotkey.clone(),
        )
    };
    if normalized == normalize(&main_hotkey) {
        return Err("该快捷键已被「呼出面板」占用".into());
    }
    if !capture_hotkey.trim().is_empty() && normalized == normalize(&capture_hotkey) {
        return Err("该快捷键已被「快速捕获」占用".into());
    }
    let conflict = {
        let store = crate::store::lock(app);
        store
            .data
            .prompts
            .iter()
            .find(|p| {
                p.id != prompt_id
                    && !p.hotkey.trim().is_empty()
                    && normalize(&p.hotkey) == normalized
            })
            .map(|p| p.title.clone())
    };
    if let Some(title) = conflict {
        return Err(format!("该快捷键已被提示词「{}」占用", title));
    }
    Ok(())
}

/// 统一 accelerator 大小写格式，如 "Alt+P" -> "alt+p"
pub(crate) fn normalize(s: &str) -> String {
    let parts: Vec<String> = s
        .split('+')
        .map(|p| p.trim().to_lowercase())
        .filter(|p| !p.is_empty())
        .collect();
    parts.join("+")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::AppData;

    fn prompt_with_hotkey(id: &str, hotkey: &str, updated_at: u64) -> crate::models::Prompt {
        crate::models::Prompt {
            id: id.to_string(),
            title: format!("提示词-{id}"),
            content: "正文".into(),
            category: "开发".into(),
            tags: vec![],
            pinned: false,
            hotkey: hotkey.to_string(),
            use_count: 0,
            last_used_at: 0,
            created_at: 1,
            updated_at,
        }
    }

    // ---------- normalize ----------

    #[test]
    fn normalize_unifies_case_and_whitespace() {
        assert_eq!(normalize("Alt+P"), "alt+p");
        assert_eq!(normalize("  Ctrl + Shift + A "), "ctrl+shift+a");
        assert_eq!(normalize("ALT+Q"), "alt+q");
    }

    #[test]
    fn normalize_drops_empty_segments() {
        assert_eq!(normalize("alt++p"), "alt+p");
        assert_eq!(normalize("++"), "");
        assert_eq!(normalize(""), "");
    }

    // ---------- sanitize_prompt_hotkeys ----------

    // 评审 2026-09-10 M7#10：导入路径绕过 UI 的修饰键强制，裸键在此兜底清空
    #[test]
    fn sanitize_strips_bare_keys_without_modifier() {
        let mut data = AppData::default();
        data.prompts.push(prompt_with_hotkey("p1", "a", 5));
        data.prompts.push(prompt_with_hotkey("p2", "shift+5", 5));
        data.prompts.push(prompt_with_hotkey("p3", "ctrl+k", 5));
        sanitize_prompt_hotkeys(&mut data);
        assert_eq!(data.prompts[0].hotkey, "", "裸字母键必须清空");
        assert_eq!(data.prompts[1].hotkey, "", "仅 Shift 修饰不算有效组合");
        assert_eq!(data.prompts[2].hotkey, "ctrl+k", "带修饰键的合法组合保留");
    }

    #[test]
    fn sanitize_clears_prompt_hotkey_conflicting_with_main_key() {
        let mut data = AppData::default(); // 默认主键 alt+q
        data.prompts.push(prompt_with_hotkey("p1", "Alt+Q", 5));
        data.prompts.push(prompt_with_hotkey("p2", "ctrl+k", 5));

        sanitize_prompt_hotkeys(&mut data);
        let p1 = data.prompts.iter().find(|p| p.id == "p1").unwrap();
        let p2 = data.prompts.iter().find(|p| p.id == "p2").unwrap();
        assert!(p1.hotkey.is_empty(), "与主键冲突（归一化后比较）必须清空");
        assert!(p1.updated_at > 5, "清理结果必须 bump updated_at 才能同步出去");
        assert_eq!(p2.hotkey, "ctrl+k", "无冲突的快捷键必须保留");
    }

    #[test]
    fn sanitize_clears_prompt_hotkey_conflicting_with_capture_key() {
        let mut data = AppData::default(); // 默认捕获键 alt+s
        data.prompts.push(prompt_with_hotkey("p1", "  ALT + S ", 5));
        sanitize_prompt_hotkeys(&mut data);
        assert!(data.prompts[0].hotkey.is_empty());
    }

    #[test]
    fn sanitize_duplicate_prompt_hotkeys_first_wins() {
        let mut data = AppData::default();
        data.prompts.push(prompt_with_hotkey("p1", "ctrl+j", 5));
        data.prompts.push(prompt_with_hotkey("p2", "Ctrl+J", 5));
        data.prompts.push(prompt_with_hotkey("p3", " ctrl+j ", 5));

        sanitize_prompt_hotkeys(&mut data);
        assert_eq!(data.prompts[0].hotkey, "ctrl+j", "先注册者保留");
        assert!(data.prompts[1].hotkey.is_empty(), "后到的重复键清空");
        assert!(data.prompts[2].hotkey.is_empty(), "仅空白差异的等价形式同样算重复");
    }

    #[test]
    fn sanitize_leaves_empty_hotkeys_untouched() {
        let mut data = AppData::default();
        data.prompts.push(prompt_with_hotkey("p1", "   ", 7));
        sanitize_prompt_hotkeys(&mut data);
        assert_eq!(data.prompts[0].updated_at, 7, "空快捷键不应触发 updated_at 变化");
    }

    // ---------- one_third_top：面板垂直定位（工作区上 1/3） ----------

    #[test]
    fn one_third_top_places_panel_in_upper_third_of_work_area() {
        // 1040 可用高、520 面板 → (1040-520)/3 = 173
        assert_eq!(one_third_top(0, 1040, 520), 173);
        // 面板更小则更靠上：(1040-260)/3 = 260
        assert_eq!(one_third_top(0, 1040, 260), 260);
        // 恰好整除：(600-300)/3 = 100
        assert_eq!(one_third_top(0, 600, 300), 100);
    }

    #[test]
    fn one_third_top_keeps_multi_monitor_offset_and_clamps_negative() {
        // 副屏在上方（工作区顶 y=-1440）：基准点必须跟随该屏
        assert_eq!(one_third_top(-1440, 1040, 520), -1440 + 173);
        // 面板高于工作区：负余量兜底为 0，贴工作区顶而非飞出屏幕
        assert_eq!(one_third_top(0, 400, 640), 0);
        assert_eq!(one_third_top(60, 400, 640), 60);
    }

    // ---------- has_vars：与前端 vars.ts 的行为一致性 ----------

    #[test]
    fn rust_has_vars_matches_frontend_semantics() {
        assert!(!has_vars("纯文本"));
        assert!(!has_vars(""), "空内容无变量");
        assert!(!has_vars("{{}}"), "空变量名不算");
        assert!(!has_vars("{{clipboard}}"), "自动变量不算手动变量");
        assert!(!has_vars("{{ ClipBoard }}"), "自动变量忽略大小写与空白");
        assert!(!has_vars("{{未闭合"), "缺少闭合不算");
        assert!(has_vars("{{名字}}"));
        assert!(has_vars("前缀 {{ name | 提示语 }} 后缀"));
        assert!(has_vars("{{a|hint}}"));
    }
}

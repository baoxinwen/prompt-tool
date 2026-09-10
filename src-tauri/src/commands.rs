use tauri::{AppHandle, Emitter, Manager, WebviewWindow, Wry};
use tauri_plugin_autostart::ManagerExt as AutostartManagerExt;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

use crate::models::{new_id, now_ms, AppData, ClipboardItem, Prompt, Settings, SyncReport};
use crate::store::lock;
use crate::transfer;
use crate::updater;
use crate::{hotkey, paste, sync};

fn emit_data_changed(app: &AppHandle) {
    let _ = app.emit("data-changed", ());
}

// ---------- 可测的纯逻辑（命令包装层调用这些函数） ----------

/// 保存/新建提示词的公共落库逻辑：空标题兜底「未命名提示词」、空分类兜底「未分类」、
/// 更新时间刷新；新建生成 id 与 created_at，更新保留 created_at 与使用统计。
/// 返回该条目的有效 id（新建时为生成的 id，供前端绑定草稿，避免前端用
/// 「createdAt 最新」启发式反查在并发建档时绑错条目——评审 2026-09-10 I5）
fn upsert_prompt(data: &mut AppData, mut prompt: Prompt) -> String {
    prompt.updated_at = now_ms();
    if prompt.title.trim().is_empty() {
        prompt.title = "未命名提示词".into();
    }
    // 空分类兜底：否则该提示词不出现在任何分类下，
    // 且 export_markdown（按 categories 遍历）会把它静默丢掉
    if prompt.category.trim().is_empty() {
        prompt.category = "未分类".into();
    }
    let id = prompt.id.clone();
    if id.is_empty() {
        prompt.id = new_id();
        prompt.created_at = now_ms();
        let p = prompt.clone();
        data.ensure_category(&p.category);
        data.prompts.push(p);
        prompt.id
    } else {
        match data.prompts.iter_mut().find(|x| x.id == id) {
            Some(existing) => {
                let created = existing.created_at;
                let use_count = existing.use_count;
                let last_used = existing.last_used_at;
                *existing = prompt.clone();
                existing.created_at = created;
                existing.use_count = use_count;
                existing.last_used_at = last_used;
                data.ensure_category(&prompt.category);
            }
            None => {
                let p = prompt.clone();
                data.ensure_category(&p.category);
                data.prompts.push(p);
            }
        }
        id
    }
}

/// 分类重命名校验：空名 / 与现有分类重名都拒绝；old_name 必须存在——
/// 否则零修改却返回成功，前端误以为重命名完成（评审 2026-09-10 M6#6）
fn validate_category_rename(data: &AppData, old_name: &str, new_name: &str) -> Result<(), String> {
    let new_name = new_name.trim();
    if new_name.is_empty() {
        return Err("分类名不能为空".into());
    }
    if data.categories.iter().any(|c| c == new_name) {
        return Err("该分类名已存在".into());
    }
    if !data.categories.iter().any(|c| c == old_name) {
        return Err("原分类不存在".into());
    }
    Ok(())
}

/// 分类重命名的应用步骤：迁移分类列表与该分类下的提示词，并 bump 更新时间
fn apply_category_rename(data: &mut AppData, old_name: &str, new_name: &str) {
    for c in data.categories.iter_mut() {
        if c == old_name {
            *c = new_name.to_string();
        }
    }
    for p in data.prompts.iter_mut() {
        if p.category == old_name {
            p.category = new_name.to_string();
            p.updated_at = now_ms();
        }
    }
}

/// 删除分类：该分类下的提示词归入「未分类」
fn delete_category_in(data: &mut AppData, name: &str) {
    data.categories.retain(|c| c != name);
    data.ensure_category("未分类");
    for p in data.prompts.iter_mut() {
        if p.category == name {
            p.category = "未分类".into();
            p.updated_at = now_ms();
        }
    }
}

/// 按扩展名把单个文件内容导入数据；ext 已小写，file_stem 供 txt 作标题
fn import_dispatch(
    data: &mut AppData,
    ext: &str,
    file_stem: Option<&str>,
    text: &str,
) -> Result<(usize, usize), String> {
    match ext {
        "json" => transfer::import_json(data, text),
        "md" | "markdown" => {
            let (a, s) = transfer::import_markdown(data, text, "未分类");
            Ok((a, s))
        }
        "txt" => {
            let title = file_stem
                .map(|s| s.to_string())
                .unwrap_or_else(|| "导入文本".into());
            if transfer::import_text(data, &title, text) {
                Ok((1, 0))
            } else {
                Ok((0, 1))
            }
        }
        other => Err(format!("不支持的格式: {other}")),
    }
}

// ---------- 数据读取 / 提示词 ----------
// 落盘类命令一律 async + spawn_blocking：非 async 命令会在 UI 主线程内联执行，
// 其内部 Store::save 是全量序列化 + fsync + 备份 + 改名，数据量大或磁盘慢时
// 卡顿主线程、延迟窗口事件（评审 I10）。export/import/sync 已是该范式

/// get_data 按调用窗口脱敏凭据（评审 rc-M9）：管理窗口需要完整设置
///（同步表单回显/保存依赖凭据字段）；快捷面板与捕获窗不读也不写凭据，
/// 返回前清空明文，收敛凭据暴露面。window 由 Tauri 自动注入（同 invoke_paste）
fn redact_credentials(mut data: AppData) -> AppData {
    data.settings.webdav.password = String::new();
    data.settings.gist.token = String::new();
    data
}

#[tauri::command]
pub fn get_data(app: AppHandle, window: WebviewWindow) -> AppData {
    let data = lock(&app).data.clone();
    if window.label() == "manager" {
        data
    } else {
        redact_credentials(data)
    }
}

#[tauri::command]
pub async fn save_prompt(app: AppHandle, prompt: Prompt) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        hotkey::validate_prompt_hotkey(&app, &prompt.id, &prompt.hotkey)?;
        let mut store = lock(&app);
        let saved_id = store.mutate(|d| upsert_prompt(d, prompt.clone()))?;
        drop(store);
        // 注册失败要回传给用户：数据已保存（上面 mutate 已落盘），但快捷键按下无反应，
        // 静默失败会让用户以为绑定成功而无法自助排查
        let failed = hotkey::register_all(&app);
        if !prompt.hotkey.trim().is_empty()
            && failed.iter().any(|a| *a == hotkey::normalize(&prompt.hotkey))
        {
            emit_data_changed(&app);
            return Err(format!(
                "提示词已保存，但独立快捷键 {} 注册失败（可能被其他程序占用），按下不会生效",
                prompt.hotkey
            ));
        }
        emit_data_changed(&app);
        Ok(saved_id)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn delete_prompt(app: AppHandle, id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        {
            let mut store = lock(&app);
            store.mutate(|d| {
                d.prompts.retain(|p| p.id != id);
                d.tombstone(&id);
            })?;
        }
        hotkey::register_all(&app);
        emit_data_changed(&app);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn record_prompt_use(app: AppHandle, id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut store = lock(&app);
        store.mutate(|d| {
            if let Some(p) = d.prompts.iter_mut().find(|p| p.id == id) {
                p.use_count += 1;
                p.last_used_at = now_ms();
            }
        })?;
        emit_data_changed(&app);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---------- 分类 ----------

#[tauri::command]
pub async fn add_category(app: AppHandle, name: String) -> Result<(), String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("分类名不能为空".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let mut store = lock(&app);
        store.mutate(|d| d.ensure_category(&name))?;
        emit_data_changed(&app);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn rename_category(app: AppHandle, old_name: String, new_name: String) -> Result<(), String> {
    let old_name = old_name.trim().to_string();
    let new_name = new_name.trim().to_string();
    tauri::async_runtime::spawn_blocking(move || {
        let mut store = lock(&app);
        // old_name 必须存在：不存在时零修改返回成功会让前端误以为重命名完成（M6#6）
        validate_category_rename(&store.data, &old_name, &new_name)?;
        store.mutate(|d| apply_category_rename(d, &old_name, &new_name))?;
        drop(store);
        emit_data_changed(&app);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn delete_category(app: AppHandle, name: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut store = lock(&app);
        store.mutate(|d| delete_category_in(d, &name))?;
        drop(store);
        emit_data_changed(&app);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---------- 剪贴板 / 粘贴 ----------

#[tauri::command]
pub fn copy_text(app: AppHandle, text: String) -> Result<(), String> {
    let generation = {
        let mut store = lock(&app);
        store.suppress_clipboard = true;
        // 使仍在飞行的粘贴会话收尾失效：它们若照常恢复原剪贴板，
        // 会覆盖刚写入的复制内容（评审 M1 同源问题）
        store.paste_generation += 1;
        store.paste_generation
    };
    // 写入失败必须解除抑制，否则剪贴板历史从此静默失效直到重启。
    // 按代际判断（M6#7）：失败瞬间可能已有新会话推进代际，直接清标志
    // 会提前解除新会话的抑制，使其内容被误记进剪贴板历史
    if let Err(e) = paste::set_clipboard_text(&text) {
        lock(&app).release_suppress_if_current(generation);
        return Err(e);
    }
    let h = app.clone();
    std::thread::spawn(move || {
        // 与其他写入路径共用抑制窗口：跨过一个完整轮询周期，监听线程
        // 把本次写入吸收进基线后才解除。代际已变（期间又有新写入会话）
        // 时不得解除，否则会把新会话写入的内容误记进历史（评审 I2）
        std::thread::sleep(crate::clipboard::SUPPRESS_WINDOW);
        lock(&h).release_suppress_if_current(generation);
    });
    Ok(())
}

#[tauri::command]
pub async fn invoke_paste(
    app: AppHandle,
    window: WebviewWindow,
    text: String,
    prompt_id: Option<String>,
) -> Result<(), String> {
    // 用量记账走全量落盘，不得阻塞 UI 主线程（评审 I10）
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(id) = prompt_id {
            let mut store = lock(&app);
            let _ = store.mutate(|d| {
                if let Some(p) = d.prompts.iter_mut().find(|p| p.id == id) {
                    p.use_count += 1;
                    p.last_used_at = now_ms();
                }
            });
        }
        paste::paste_to_previous_window(&window, &app, &text)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn paste_text_direct(app: AppHandle, window: WebviewWindow, text: String) -> Result<(), String> {
    paste::paste_to_previous_window(&window, &app, &text)
}

/// 读取当前剪贴板文本（供 {{clipboard}} 自动变量使用）
#[tauri::command]
pub fn get_clipboard_text() -> Option<String> {
    paste::get_clipboard_text()
}

#[tauri::command]
pub fn get_var_memory(
    app: tauri::AppHandle<Wry>,
    prompt_id: String,
) -> std::collections::BTreeMap<String, String> {
    crate::var_memory::get_var_memory(app, prompt_id)
}

#[tauri::command]
pub async fn save_var_memory(
    app: tauri::AppHandle<Wry>,
    prompt_id: String,
    values: std::collections::BTreeMap<String, String>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || crate::var_memory::save_var_memory(app, prompt_id, values))
        .await
        .map_err(|e| e.to_string())?
}

// ---------- 剪贴板图片 ----------

#[tauri::command]
pub async fn get_image_thumb(app: AppHandle, id: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let file = lock(&app)
            .data
            .clipboard
            .iter()
            .find(|i| i.id == id)
            .and_then(|i| i.image.as_ref())
            .map(|im| im.file.clone())
            .ok_or("图片不存在")?;
        let bytes = crate::images::read_png(&app, &crate::images::thumb_name(&file))?;
        Ok(crate::images::png_base64(&bytes))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 找到剪贴板图片条目并解码为 RGBA（paste_image / copy_image 共用）
fn load_image_rgba(app: &AppHandle, id: &str) -> Result<image::RgbaImage, String> {
    let file = lock(app)
        .data
        .clipboard
        .iter()
        .find(|i| i.id == id)
        .and_then(|i| i.image.as_ref())
        .map(|im| im.file.clone())
        .ok_or("图片不存在")?;
    let bytes = crate::images::read_png(app, &file)?;
    Ok(image::load_from_memory(&bytes)
        .map_err(|e| format!("解码图片失败: {e}"))?
        .to_rgba8())
}

/// 把 RGBA 写入系统剪贴板
fn set_clipboard_image(rgba: &image::RgbaImage) -> Result<(), String> {
    arboard::Clipboard::new()
        .and_then(|mut c| {
            c.set_image(arboard::ImageData {
                width: rgba.width() as usize,
                height: rgba.height() as usize,
                bytes: std::borrow::Cow::Borrowed(rgba.as_raw()),
            })
        })
        .map_err(|e| format!("写入图片剪贴板失败: {e}"))
}

#[tauri::command]
pub async fn paste_image(app: AppHandle, window: WebviewWindow, id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let rgba = load_image_rgba(&app, &id)?;

        let generation = {
            let mut store = lock(&app);
            store.suppress_clipboard = true;
            // 使更早的写入会话收尾失效（同 paste_text/copy_text 范式）：
            // 旧会话不得恢复剪贴板覆盖本会话写入的图片，也不得提前解除抑制
            store.paste_generation += 1;
            store.paste_generation
        };
        // 写入失败必须解除抑制，否则剪贴板历史从此静默失效直到重启
        if let Err(e) = set_clipboard_image(&rgba) {
            lock(&app).suppress_clipboard = false;
            return Err(e);
        }

        let _ = window.hide();
        let append_enter = lock(&app).data.settings.paste_append_enter;
        let handle = app.clone();
        std::thread::spawn(move || {
            let target = lock(&handle).paste_target.take();
            if let Err(e) = paste::send_paste(target, append_enter) {
                // 注入失败只能异步上报（invoke_paste 此时已返回），交前端 toast
                eprintln!("[prompt-tool] {e}");
                let _ = tauri::Emitter::emit(&handle, "paste-failed", e);
            }
            // 与其他写入路径共用抑制窗口；代际未变才解除，避免把程序自己
            // 写入/粘贴的内容误记进剪贴板历史（评审 I2）
            std::thread::sleep(crate::clipboard::SUPPRESS_WINDOW);
            lock(&handle).release_suppress_if_current(generation);
        });
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 复制图片条目到剪贴板（只复制不粘贴）：快捷面板对图片项按
/// Shift+Enter 的语义（底栏提示 Shift+Enter=复制），此前静默无操作（评审 M12）
#[tauri::command]
pub async fn copy_image(app: AppHandle, id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let rgba = load_image_rgba(&app, &id)?;

        let generation = {
            let mut store = lock(&app);
            store.suppress_clipboard = true;
            // 同 copy_text：程序自己的写入不记入剪贴板历史
            store.paste_generation += 1;
            store.paste_generation
        };
        if let Err(e) = set_clipboard_image(&rgba) {
            lock(&app).suppress_clipboard = false;
            return Err(e);
        }

        let handle = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(crate::clipboard::SUPPRESS_WINDOW);
            lock(&handle).release_suppress_if_current(generation);
        });
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn delete_history_item(app: AppHandle, id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let file = {
            let mut store = lock(&app);
            let file = store
                .data
                .clipboard
                .iter()
                .find(|i| i.id == id)
                .and_then(|i| i.image.as_ref())
                .map(|im| im.file.clone());
            store.mutate(|d| {
                d.clipboard.retain(|i| i.id != id);
                d.tombstone(&id);
            })?;
            file
        };
        if let Some(f) = file {
            if let Some(base) = f.strip_suffix(".png") {
                crate::images::delete_files(&app, base);
            }
        }
        emit_data_changed(&app);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn clear_history(app: AppHandle) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let count = {
            let mut store = lock(&app);
            let count = store.data.clipboard.len();
            // 清空历史进 stash（不删图片文件，撤销可原样恢复）
            store.stash = Some(std::mem::take(&mut store.data.clipboard));
            store.mutate(|d| {
                d.clipboard.clear();
            })?;
            count
        };
        emit_data_changed(&app);
        Ok(count)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn restore_history(app: AppHandle) -> Result<usize, String> {
    let mut store = lock(&app);
    let Some(stashed) = store.stash.take() else {
        return Ok(0);
    };
    // stash 条目按 id 与现存历史去重后并回；恢复条目 copied_at 刷新为当前时刻，
    // 使其 > 任何既有墓碑 at（双向 merge 收敛到「已恢复」，不被同步回退误删）
    let now = crate::models::now_ms();
    let mut restored: usize = 0;
    for mut item in stashed {
        if !store.data.clipboard.iter().any(|i| i.id == item.id) {
            item.copied_at = now;
            store.data.clipboard.push(item);
            restored += 1;
        }
    }
    let live_ids: std::collections::HashSet<String> =
        store.data.clipboard.iter().map(|i| i.id.clone()).collect();
    store.data.tombstones.retain(|t| !live_ids.contains(&t.id));
    if restored > 0 {
        store.save().map_err(|e| e.to_string())?;
    }
    Ok(restored)
}

/// 收集 stash 里的图片文件引用（delete_files 需要去掉 .png 的 base 名再拼回）
fn stash_image_files(stash: &[ClipboardItem]) -> Vec<String> {
    stash
        .iter()
        .filter(|i| i.is_image())
        .filter_map(|i| i.image.as_ref().map(|im| im.file.clone()))
        .collect()
}

/// 应用退出时收口 stash：图片文件统一删盘（再无恢复机会）
pub fn purge_stash_images(app: &AppHandle) {
    let files = {
        let mut store = lock(app);
        let stash = store.stash.take();
        stash_image_files(stash.as_deref().unwrap_or(&[]))
    };
    for f in files {
        if let Some(base) = f.strip_suffix(".png") {
            crate::images::delete_files(app, base);
        }
    }
}

// ---------- 窗口控制 ----------

#[tauri::command]
pub fn hide_quick(window: WebviewWindow) {
    let _ = window.hide();
}

#[tauri::command]
pub fn open_manager(app: AppHandle) {
    hotkey::open_manager_window(&app);
}

/// 启动时数据文件损坏的恢复提示（取后即清）。
/// 不用事件推送：窗口刚创建时前端尚未注册监听，事件必然丢失
#[tauri::command]
pub fn get_recovery_notice(app: AppHandle) -> Option<String> {
    lock(&app).recovered_notice.take()
}

// ---------- 设置 ----------

#[tauri::command]
pub fn close_capture(app: AppHandle) {
    crate::capture::hide(&app);
}

/// 快捷面板高度自适应：前端测量内容高度后调用，窗口随之伸缩。
/// 最小托底 300 与前端 panelHeight.ts 的 PANEL_MIN 保持一致
#[tauri::command]
pub fn set_panel_height(app: AppHandle, height: f64) {
    use tauri::LogicalSize;
    if let Some(w) = app.get_webview_window("main") {
        let h = height.clamp(300.0, 640.0);
        let _ = w.set_size(LogicalSize::new(760.0_f64, h));
    }
}

#[tauri::command]
pub async fn save_settings(app: AppHandle, settings: Settings) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (old_hotkey, old_capture_hotkey) = {
            let store = lock(&app);
            (
                store.data.settings.hotkey.clone(),
                store.data.settings.capture_hotkey.clone(),
            )
        };
        {
            let mut store = lock(&app);
            store.mutate(|d| d.settings = settings.clone())?;
        }
        // 任一全局快捷键变化都需要重注册
        if settings.hotkey.trim().to_lowercase() != old_hotkey.trim().to_lowercase()
            || settings.capture_hotkey.trim().to_lowercase() != old_capture_hotkey.trim().to_lowercase()
        {
            let failed = hotkey::register_all(&app);
            // 只对本次保存涉及的两个键负责：其他提示词键被占用与本次保存无关
            let relevant: Vec<String> = [settings.hotkey.trim(), settings.capture_hotkey.trim()]
                .iter()
                .filter(|k| !k.is_empty())
                .map(|k| hotkey::normalize(k))
                .collect();
            let hit: Vec<&String> = failed.iter().filter(|f| relevant.contains(f)).collect();
            if !hit.is_empty() {
                emit_data_changed(&app);
                return Err(format!(
                    "设置已保存，但快捷键注册失败（可能被其他程序占用）: {}",
                    hit.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("、")
                ));
            }
        }
        emit_data_changed(&app);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn get_autostart(app: AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, enable: bool) -> Result<(), String> {
    let launcher = app.autolaunch();
    if enable {
        launcher.enable().map_err(|e| e.to_string())
    } else {
        launcher.disable().map_err(|e| e.to_string())
    }
}

#[tauri::command]
pub fn open_data_dir(app: AppHandle) -> Result<(), String> {
    let dir = {
        let store = lock(&app);
        store.data_dir()
    };
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| format!("打开目录失败: {e}"))
}

// ---------- 导入 / 导出 ----------

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub added: usize,
    pub skipped: usize,
    pub message: String,
}

#[tauri::command]
pub async fn export_data(app: AppHandle, kind: String, include_clipboard: bool) -> Result<String, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let data = lock(&handle).data.clone();
        let stamp = time_stamp();
        let (filter_name, filter_ext, default_name) = match kind.as_str() {
            "markdown" => (
                "Markdown",
                vec!["md"],
                format!("prompt-tool-{stamp}.md"),
            ),
            _ => ("JSON", vec!["json"], format!("prompt-tool-{stamp}.json")),
        };

        let file = handle
            .dialog()
            .file()
            .add_filter(filter_name, &filter_ext)
            .set_file_name(&default_name)
            .blocking_save_file();

        let Some(file_path) = file else {
            return Ok(String::new());
        };
        let path = file_path.into_path().map_err(|e| e.to_string())?;

        let content = match kind.as_str() {
            "markdown" => transfer::export_markdown(&data),
            _ => transfer::export_json(&data, include_clipboard)?,
        };
        // 写入失败时清理半成品（评审 2026-09-10 M8#10）：
        // 残留的部分字节文件会被用户误当完整备份留存
        if let Err(e) = std::fs::write(&path, &content) {
            let _ = std::fs::remove_file(&path);
            return Err(format!("写入文件失败: {e}"));
        }
        Ok(path.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn import_data(app: AppHandle) -> Result<ImportSummary, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let files = handle
            .dialog()
            .file()
            .add_filter("提示词文件", &["json", "md", "markdown", "txt"])
            .blocking_pick_files();

        let Some(files) = files else {
            return Ok(ImportSummary {
                added: 0,
                skipped: 0,
                message: "已取消".into(),
            });
        };

        let paths: Vec<std::path::PathBuf> = files
            .into_iter()
            .filter_map(|f| f.into_path().ok())
            .collect();
        import_from_files(&handle, &paths)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 拖拽导入：直接传入文件路径列表
#[tauri::command]
pub async fn import_paths(app: AppHandle, paths: Vec<String>) -> Result<ImportSummary, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        import_from_files(&handle, &paths.iter().map(std::path::PathBuf::from).collect::<Vec<_>>())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 单文件导入上限：合法备份远小于此值；误拖安装包/视频等大文件时在读取前
/// 快速失败，避免全量读入数倍内存导致长时间无响应甚至 OOM（评审 2026-09-10 I23）
const MAX_IMPORT_FILE_BYTES: u64 = 50 * 1024 * 1024;

fn validate_import_size(len: u64) -> Result<(), String> {
    if len > MAX_IMPORT_FILE_BYTES {
        return Err(format!(
            "文件超过 {}MB 导入上限",
            MAX_IMPORT_FILE_BYTES / 1024 / 1024
        ));
    }
    Ok(())
}

fn import_from_files(handle: &AppHandle, files: &[std::path::PathBuf]) -> Result<ImportSummary, String> {
    let mut total_added = 0usize;
    let mut total_skipped = 0usize;
    let mut errors: Vec<String> = Vec::new();
    let mut dirty = false;

    for path in files {
        // 大小上限在读取前校验（fail fast，评审 2026-09-10 I23）
        if let Err(e) = std::fs::metadata(path)
            .map(|m| m.len())
            .map_err(|e| format!("{}: {e}", path.display()))
            .and_then(validate_import_size)
        {
            errors.push(e);
            continue;
        }
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                errors.push(format!("{}: {e}", path.display()));
                continue;
            }
        };
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let stem = path.file_stem().map(|s| s.to_string_lossy().to_string());

        let mut store = lock(handle);
        let result = import_dispatch(&mut store.data, &ext, stem.as_deref(), &text);
        match result {
            Ok((a, s)) => {
                total_added += a;
                total_skipped += s;
                // 导入不走 save_prompt 的快捷键校验，这里统一清理冲突
                hotkey::sanitize_prompt_hotkeys(&mut store.data);
                // 循环外统一 commit：N 个文件不再放大成 N 次全量落盘（评审 2026-09-10 R6#11）
                dirty = true;
            }
            Err(e) => errors.push(format!("{}: {e}", path.display())),
        }
    }
    if dirty {
        let mut store = lock(handle);
        // 落盘失败要反馈给用户，不能让"导入成功"的数据只停留在内存里重启即丢
        if let Err(e) = store.commit() {
            errors.push(format!("保存失败: {e}"));
        }
    }

    if total_added > 0 {
        // 导入的提示词可能带独立快捷键，必须重新注册后再通知前端
        hotkey::register_all(handle);
        emit_data_changed(handle);
    }
    let mut message = format!("导入完成：新增 {total_added} 条，跳过 {total_skipped} 条");
    if !errors.is_empty() {
        message.push_str(&format!("，{} 个文件失败", errors.len()));
    }
    Ok(ImportSummary {
        added: total_added,
        skipped: total_skipped,
        message,
    })
}

fn time_stamp() -> String {
    let ms = now_ms();
    let secs = ms / 1000;
    let days = secs / 86400;
    // 简单的 UTC 日期格式化，足够用于文件名
    let (y, mo, d) = civil_from_days(days as i64);
    format!("{y}{mo:02}{d:02}")
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

// ---------- 同步 ----------

#[tauri::command]
pub async fn webdav_test(url: String, username: String, password: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || sync::test_connection(&url, &username, &password))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn gist_test(token: String, gist_id: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || sync::gist_test(&token, &gist_id))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn sync_now(app: AppHandle, direction: String) -> Result<SyncReport, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || sync::run_sync(&handle, &direction))
        .await
        .map_err(|e| e.to_string())?
}

// ---------- 应用内更新 ----------

#[tauri::command]
pub async fn check_for_update(app: tauri::AppHandle) -> updater::UpdateStatus {
    updater::check(&app).await
}

#[tauri::command]
pub async fn download_and_install_update(app: tauri::AppHandle) -> Result<(), String> {
    updater::download_and_install(app).await
}

#[tauri::command]
pub fn open_release_page(app: tauri::AppHandle) -> Result<(), String> {
    app.opener()
        .open_url("https://github.com/baoxinwen/prompt-tool/releases", None::<&str>)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prompt(id: &str, title: &str, category: &str) -> Prompt {
        Prompt {
            id: id.to_string(),
            title: title.to_string(),
            content: "正文".into(),
            category: category.to_string(),
            tags: vec![],
            pinned: false,
            hotkey: String::new(),
            use_count: 0,
            last_used_at: 0,
            created_at: 1_000,
            updated_at: 1_000,
        }
    }

    fn data_with_prompt() -> AppData {
        let mut d = AppData::default();
        d.categories = vec!["开发".into()];
        d.prompts.push(prompt("p1", "已有", "开发"));
        d
    }

    // ---------- upsert_prompt ----------

    // 评审 2026-09-10 I23：导入文件大小上限的边界锁定
    #[test]
    fn validate_import_size_rejects_over_limit() {
        assert!(validate_import_size(0).is_ok());
        assert!(validate_import_size(1024).is_ok());
        assert!(validate_import_size(50 * 1024 * 1024).is_ok(), "恰好上限应放行");
        let err = validate_import_size(50 * 1024 * 1024 + 1).unwrap_err();
        assert!(err.contains("50MB"), "错误信息应包含上限值: {err}");
    }

    // 评审 2026-09-10 I5：save_prompt 必须回传建档 id，前端据此绑定草稿
    #[test]
    fn upsert_prompt_returns_effective_id() {
        let mut d = AppData::default();
        let created = upsert_prompt(&mut d, prompt("", "新建", ""));
        assert!(!created.is_empty(), "新建必须返回生成的 id");

        let updated = upsert_prompt(&mut d, prompt(&created, "改名", ""));
        assert_eq!(updated, created, "更新必须原样返回已有 id");
        assert_eq!(d.prompts.len(), 1, "更新不得另建条目");
        assert_eq!(d.prompts[0].title, "改名");
    }

    #[test]
    fn upsert_new_assigns_id_and_defaults() {
        let mut d = AppData::default();
        upsert_prompt(&mut d, prompt("", "", ""));
        assert_eq!(d.prompts.len(), 1);
        let p = &d.prompts[0];
        assert!(!p.id.is_empty(), "空 id 必须生成新 id");
        assert_eq!(p.title, "未命名提示词", "空标题兜底");
        assert_eq!(p.category, "未分类", "空分类兜底");
        assert!(d.categories.contains(&"未分类".to_string()), "兜底分类必须登记");
        assert!(p.created_at > 0);
        assert!(p.updated_at > 0);
        // 不断言 updated_at >= created_at：两者来自两次独立的墙钟读取，
        // NTP 回拨可使其反转，时序不构成产品契约（曾致偶发失败）
    }

    #[test]
    fn upsert_update_preserves_created_and_usage() {
        let mut d = data_with_prompt();
        d.prompts[0].created_at = 111;
        d.prompts[0].use_count = 5;
        d.prompts[0].last_used_at = 99;

        let mut edited = prompt("p1", "改名", "写作");
        edited.use_count = 42; // 前端带来的计数不得覆盖后端累计
        upsert_prompt(&mut d, edited);

        assert_eq!(d.prompts.len(), 1, "同 id 更新不得新增条目");
        let p = &d.prompts[0];
        assert_eq!(p.title, "改名");
        assert_eq!(p.category, "写作");
        assert_eq!(p.created_at, 111, "created_at 必须保留");
        assert_eq!(p.use_count, 5, "use_count 必须保留");
        assert_eq!(p.last_used_at, 99, "last_used_at 必须保留");
        assert!(d.categories.contains(&"写作".to_string()), "新分类必须登记");
        assert!(d.categories.contains(&"开发".to_string()), "旧分类不得丢失");
    }

    #[test]
    fn upsert_update_unknown_id_pushes_and_registers_category() {
        // 回归：带已知 id 但库里不存在（例如云端删除后本地再保存）时走 push，
        // 此前该路径不登记分类，条目会游离在所有分类 chips 之外
        let mut d = data_with_prompt();
        upsert_prompt(&mut d, prompt("ghost", "幽灵条目", "新分类"));
        assert_eq!(d.prompts.len(), 2);
        assert!(
            d.categories.contains(&"新分类".to_string()),
            "push 路径同样必须登记分类"
        );
    }

    // ---------- 分类重命名 / 删除 ----------

    #[test]
    fn rename_rejects_empty_and_duplicate() {
        let mut d = data_with_prompt();
        d.categories.push("写作".into());

        assert_eq!(
            validate_category_rename(&d, "开发", "   "),
            Err("分类名不能为空".into())
        );
        assert_eq!(
            validate_category_rename(&d, "开发", "写作"),
            Err("该分类名已存在".into())
        );
        assert!(validate_category_rename(&d, "开发", "设计").is_ok());
        assert!(
            validate_category_rename(&d, "不存在", "设计").is_err(),
            "old_name 不存在必须报错，零修改假成功会误导前端（M6#6）"
        );
    }

    #[test]
    fn rename_moves_categories_and_prompts_bumping_updated_at() {
        let mut d = data_with_prompt();
        d.categories.push("旧名".into());
        d.prompts.push(prompt("p2", "待迁移", "旧名"));

        apply_category_rename(&mut d, "旧名", "新名");
        assert!(d.categories.contains(&"新名".to_string()));
        assert!(!d.categories.contains(&"旧名".to_string()));
        let p = d.prompts.iter().find(|p| p.id == "p2").unwrap();
        assert_eq!(p.category, "新名");
        assert!(p.updated_at > 1_000, "迁移后的提示词要 bump updated_at 以便同步");
        // 其他分类不受影响
        assert_eq!(d.prompts.iter().find(|p| p.id == "p1").unwrap().category, "开发");
    }

    #[test]
    fn delete_category_moves_prompts_to_uncategorized() {
        let mut d = data_with_prompt();
        d.prompts.push(prompt("p2", "待迁移", "临时分类"));
        d.categories.push("临时分类".into());

        delete_category_in(&mut d, "临时分类");
        assert!(!d.categories.contains(&"临时分类".to_string()));
        assert!(d.categories.contains(&"未分类".to_string()), "未分类必须被登记");
        let p = d.prompts.iter().find(|p| p.id == "p2").unwrap();
        assert_eq!(p.category, "未分类");
        assert!(p.updated_at > 1_000);
    }

    // ---------- import_dispatch ----------

    #[test]
    fn dispatch_json_ok_and_error() {
        let mut d = AppData::default();
        let (a, s) = import_dispatch(&mut d, "json", None, r#"[{"title":"T","content":"C"}]"#).unwrap();
        assert_eq!((a, s), (1, 0));
        assert_eq!(d.prompts[0].title, "T");

        let err = import_dispatch(&mut d, "json", None, "不是JSON").unwrap_err();
        assert!(err.contains("JSON 解析失败"));
    }

    #[test]
    fn dispatch_markdown_uses_h1_category() {
        let mut d = AppData::default();
        let md = "# 我的分类\n\n## 标题A\n\n```\n正文\n```\n";
        let (a, s) = import_dispatch(&mut d, "md", None, md).unwrap();
        assert_eq!((a, s), (1, 0));
        assert_eq!(d.prompts[0].category, "我的分类");
    }

    #[test]
    fn dispatch_txt_uses_file_stem_as_title() {
        let mut d = AppData::default();
        let (a, s) = import_dispatch(&mut d, "txt", Some("便签"), "正文内容").unwrap();
        assert_eq!((a, s), (1, 0));
        assert_eq!(d.prompts[0].title, "便签");

        let mut d2 = AppData::default();
        let (a2, s2) = import_dispatch(&mut d2, "txt", None, "正文内容").unwrap();
        assert_eq!((a2, s2), (1, 0));
        assert_eq!(d2.prompts[0].title, "导入文本", "无文件名时用兜底标题");

        let mut d3 = AppData::default();
        let (a3, s3) = import_dispatch(&mut d3, "txt", Some("空白"), "   \n  ").unwrap();
        assert_eq!((a3, s3), (0, 1), "空白内容的 txt 应计为跳过");
    }

    #[test]
    fn dispatch_rejects_unsupported_extension() {
        let mut d = AppData::default();
        let err = import_dispatch(&mut d, "pdf", Some("文件"), "内容").unwrap_err();
        assert_eq!(err, "不支持的格式: pdf");
    }

    // ---------- 日期数学 ----------

    #[test]
    fn civil_from_days_known_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(1), (1970, 1, 2));
        assert_eq!(civil_from_days(365), (1971, 1, 1), "1970 年 365 天");
        assert_eq!(civil_from_days(19723), (2024, 1, 1));
        assert_eq!(civil_from_days(19782), (2024, 2, 29), "闰年 2 月 29 日");
    }

    #[test]
    fn time_stamp_has_yyyymmdd_shape() {
        let ts = time_stamp();
        assert_eq!(ts.len(), 8, "应为 YYYYMMDD: {ts}");
        assert!(ts.chars().all(|c| c.is_ascii_digit()));
    }

    // ---------- get_data 凭据脱敏（评审 rc-M9） ----------

    #[test]
    fn redact_credentials_clears_password_and_token_only() {
        let mut d = data_with_prompt();
        d.settings.webdav.password = "dav-secret".into();
        d.settings.gist.token = "ghp_secret".into();
        d.categories.push("开发".into());

        let d = redact_credentials(d);

        assert_eq!(d.settings.webdav.password, "", "WebDAV 密码必须清空");
        assert_eq!(d.settings.gist.token, "", "Gist Token 必须清空");
        // 非凭据字段原样保留：提示词、分类、其他设置
        assert_eq!(d.prompts.len(), 1);
        assert!(d.categories.contains(&"开发".to_string()));
        assert_eq!(d.settings.hotkey, Settings::default().hotkey);
    }
}

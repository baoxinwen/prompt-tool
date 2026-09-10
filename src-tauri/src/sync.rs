use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use base64::Engine;
use serde_json::Value;
use tauri::{AppHandle, Emitter};

use crate::hotkey;
use crate::models::{Settings, SyncPayload, SyncReport};
use crate::store::lock;

const SYNC_FILE: &str = "prompt-tool-sync.json";
const TIMEOUT: Duration = Duration::from_secs(20);
const GIST_API: &str = "https://api.github.com/gists";
const USER_API: &str = "https://api.github.com/user";
/// 同步载荷上限：Gist API 超限返回难懂的 422，WebDAV 服务端行为不一
/// （可能截断/拒收）。两个后端的上传都以此为预检，超限给出可行动的错误
const MAX_SYNC_PAYLOAD_BYTES: usize = 9_000_000;

fn payload_too_large_err() -> String {
    "同步数据超过 9MB（通常是剪贴板历史过大），请在「剪贴板」页清理历史后重试".into()
}

/// 云端响应体 → 同步载荷。空 body 视为「云端暂无数据」；
/// 非空但解析失败必须报错——回退成 None 会让调用方覆盖云端真实数据
fn payload_from_body(body: &str) -> Result<Option<SyncPayload>, String> {
    if body.trim().is_empty() {
        return Ok(None);
    }
    serde_json::from_str::<SyncPayload>(body)
        .map(Some)
        .map_err(|e| format!("云端数据解析失败: {e}"))
}

/// 防止手动同步与自动同步并发重入
static SYNCING: AtomicBool = AtomicBool::new(false);

/// 「已有同步在进行中」的唯一文案：run_sync 产生，自动同步循环据此
/// 区分「跳过」与「失败」（跳过不计退避、不提示错误）。两处必须同源
const ERR_SYNC_BUSY: &str = "已有同步在进行中，请稍候";

/// SYNCING 标志的 panic 安全复位：run_sync_inner 无论正常返回还是
/// panic 展开，Drop 都会把标志清回 false，避免同步从此永久卡死
struct SyncGuard;
impl Drop for SyncGuard {
    fn drop(&mut self) {
        SYNCING.store(false, Ordering::SeqCst);
    }
}

struct HttpResponse {
    status: u16,
    body: String,
    /// classic PAT 的 X-OAuth-Scopes 响应头（说明该 token 拥有的作用域）
    scopes: Option<String>,
}

fn collect_response(result: Result<ureq::Response, ureq::Error>) -> Result<HttpResponse, String> {
    // 响应体读取失败（网络中断、超过 ureq 的 10MiB 读入上限）必须报错——
    // 吞成空串会让同步误判「云端无数据」，随后的上传会覆盖掉云端真实数据
    fn read_body(r: ureq::Response, status: u16) -> Result<String, String> {
        r.into_string()
            .map_err(|e| format!("读取响应体失败 (HTTP {status}): {e}"))
    }
    match result {
        Ok(r) => {
            let status = r.status();
            let scopes = r.header("x-oauth-scopes").map(|s| s.to_string());
            let body = read_body(r, status)?;
            Ok(HttpResponse { status, body, scopes })
        }
        // ureq 把非 2xx 当错误返回，这里统一还原成带状态码的响应交给调用方处理
        Err(ureq::Error::Status(status, r)) => {
            let scopes = r.header("x-oauth-scopes").map(|s| s.to_string());
            let body = read_body(r, status)?;
            Ok(HttpResponse { status, body, scopes })
        }
        Err(e) => Err(redact_url_credentials(&format!("网络请求失败: {e}"))),
    }
}

/// 脱敏错误文本中 URL 的 userinfo（http://user:pass@host → http://***@host）。
/// WebDAV 地址内嵌凭据是常见习惯，而 ureq 传输错误的 Display 携带请求 URL，
/// 不脱敏会随 eprintln 日志与 sync-done 事件进入 stderr 和前端状态条（评审 I13）
fn redact_url_credentials(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find("://") {
        out.push_str(&rest[..pos + 3]);
        rest = &rest[pos + 3..];
        let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        let authority = &rest[..end];
        match authority.rsplit_once('@') {
            Some((_userinfo, host)) if !host.is_empty() => {
                out.push_str("***@");
                out.push_str(host);
            }
            _ => out.push_str(authority),
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

// ---------- WebDAV 后端 ----------

#[derive(Clone)]
struct DavClient {
    agent: ureq::Agent,
    base_url: String,
    username: String,
    password: String,
}

impl DavClient {
    fn new(url: &str, username: &str, password: &str) -> Self {
        Self {
            agent: ureq::AgentBuilder::new().timeout(TIMEOUT).build(),
            base_url: url.trim().trim_end_matches('/').to_string(),
            username: username.to_string(),
            password: password.to_string(),
        }
    }

    fn basic_auth(&self) -> String {
        let raw = format!("{}:{}", self.username, self.password);
        format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(raw)
        )
    }

    fn request(
        &self,
        method: &str,
        url: &str,
        body: Option<String>,
    ) -> Result<HttpResponse, String> {
        let method = method.to_uppercase();
        let req = self
            .agent
            .request(&method, url)
            .set("Authorization", &self.basic_auth());
        let result = match body {
            Some(b) => req
                .set("Content-Type", "application/json; charset=utf-8")
                .send_string(&b),
            None => req.call(),
        };
        collect_response(result)
    }

    /// 解析出 scheme://host[:port] 与路径段
    fn split_url(&self) -> Result<(String, Vec<String>), String> {
        let (scheme, rest) = self
            .base_url
            .split_once("://")
            .ok_or("WebDAV 地址无效（缺少 http(s):// 前缀）")?;
        let (host, path) = match rest.split_once('/') {
            Some((h, p)) => (h.to_string(), p.to_string()),
            None => (rest.to_string(), String::new()),
        };
        let segments: Vec<String> = path
            .split('/')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
        Ok((format!("{scheme}://{host}"), segments))
    }

    /// 确保 WebDAV 目录存在：逐级 MKCOL（已存在 405 / 冲突 409 视为成功）
    fn ensure_dir(&self) -> Result<(), String> {
        let (origin, segments) = self.split_url()?;
        let mut path = String::new();
        for seg in segments.iter() {
            path.push('/');
            path.push_str(seg);
            let resp = self.request("MKCOL", &format!("{origin}{path}"), None)?;
            if (300..400).contains(&resp.status) {
                return Err(format!(
                    "网盘地址发生重定向 (HTTP {})，请改用重定向后的最终地址（通常需 https://）",
                    resp.status
                ));
            }
            let ok = (200..300).contains(&resp.status) || resp.status == 405 || resp.status == 409;
            if !ok {
                return Err(format!(
                    "创建网盘目录失败 (HTTP {})，请在网盘中手动创建该目录",
                    resp.status
                ));
            }
        }
        Ok(())
    }

    fn file_url(&self) -> String {
        format!("{}/{}", self.base_url, SYNC_FILE)
    }

    fn fetch(&self) -> Result<Option<SyncPayload>, String> {
        self.ensure_dir()?;
        let resp = self.request("GET", &self.file_url(), None)?;
        if resp.status == 404 {
            return Ok(None);
        }
        if !(200..300).contains(&resp.status) {
            return Err(format!(
                "下载云端数据失败 (HTTP {})，请检查地址与账号",
                resp.status
            ));
        }
        payload_from_body(&resp.body)
    }

    fn upload(&self, payload: &SyncPayload) -> Result<Option<String>, String> {
        // 与 Gist 上传同款预检，且先于任何网络操作：超限提前报错，
        // 不让服务端截断/拒收
        let json = serde_json::to_string(payload).map_err(|e| e.to_string())?;
        if json.len() > MAX_SYNC_PAYLOAD_BYTES {
            return Err(payload_too_large_err());
        }
        self.ensure_dir()?;
        let resp = self.request("PUT", &self.file_url(), Some(json))?;
        if (200..300).contains(&resp.status) {
            Ok(None)
        } else {
            Err(format!("上传失败 (HTTP {})，请确认目录可写", resp.status))
        }
    }

    fn test(&self) -> Result<String, String> {
        self.ensure_dir()?;
        let probe = format!("{}/.prompt-tool-probe", self.base_url);
        self.request("PUT", &probe, Some("ok".into()))?;
        let body = self.request("GET", &probe, None)?.body;
        let _ = self.request("DELETE", &probe, None);
        if body.contains("ok") {
            Ok("连接成功，目录可读写".into())
        } else {
            Err("目录可访问，但读写校验失败".into())
        }
    }
}

// ---------- GitHub Gist 后端 ----------

#[derive(Clone)]
struct GistClient {
    agent: ureq::Agent,
    token: String,
    gist_id: Option<String>,
}

impl GistClient {
    fn new(token: &str, gist_id: &str) -> Self {
        Self {
            agent: ureq::AgentBuilder::new().timeout(TIMEOUT).build(),
            token: token.trim().to_string(),
            gist_id: {
                let id = gist_id.trim();
                (!id.is_empty()).then(|| id.to_string())
            },
        }
    }

    fn request(
        &self,
        method: &str,
        url: &str,
        body: Option<String>,
    ) -> Result<HttpResponse, String> {
        let method = method.to_uppercase();
        let req = self
            .agent
            .request(&method, url)
            .set("Authorization", &format!("Bearer {}", self.token))
            .set("User-Agent", "Prompt Tool")
            .set("Accept", "application/vnd.github+json");
        let result = match body {
            Some(b) => req
                .set("Content-Type", "application/json; charset=utf-8")
                .send_string(&b),
            None => req.call(),
        };
        collect_response(result)
    }

    /// 从 GET /gists/{id} 响应中取出数据文件内容（过大时回退 raw_url）
    fn extract_content(&self, body: &str) -> Result<SyncPayload, String> {
        let v: Value =
            serde_json::from_str(body).map_err(|e| format!("Gist 响应解析失败: {e}"))?;
        let file = v
            .get("files")
            .and_then(|f| f.get(SYNC_FILE))
            .ok_or("Gist 中未找到数据文件（可能不是 Prompt Tool 的同步 Gist）")?;
        let truncated = file
            .get("truncated")
            .and_then(|t| t.as_bool())
            .unwrap_or(false);
        let content = if truncated {
            let raw = file
                .get("raw_url")
                .and_then(|u| u.as_str())
                .ok_or("数据文件过大且缺少 raw_url")?;
            self.request("GET", raw, None)?.body
        } else {
            file.get("content")
                .and_then(|c| c.as_str())
                .ok_or("数据文件内容为空")?
                .to_string()
        };
        serde_json::from_str::<SyncPayload>(&content)
            .map_err(|e| format!("云端数据解析失败: {e}"))
    }

    fn fetch(&self) -> Result<Option<SyncPayload>, String> {
        let Some(id) = &self.gist_id else {
            // 尚无 Gist，视为云端暂无数据（首次同步将创建）
            return Ok(None);
        };
        let resp = self.request("GET", &format!("{GIST_API}/{id}"), None)?;
        if resp.status == 404 {
            return Err("Gist 不存在或 Token 无权访问（请检查 Gist ID）".into());
        }
        if !(200..300).contains(&resp.status) {
            return Err(gist_err(resp.status, "读取 Gist"));
        }
        self.extract_content(&resp.body).map(Some)
    }

    fn upload(&self, payload: &SyncPayload) -> Result<Option<String>, String> {
        let content = serde_json::to_string(payload).map_err(|e| e.to_string())?;
        // Gist API 对大文件会返回难懂的 422，这里提前给出可行动的错误
        if content.len() > MAX_SYNC_PAYLOAD_BYTES {
            return Err(payload_too_large_err());
        }
        let body = gist_body(&content);
        match &self.gist_id {
            Some(id) => {
                let resp = self.request("PATCH", &format!("{GIST_API}/{id}"), Some(body))?;
                if (200..300).contains(&resp.status) {
                    Ok(None)
                } else {
                    Err(gist_err(resp.status, "更新 Gist"))
                }
            }
            None => {
                let resp = self.request("POST", GIST_API, Some(body))?;
                if !(200..300).contains(&resp.status) {
                    return Err(gist_err(resp.status, "创建 Gist"));
                }
                let v: Value = serde_json::from_str(&resp.body)
                    .map_err(|e| format!("创建 Gist 响应解析失败: {e}"))?;
                let id = v
                    .get("id")
                    .and_then(|i| i.as_str())
                    .ok_or("创建 Gist 响应中缺少 id")?
                    .to_string();
                Ok(Some(id))
            }
        }
    }

    fn test(&self) -> Result<String, String> {
        let resp = self.request("GET", USER_API, None)?;
        if !(200..300).contains(&resp.status) {
            return Err(gist_err(resp.status, "验证 Token"));
        }
        // classic PAT 会携带 X-OAuth-Scopes：缺 gist 作用域时写操作必然失败，
        // 必须在这里拦下（GET /user 本身不需要任何作用域，会造成"测试成功"的假象）
        if let Some(scopes) = &resp.scopes {
            let has_gist = scopes
                .split(',')
                .any(|s| s.trim().eq_ignore_ascii_case("gist"));
            if !has_gist {
                return Err(
                    "Token 缺少 gist 作用域：请在 GitHub 生成 Token 时勾选 gist 权限后重试"
                        .into(),
                );
            }
        }
        let login = serde_json::from_str::<Value>(&resp.body)
            .ok()
            .and_then(|v| v.get("login").and_then(|l| l.as_str()).map(String::from))
            .unwrap_or_default();
        let account = if login.is_empty() {
            "Token 有效".to_string()
        } else {
            format!("Token 有效（账号 {login}）")
        };
        match &self.gist_id {
            Some(id) => {
                let r = self.request("GET", &format!("{GIST_API}/{id}"), None)?;
                if r.status == 404 {
                    return Err(format!("{account}，但无法访问该 Gist（不存在或无权限）"));
                }
                if !(200..300).contains(&r.status) {
                    return Err(gist_err(r.status, "验证 Gist"));
                }
                Ok(format!("{account}，Gist 可访问"))
            }
            None => Ok(format!("{account}，首次同步时将自动创建 Gist")),
        }
    }
}

fn gist_err(status: u16, ctx: &str) -> String {
    match status {
        401 => "GitHub Token 无效（可能已在 GitHub 侧被重新生成或删除）——请重新生成 Token 并在同步设置中更新".into(),
        403 => "GitHub API 拒绝访问（检查 Token 的 gist 权限或速率限制）".into(),
        404 => "Gist 不存在或 Token 无权访问".into(),
        422 => "GitHub 拒绝了请求（数据格式或参数错误）".into(),
        s => format!("GitHub 请求失败 (HTTP {s})，{ctx}"),
    }
}

fn gist_body(content: &str) -> String {
    let mut files = serde_json::Map::new();
    files.insert(
        SYNC_FILE.to_string(),
        serde_json::json!({ "content": content }),
    );
    serde_json::json!({
        "description": "Prompt Tool sync data",
        "public": false,
        "files": Value::Object(files),
    })
    .to_string()
}

// ---------- 后端抽象 ----------

enum SyncBackend {
    Dav(DavClient),
    Gist(GistClient),
}

impl SyncBackend {
    fn fetch(&self) -> Result<Option<SyncPayload>, String> {
        match self {
            SyncBackend::Dav(c) => c.fetch(),
            SyncBackend::Gist(c) => c.fetch(),
        }
    }

    /// 返回 Ok(Some(id)) 表示新建了 Gist，需要把 id 写回配置
    fn upload(&self, payload: &SyncPayload) -> Result<Option<String>, String> {
        match self {
            SyncBackend::Dav(c) => c.upload(payload),
            SyncBackend::Gist(c) => c.upload(payload),
        }
    }
}

fn select_backend(settings: &Settings) -> Result<SyncBackend, String> {
    match settings.sync_provider.as_str() {
        "gist" => {
            if settings.gist.token.trim().is_empty() {
                return Err("尚未配置 GitHub Token，请在「云同步」页填写".into());
            }
            Ok(SyncBackend::Gist(GistClient::new(
                &settings.gist.token,
                &settings.gist.gist_id,
            )))
        }
        _ => {
            if settings.webdav.url.trim().is_empty() {
                return Err("尚未配置 WebDAV 地址，请在「云同步」页填写".into());
            }
            Ok(SyncBackend::Dav(DavClient::new(
                &settings.webdav.url,
                &settings.webdav.username,
                &settings.webdav.password,
            )))
        }
    }
}

/// 剪贴板同步范围语义（评审 I11）：sync_clipboard=false 时剪贴板是
/// 「不同步范围」，上传侧必须保留云端既有剪贴板——上传载荷此时为空数组，
/// 而 WebDAV PUT / Gist PATCH 是单文件整体替换语义，直接上传会静默清空
/// 云端剪贴板副本（与下载方向 r.clipboard.clear() 的保护不对称）。
/// 云端尚无数据（remote=None）时保持空数组即可
fn apply_clipboard_scope(payload: &mut SyncPayload, remote: Option<&SyncPayload>, sync_clipboard: bool) {
    if sync_clipboard {
        return;
    }
    if let Some(r) = remote {
        payload.clipboard = r.clipboard.clone();
    }
}

/// pull 的数据整体替换（纯逻辑，可测，评审 2026-09-10 I15 抽取）。
/// 替换范围：categories/prompts/tombstones 全量；剪贴板按作用域——开启时取
/// 云端条目，关闭时文本剪贴板是纯本地数据原样保留；图片条目始终保留本机的
/// （云端载荷不含图片文件）。最后清掉与主键/捕获键冲突或重复的独立快捷键：
/// pull 与 merge 必须同样有 sanitize 兜底，否则冲突键注册失败但数据里
/// hotkey 保留，按键静默失效，之后 merge 还会把它清空并传播回云端
fn apply_pull(local: &mut crate::models::AppData, remote: &SyncPayload, sync_clipboard: bool) {
    local.categories = remote.categories.clone();
    local.prompts = remote.prompts.clone();
    let mut clip = if sync_clipboard {
        // 与 merge 同口径：跳过云端残留的图片条目（评审 2026-09-10 M7#5）
        remote
            .clipboard
            .iter()
            .filter(|i| !i.is_image())
            .cloned()
            .collect::<Vec<_>>()
    } else {
        local
            .clipboard
            .iter()
            .filter(|i| !i.is_image())
            .cloned()
            .collect()
    };
    clip.extend(local.clipboard.iter().filter(|i| i.is_image()).cloned());
    clip.sort_by_key(|i| std::cmp::Reverse(i.copied_at));
    clip.truncate(crate::models::MAX_CLIPBOARD_ITEMS);
    local.clipboard = clip;
    local.tombstones = remote.tombstones.clone();
    crate::hotkey::sanitize_prompt_hotkeys(local);
}

/// 条目级合并：内容按 updated_at 取新者；使用统计（use_count/last_used_at）
/// 单调收敛取 max——它们的变化不 bump updated_at，不能参与 LWW，否则
/// 一端的使用计数永远传不到另一端，且会被另一端的内容编辑整体覆盖。
fn merge(local: &mut crate::models::AppData, remote: &SyncPayload) -> (u64, u64, u64) {
    let mut added: u64 = 0;
    let mut updated: u64 = 0;

    for c in &remote.categories {
        local.ensure_category(c);
    }

    for rp in &remote.prompts {
        match local.prompts.iter_mut().find(|p| p.id == rp.id) {
            None => {
                local.prompts.push(rp.clone());
                added += 1;
            }
            Some(lp) => {
                if rp.updated_at > lp.updated_at {
                    let use_count = lp.use_count.max(rp.use_count);
                    let last_used_at = lp.last_used_at.max(rp.last_used_at);
                    *lp = rp.clone();
                    lp.use_count = use_count;
                    lp.last_used_at = last_used_at;
                    updated += 1;
                } else if rp.use_count > lp.use_count || rp.last_used_at > lp.last_used_at {
                    lp.use_count = lp.use_count.max(rp.use_count);
                    lp.last_used_at = lp.last_used_at.max(rp.last_used_at);
                }
            }
        }
    }

    for ri in &remote.clipboard {
        // 防御（评审 2026-09-10 M7#5）：上传侧已过滤图片条目，但云端可能残留
        // 旧版本数据或被外部修改；并入引用本机不存在文件的图片条目会成为
        // 永久裂图，且 GC 对账会视其为"有引用"而永不清理
        if ri.is_image() {
            continue;
        }
        match local.clipboard.iter_mut().find(|i| i.id == ri.id) {
            None => {
                local.clipboard.push(ri.clone());
                added += 1;
            }
            Some(li) => {
                if ri.copied_at > li.copied_at {
                    *li = ri.clone();
                }
            }
        }
    }

    for rt in &remote.tombstones {
        match local.tombstones.iter_mut().find(|t| t.id == rt.id) {
            None => local.tombstones.push(rt.clone()),
            Some(lt) => lt.at = lt.at.max(rt.at),
        }
    }

    let before = local.prompts.len() + local.clipboard.len();
    let tombstones = local.tombstones.clone();
    local
        .prompts
        .retain(|p| !tombstones.iter().any(|t| t.id == p.id && t.at > p.updated_at));
    local
        .clipboard
        .retain(|i| !tombstones.iter().any(|t| t.id == i.id && t.at > i.copied_at));
    let removed = (before - (local.prompts.len() + local.clipboard.len())) as u64;

    // 双端剪贴板做并集可能突破上限，收敛到统一上限（保留最新）
    if local.clipboard.len() > crate::models::MAX_CLIPBOARD_ITEMS {
        local
            .clipboard
            .sort_by_key(|i| std::cmp::Reverse(i.copied_at));
        local
            .clipboard
            .truncate(crate::models::MAX_CLIPBOARD_ITEMS);
    }

    (added, updated, removed)
}

/// 执行同步。direction: "merge"（双向合并，默认）| "push"（本机覆盖云端）| "pull"（云端覆盖本机）
pub fn run_sync(app: &AppHandle, direction: &str) -> Result<SyncReport, String> {
    if SYNCING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(ERR_SYNC_BUSY.into());
    }
    let _guard = SyncGuard;
    let result = run_sync_inner(app, direction);
    // lastSyncAt 记账收口在 run_sync 这一处：merge/push/pull 三条内部路径
    // 只管数据本身，成功统一盖章，失败保持原值
    let mut store = lock(app);
    stamp_last_sync(&mut store, &result)?;
    drop(store);
    result
}

/// 同步收尾记账：成功 → lastSyncAt=当前毫秒并落盘；失败 → 原值不动、不落盘。
/// 抽成不依赖 AppHandle 的 Store 操作，便于对成功/失败两分支做单元测试
fn stamp_last_sync(
    store: &mut crate::store::Store,
    result: &Result<SyncReport, String>,
) -> Result<(), String> {
    if result.is_ok() {
        store.data.settings.last_sync_at = Some(crate::models::now_ms() as i64);
        store.save()?;
    }
    Ok(())
}

fn run_sync_inner(app: &AppHandle, direction: &str) -> Result<SyncReport, String> {
    let (backend, payload, snap_mark, sync_clipboard) = {
        let store = lock(app);
        (
            select_backend(&store.data.settings)?,
            SyncPayload::from(&store.data),
            store.mutations,
            store.data.settings.sync_clipboard,
        )
    };

    match direction {
        "push" => {
            // 关闭剪贴板同步时先取云端既有剪贴板填入载荷，避免空数组
            // 借整体替换语义清空云端副本（评审 I11）；云端无数据则保持为空。
            // 开启剪贴板同步时载荷原样上传，无需 fetch（评审 2026-09-10 M7#6）：
            // 省一次网络往返，云端 GET 瞬时故障也不再阻断本可成功的上传
            let mut payload = payload;
            if !sync_clipboard {
                let remote_for_scope = backend.fetch()?;
                apply_clipboard_scope(&mut payload, remote_for_scope.as_ref(), sync_clipboard);
            }
            let new_gist = backend.upload(&payload)?;
            persist_new_gist_id(app, new_gist)?;
            // 上传期间若有新变更，保持 dirty，让下个自动同步周期补传
            let mut store = lock(app);
            if store.mutations == snap_mark {
                store.dirty_unsynced = false;
            }
            Ok(SyncReport {
                added: 0,
                updated: 0,
                removed: 0,
                message: format!("已上传本机数据（{} 条提示词）", payload.prompts.len()),
            })
        }
        "pull" => {
            let remote = backend.fetch()?.ok_or("云端暂无数据")?;
            {
                let mut store = lock(app);
                apply_pull(&mut store.data, &remote, sync_clipboard);
                store.dirty_unsynced = false;
                store.save()?;
            }
            // pull 整体替换了 prompts：云端带来的独立快捷键要重新注册，
            // 指向已删除提示词的旧注册也要随之释放
            hotkey::register_all(app);
            let _ = app.emit("data-changed", ());
            Ok(SyncReport {
                added: remote.prompts.len() as u64,
                updated: 0,
                removed: 0,
                message: format!("已下载云端数据（{} 条提示词）", remote.prompts.len()),
            })
        }
        _ => {
            let mut remote = backend.fetch()?;
            // 用户关闭剪贴板同步时，不把云端剪贴板合并进本地
            if let Some(r) = &mut remote {
                if !sync_clipboard {
                    r.clipboard.clear();
                }
            }
            let (added, updated, removed, detail) = match &remote {
                Some(r) => {
                    let mut store = lock(app);
                    let (a, u, rm) = merge(&mut store.data, r);
                    hotkey::sanitize_prompt_hotkeys(&mut store.data);
                    // 合并结果先落盘：后续上传失败时内存与磁盘保持一致，
                    // 直接退出也不会丢掉已合并的数据（云端未更新则下次
                    // 同步重传，merge 幂等）
                    store.save()?;
                    (a, u, rm, format!("新增 {a} 条，更新 {u} 条，删除 {rm} 条"))
                }
                None => (0, 0, 0, "云端暂无数据，已上传本机数据".to_string()),
            };
            // 上传放在锁外，避免网络请求阻塞剪贴板监听等持锁方
            let (mut merged, merged_mark) = {
                let store = lock(app);
                (SyncPayload::from(&store.data), store.mutations)
            };
            // 关闭剪贴板同步时上传侧保留云端既有剪贴板，与下载方向的
            // 保护对称（评审 I11）
            apply_clipboard_scope(&mut merged, remote.as_ref(), sync_clipboard);
            let new_gist = backend.upload(&merged)?;
            persist_new_gist_id(app, new_gist)?;
            {
                let mut store = lock(app);
                store.data.seeded = true;
                // 快照后又有本地变更：不能清 dirty，否则这次变更在下次修改前都不会再上传
                if store.mutations == merged_mark {
                    store.dirty_unsynced = false;
                }
                store.save()?;
            }
            if remote.is_some() {
                // merge 可能带来/清掉提示词的独立快捷键，重新注册保持系统键位与数据一致
                hotkey::register_all(app);
                let _ = app.emit("data-changed", ());
            }
            Ok(SyncReport {
                added,
                updated,
                removed,
                message: format!("同步完成：{detail}，并已上传云端"),
            })
        }
        other => Err(format!("未知同步方向: {other}")),
    }
}

/// gist id 的本地 sidecar：store.save 失败（磁盘满等）时 Gist 已创建且含全量
/// 数据副本，id 只在这里有副本；下次启动恢复，避免下轮自动同步再建一个
/// 孤儿 Gist（评审 2026-09-10 I17）
const SYNC_STATE_FILE: &str = "sync-state.json";

fn write_gist_sidecar(dir: &std::path::Path, gist_id: &str) -> Result<(), String> {
    let tmp = dir.join(format!("{SYNC_STATE_FILE}.tmp"));
    let path = dir.join(SYNC_STATE_FILE);
    let body = serde_json::json!({ "gist_id": gist_id }).to_string();
    std::fs::write(&tmp, body).map_err(|e| format!("写入同步状态失败: {e}"))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("提交同步状态失败: {e}"))
}

fn read_gist_sidecar(dir: &std::path::Path) -> Option<String> {
    let text = std::fs::read_to_string(dir.join(SYNC_STATE_FILE)).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    v.get("gist_id")
        .and_then(|g| g.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from)
}

/// 启动时从 sidecar 恢复 gist_id（设置里为空且 sidecar 有值才生效）
pub fn restore_gist_id_from_sidecar(app: &AppHandle) {
    let Some(id) = read_gist_sidecar(&{
        let store = lock(app);
        store.data_dir()
    }) else {
        return;
    };
    {
        let mut store = lock(app);
        if !store.data.settings.gist.gist_id.trim().is_empty() {
            return; // 设置里已有 id：sidecar 是陈旧副本，不动
        }
        store.data.settings.gist.gist_id = id;
        if store.save().is_err() {
            return; // 落盘失败保持内存态即可，sidecar 仍在
        }
    }
    let _ = app.emit("data-changed", ());
}

/// 首次上传时自动创建的 Gist id 写回配置
fn persist_new_gist_id(app: &AppHandle, new_gist: Option<String>) -> Result<(), String> {
    let Some(id) = new_gist else {
        return Ok(());
    };
    // 先落 sidecar 再 save：save 失败（磁盘满等）时 id 至少有本地副本可恢复（I17）
    write_gist_sidecar(&{
        let store = lock(app);
        store.data_dir()
    }, &id)?;
    {
        let mut store = lock(app);
        store.data.settings.gist.gist_id = id;
        store.save()?;
    }
    let _ = app.emit("data-changed", ());
    Ok(())
}

/// WebDAV 连接测试（供 webdav_test 命令使用）
pub fn test_connection(url: &str, username: &str, password: &str) -> Result<String, String> {
    DavClient::new(url, username, password).test()
}

/// GitHub Gist 连接测试（供 gist_test 命令使用）
pub fn gist_test(token: &str, gist_id: &str) -> Result<String, String> {
    if token.trim().is_empty() {
        return Err("请先填写 GitHub Token".into());
    }
    GistClient::new(token, gist_id).test()
}

/// 自动同步：轮询 dirty 标志，按 provider 配置后台合并同步。
/// 成功静默（前端有 data-changed 即可）；失败通过 sync-done 事件上报，
/// 并按连续失败次数退避，避免离线时每 15s 一轮、每轮 20s 超时的空转
pub fn spawn_auto_sync(app: AppHandle) {
    std::thread::spawn(move || {
        let mut first_round = true;
        let mut failures: u32 = 0;
        loop {
            let backoff = Duration::from_secs(15) * failures.min(16);
            let wait = if first_round {
                Duration::from_secs(10)
            } else {
                Duration::from_secs(15) + backoff
            };
            std::thread::sleep(wait);

            let (auto_on, dirty) = {
                let store = lock(&app);
                let s = &store.data.settings;
                let auto_on = match s.sync_provider.as_str() {
                    "gist" => s.gist.enabled && s.gist.auto_sync,
                    _ => s.webdav.enabled && s.webdav.auto_sync,
                };
                (auto_on, store.dirty_unsynced)
            };

            if auto_on && (first_round || dirty) {
                match run_sync(&app, "merge") {
                    Ok(_) => failures = 0,
                    Err(e) => {
                        // 手动同步占用不算自动同步失败，不计入退避
                        if e == ERR_SYNC_BUSY {
                            eprintln!("[prompt-tool] 自动同步跳过（手动同步进行中）");
                        } else {
                            failures = failures.saturating_add(1);
                            eprintln!("[prompt-tool] 自动同步失败: {e}");
                            let _ = app.emit(
                                "sync-done",
                                serde_json::json!({
                                    "added": 0, "updated": 0, "removed": 0,
                                    "message": e, "ok": false, "auto": true,
                                }),
                            );
                        }
                    }
                }
            }
            first_round = false;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AppData, ClipboardItem, Prompt, Settings, Tombstone, MAX_CLIPBOARD_ITEMS};

    fn prompt(id: &str, content: &str, updated_at: u64, use_count: u64, last_used_at: u64) -> Prompt {
        Prompt {
            id: id.to_string(),
            title: format!("标题-{id}"),
            content: content.to_string(),
            category: "开发".to_string(),
            tags: vec![],
            pinned: false,
            hotkey: String::new(),
            use_count,
            last_used_at,
            created_at: 1,
            updated_at,
        }
    }

    fn text_clip(id: &str, content: &str, copied_at: u64) -> ClipboardItem {
        ClipboardItem {
            id: id.to_string(),
            content: content.to_string(),
            copied_at,
            kind: "text".into(),
            image: None,
        }
    }

    fn payload(
        categories: Vec<&str>,
        prompts: Vec<Prompt>,
        clipboard: Vec<ClipboardItem>,
        tombstones: Vec<Tombstone>,
    ) -> SyncPayload {
        SyncPayload {
            version: 1,
            exported_at: 0,
            categories: categories.into_iter().map(String::from).collect(),
            prompts,
            clipboard,
            tombstones,
        }
    }

    #[test]
    fn merge_adds_remote_only_prompts_and_categories() {
        let mut local = AppData::default();
        let remote = payload(
            vec!["开发", "新分类"],
            vec![prompt("r1", "远端正文", 100, 0, 0)],
            vec![],
            vec![],
        );
        let (added, updated, removed) = merge(&mut local, &remote);
        assert_eq!((added, updated, removed), (1, 0, 0));
        assert_eq!(local.prompts[0].content, "远端正文");
        assert!(local.categories.contains(&"新分类".to_string()));
    }

    // ---------- 评审 2026-09-10 I15：pull 必须有快捷键 sanitize 兜底 ----------

    #[test]
    fn apply_pull_sanitizes_conflicting_prompt_hotkeys() {
        let mut local = AppData::default();
        local.settings.hotkey = "alt+q".into();
        let mut remote_prompt = prompt("r1", "远端正文", 100, 0, 0);
        remote_prompt.hotkey = "alt+q".into(); // 与 B 端主键冲突
        let remote = payload(vec![], vec![remote_prompt], vec![], vec![]);

        apply_pull(&mut local, &remote, false);
        assert_eq!(
            local.prompts[0].hotkey, "",
            "与主键冲突的快捷键必须在 pull 后清空，否则按键静默失效并被后续 merge 传播删除"
        );
    }

    #[test]
    fn apply_pull_clipboard_scope_and_image_retention() {
        let mut local = AppData::default();
        local.clipboard.push(text_clip("l1", "本地文本", 100));
        let mut local_img = text_clip("li", "", 90);
        local_img.kind = "image".into();
        local_img.image = Some(crate::models::ImageRef {
            file: "li.png".into(),
            width: 4,
            height: 4,
        });
        local.clipboard.push(local_img);

        // 关闭剪贴板同步：文本保留本机的，云端文本不进来，本机图片必须保留
        let remote = payload(
            vec![],
            vec![],
            vec![text_clip("r1", "云端文本", 200)],
            vec![],
        );
        apply_pull(&mut local, &remote, false);
        let ids: Vec<&str> = local.clipboard.iter().map(|c| c.id.as_str()).collect();
        assert!(ids.contains(&"l1"), "关闭作用域时本地文本保留");
        assert!(!ids.contains(&"r1"), "关闭作用域时云端文本不并入");
        assert!(ids.contains(&"li"), "图片条目始终保留本机");

        // 开启剪贴板同步：取云端文本，本机图片仍保留
        apply_pull(&mut local, &remote, true);
        let ids: Vec<&str> = local.clipboard.iter().map(|c| c.id.as_str()).collect();
        assert!(ids.contains(&"r1"), "开启作用域时取云端文本");
        assert!(ids.contains(&"li"), "开启作用域时本机图片仍保留");
    }

    // ---------- 评审 2026-09-10 I17：gist id sidecar ----------

    #[test]
    fn gist_sidecar_roundtrip_and_corrupt_tolerance() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_gist_sidecar(dir.path()), None, "无 sidecar 返回 None");

        write_gist_sidecar(dir.path(), "abc123").unwrap();
        assert_eq!(read_gist_sidecar(dir.path()).as_deref(), Some("abc123"));

        std::fs::write(dir.path().join(SYNC_STATE_FILE), "不是合法 JSON").unwrap();
        assert_eq!(read_gist_sidecar(dir.path()), None, "坏内容按无 sidecar 处理");

        std::fs::write(dir.path().join(SYNC_STATE_FILE), r#"{"gist_id":""}"#).unwrap();
        assert_eq!(read_gist_sidecar(dir.path()), None, "空 id 视为无 sidecar");
    }

    #[test]
    fn merge_keeps_local_only_prompts() {
        let mut local = AppData::default();
        local.prompts.push(prompt("l1", "本地独有", 100, 0, 0));
        let remote = payload(vec![], vec![], vec![], vec![]);
        let report = merge(&mut local, &remote);
        assert_eq!(report, (0, 0, 0));
        assert_eq!(local.prompts.len(), 1);
        assert_eq!(local.prompts[0].content, "本地独有");
    }

    #[test]
    fn merge_remote_newer_takes_content_but_keeps_max_usage() {
        let mut local = AppData::default();
        // 本机：旧内容，但使用统计更高
        local.prompts.push(prompt("p1", "旧内容", 100, 5, 900));
        // 远端：更新的编辑，但使用统计低
        let remote = payload(vec![], vec![prompt("p1", "新内容", 200, 2, 100)], vec![], vec![]);

        let (added, updated, removed) = merge(&mut local, &remote);
        assert_eq!((added, updated, removed), (0, 1, 0));
        assert_eq!(local.prompts[0].content, "新内容", "远端更新时间新，内容应取胜");
        assert_eq!(local.prompts[0].use_count, 5, "使用计数必须单调收敛取 max");
        assert_eq!(local.prompts[0].last_used_at, 900, "最后使用时间必须单调收敛取 max");
    }

    #[test]
    fn merge_local_newer_keeps_content_but_absorbs_remote_usage() {
        let mut local = AppData::default();
        local.prompts.push(prompt("p1", "本地新内容", 200, 1, 0));
        let remote = payload(vec![], vec![prompt("p1", "远端旧内容", 100, 7, 555)], vec![], vec![]);

        let (added, updated, _) = merge(&mut local, &remote);
        assert_eq!((added, updated), (0, 0), "本机较新不算更新");
        assert_eq!(local.prompts[0].content, "本地新内容");
        assert_eq!(local.prompts[0].use_count, 7, "远端更高的使用计数要被吸收");
        assert_eq!(local.prompts[0].last_used_at, 555);
    }

    #[test]
    fn merge_propagates_tombstone_deletions() {
        let mut local = AppData::default();
        local.prompts.push(prompt("p1", "将被删除", 100, 0, 0));
        local.prompts.push(prompt("p2", "不受影响", 300, 0, 0));
        local.clipboard.push(text_clip("c1", "旧剪贴", 100));
        let remote = payload(
            vec![],
            vec![],
            vec![],
            vec![Tombstone { id: "p1".into(), at: 200 }, Tombstone { id: "c1".into(), at: 150 }],
        );
        let (_, _, removed) = merge(&mut local, &remote);
        assert_eq!(removed, 2);
        assert!(local.prompts.iter().all(|p| p.id != "p1"));
        assert!(local.clipboard.iter().all(|i| i.id != "c1"));
        assert!(local.prompts.iter().any(|p| p.id == "p2"));
    }

    #[test]
    fn merge_tombstone_older_than_edit_keeps_entry() {
        let mut local = AppData::default();
        // 删除之后又在另一端编辑过（updated_at 晚于墓碑）：复活保留
        local.prompts.push(prompt("p1", "删除后又编辑", 300, 0, 0));
        let remote = payload(vec![], vec![], vec![], vec![Tombstone { id: "p1".into(), at: 200 }]);
        let (_, _, removed) = merge(&mut local, &remote);
        assert_eq!(removed, 0);
        assert_eq!(local.prompts.len(), 1);
    }

    #[test]
    fn merge_restored_clipboard_item_survives_tombstones_both_ways() {
        // 评审 R1 回归：撤销清空后，恢复条目的 copied_at 已刷新为恢复时刻
        // （晚于 clear 留下的墓碑 at）。merge 收尾 retain 与远端墓碑回灌
        // 都不得把刚恢复的条目判删——撤销不能被同步静默回退
        let mut local = AppData::default();
        local.clipboard.push(text_clip("c1", "恢复的条目", 1_000));
        local.tombstones.push(Tombstone { id: "c1".into(), at: 500 });
        // 远端也带着 clear 上传的墓碑回来
        let remote = payload(vec![], vec![], vec![], vec![Tombstone { id: "c1".into(), at: 500 }]);
        let (_, _, removed) = merge(&mut local, &remote);
        assert_eq!(removed, 0, "copied_at 晚于墓碑 at 的恢复条目不得被重删");
        assert_eq!(local.clipboard.len(), 1);
        assert_eq!(local.clipboard[0].content, "恢复的条目");
    }

    #[test]
    fn merge_clipboard_last_writer_wins_by_copied_at() {
        let mut local = AppData::default();
        local.clipboard.push(text_clip("c1", "本地旧", 100));
        let remote = payload(vec![], vec![], vec![text_clip("c1", "远端新", 200)], vec![]);
        merge(&mut local, &remote);
        assert_eq!(local.clipboard[0].content, "远端新");

        let mut local2 = AppData::default();
        local2.clipboard.push(text_clip("c1", "本地新", 200));
        let remote2 = payload(vec![], vec![], vec![text_clip("c1", "远端旧", 100)], vec![]);
        merge(&mut local2, &remote2);
        assert_eq!(local2.clipboard[0].content, "本地新");
    }

    #[test]
    fn merge_caps_clipboard_union_at_limit_keeping_newest() {
        let mut local = AppData::default();
        for i in 0..600u64 {
            local.clipboard.push(text_clip(&format!("l{i}"), "x", i));
        }
        let remote_clips: Vec<ClipboardItem> = (0..600u64)
            .map(|i| text_clip(&format!("r{i}"), "y", 600 + i))
            .collect();
        let remote = payload(vec![], vec![], remote_clips, vec![]);

        let (added, _, removed) = merge(&mut local, &remote);
        assert_eq!(added, 600, "远端 600 条全部计入新增");
        assert_eq!(removed, 0, "超限截断不计入墓碑删除");
        assert_eq!(local.clipboard.len(), MAX_CLIPBOARD_ITEMS);
        let oldest = local.clipboard.iter().map(|i| i.copied_at).min().unwrap();
        assert_eq!(oldest, 200, "应保留最新的 1000 条（copied_at 200..1200）");
        assert!(!local.clipboard.iter().any(|i| i.id == "l0"));
        assert!(local.clipboard.iter().any(|i| i.id == "r599"));
    }

    #[test]
    fn merge_empty_payload_is_noop() {
        let mut local = AppData::default();
        local.prompts.push(prompt("p1", "保持", 1, 0, 0));
        let before_len = local.prompts.len();
        let report = merge(&mut local, &payload(vec![], vec![], vec![], vec![]));
        assert_eq!(report, (0, 0, 0));
        assert_eq!(local.prompts.len(), before_len);
        // settings 不在同步载荷内，绝不能被 merge 触碰
        let s: Settings = Settings::default();
        assert_eq!(local.settings.sync_provider, s.sync_provider);
    }

    // ---------- 云端响应体解码边界（评审 C-1 回归守卫） ----------

    #[test]
    fn payload_from_body_treats_empty_body_as_no_remote_data() {
        // 云端文件确实为空（如他端异常清空）→ 无数据可丢，按「云端暂无」处理
        assert!(payload_from_body("").unwrap().is_none());
        assert!(payload_from_body("  \n\t ").unwrap().is_none());
    }

    #[test]
    fn payload_from_body_errors_on_malformed_body_instead_of_none() {
        // 非空但解析失败必须报错：回退成 None 会让调用方覆盖云端真实数据。
        // 常见污染源就是「响应体读取失败被吞成空串」的历史 bug，这里锁定
        // 「非空即必须可解析」的语义
        let err = payload_from_body("{\"prompts\": truncated").unwrap_err();
        assert!(err.contains("云端数据解析失败"), "实际错误: {err}");
    }

    #[test]
    fn payload_from_body_parses_valid_payload() {
        let p = payload(vec!["开发"], vec![prompt("p1", "正文", 1, 0, 0)], vec![], vec![]);
        let raw = serde_json::to_string(&p).unwrap();
        let got = payload_from_body(&raw).unwrap().expect("应为 Some");
        assert_eq!(got.prompts.len(), 1);
        assert_eq!(got.prompts[0].id, "p1");
    }

    #[test]
    fn dav_upload_rejects_oversized_payload_before_network() {
        // 预检先于任何网络操作：不可达地址 + 超限载荷 → 必须是「超限」错误
        // 而非连接错误，证明预检在 ensure_dir/PUT 之前生效
        let dav = DavClient::new("http://127.0.0.1:1/prompt-tool", "u", "p");
        let big = prompt("big", &"x".repeat(10_000_000), 0, 0, 0);
        let err = dav
            .upload(&payload(vec![], vec![big], vec![], vec![]))
            .unwrap_err();
        assert!(err.contains("超过 9MB"), "实际错误: {err}");
    }

    #[test]
    fn gist_upload_rejects_oversized_payload_before_network() {
        let gist = GistClient::new("token", "");
        let big = prompt("big", &"x".repeat(10_000_000), 0, 0, 0);
        let err = gist
            .upload(&payload(vec![], vec![big], vec![], vec![]))
            .unwrap_err();
        assert!(err.contains("超过 9MB"), "实际错误: {err}");
    }

    // ---------- lastSyncAt 盖章：成功写 / 失败不写 ----------

    #[test]
    fn stamp_last_sync_writes_timestamp_and_persists_on_success() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = crate::store::Store::test_store(dir.path());
        assert!(store.data.settings.last_sync_at.is_none());

        let ok = Ok(SyncReport {
            added: 1,
            updated: 2,
            removed: 0,
            message: "同步完成".into(),
        });
        stamp_last_sync(&mut store, &ok).unwrap();

        let ts = store
            .data
            .settings
            .last_sync_at
            .expect("同步成功必须写入 lastSyncAt");
        assert!(ts > 0, "时间戳应为当前毫秒");
        let saved: AppData =
            serde_json::from_str(&std::fs::read_to_string(dir.path().join("data.json")).unwrap())
                .unwrap();
        assert_eq!(saved.settings.last_sync_at, Some(ts), "lastSyncAt 必须落盘");
    }

    #[test]
    fn stamp_last_sync_keeps_previous_value_and_skips_save_on_failure() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = crate::store::Store::test_store(dir.path());
        store.data.settings.last_sync_at = Some(42);

        let failed: Result<SyncReport, String> = Err("GitHub Token 无效".into());
        stamp_last_sync(&mut store, &failed).unwrap();

        assert_eq!(
            store.data.settings.last_sync_at,
            Some(42),
            "同步失败不得改动 lastSyncAt"
        );
        assert!(
            !dir.path().join("data.json").exists(),
            "同步失败不得触发落盘"
        );
    }

    // ---------- 上传侧剪贴板范围语义（评审 I11 回归守卫） ----------

    #[test]
    fn clipboard_out_of_scope_preserves_remote_clipboard_on_upload() {
        let mut local = payload(vec![], vec![], vec![], vec![]);
        let remote = payload(vec![], vec![], vec![text_clip("c1", "云端剪贴", 1)], vec![]);
        apply_clipboard_scope(&mut local, Some(&remote), false);
        assert_eq!(local.clipboard.len(), 1, "关闭开关时上传必须保留云端剪贴板，而非以空覆盖");

        // 开关打开：本机剪贴板照常上传
        let mut local2 = payload(vec![], vec![], vec![text_clip("l1", "本机", 2)], vec![]);
        apply_clipboard_scope(&mut local2, Some(&remote), true);
        assert_eq!(local2.clipboard[0].id, "l1");

        // 云端尚无数据：保持空数组
        let mut local3 = payload(vec![], vec![], vec![], vec![]);
        apply_clipboard_scope(&mut local3, None, false);
        assert!(local3.clipboard.is_empty());
    }

    // ---------- 错误文本脱敏（评审 I13 回归守卫） ----------

    #[test]
    fn error_text_redacts_url_userinfo() {
        assert_eq!(
            redact_url_credentials(
                "网络请求失败: error sending request for url (http://user:pass@dav.example.com/dav/prompt-tool-sync.json)"
            ),
            "网络请求失败: error sending request for url (http://***@dav.example.com/dav/prompt-tool-sync.json)"
        );
        // 多个 URL、无 userinfo 的 URL、纯域名、无 URL 的文本
        assert_eq!(
            redact_url_credentials("http://a:b@h1/x http://h2/y"),
            "http://***@h1/x http://h2/y"
        );
        assert_eq!(
            redact_url_credentials("token=abc https://user@host end"),
            "token=abc https://***@host end"
        );
        assert_eq!(redact_url_credentials("没有 URL 的普通错误"), "没有 URL 的普通错误");
    }
}

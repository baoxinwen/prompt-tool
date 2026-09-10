//! 应用内更新：检查 / 下载安装 / 进度事件。
//! 策略逻辑抽成纯函数便于单测，插件 IO 保持薄。

use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime};
use tauri_plugin_updater::UpdaterExt;

/// 检查结果（序列化为 camelCase，与 src/types.ts 的 UpdateStatus 对齐）
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum UpdateStatus {
    UpToDate,
    Available { version: String, notes: String },
    Error { error_kind: String, message: String },
}

/// 进度事件载荷（update://progress）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgress {
    pub downloaded: u64,
    pub total: Option<u64>,
    pub percent: Option<u8>,
}

/// 错误归类：signature / network / unknown。纯函数。
pub fn classify_error(msg: &str) -> &'static str {
    let m = msg.to_ascii_lowercase();
    if m.contains("signature") || m.contains("minisign") {
        "signature"
    } else if m.contains("network")
        || m.contains("connect")
        || m.contains("timeout")
        || m.contains("timed out")
        || m.contains("refused")
        || m.contains("dns")
        || m.contains("proxy")
    {
        "network"
    } else {
        "unknown"
    }
}

/// 下载进度百分比（0-100，total 未知或为 0 时返回 None）。纯函数。
pub fn progress_percent(downloaded: u64, total: Option<u64>) -> Option<u8> {
    total
        .filter(|t| *t > 0)
        .map(|t| ((downloaded.min(t) as u128 * 100) / t as u128) as u8)
}

/// 检查更新；一切失败都归约为 Error 状态（前端不区分 IPC 失败与检查失败）
pub async fn check<R: Runtime>(app: &AppHandle<R>) -> UpdateStatus {
    let updater = match app.updater_builder().build() {
        Ok(u) => u,
        Err(e) => return err_status(&e.to_string()),
    };
    match updater.check().await {
        Ok(Some(update)) => UpdateStatus::Available {
            version: update.version.clone(),
            notes: update.body.clone().unwrap_or_default(),
        },
        Ok(None) => UpdateStatus::UpToDate,
        Err(e) => err_status(&e.to_string()),
    }
}

fn err_status(message: &str) -> UpdateStatus {
    UpdateStatus::Error {
        error_kind: classify_error(message).to_string(),
        message: message.to_string(),
    }
}

/// 下载/安装重入闸门（评审 2026-09-10 I7）：插件自身无并发保护，
/// 前端状态机跨页面卸载可能失效（组件态销毁后误判空闲），这里兜底拒绝并发
static INSTALL_IN_FLIGHT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// RAII 闸门：持有期间 INSTALL_IN_FLIGHT 为 true，Drop（含 panic）自动释放
pub struct InstallGuard;

impl InstallGuard {
    pub fn acquire() -> Option<Self> {
        INSTALL_IN_FLIGHT
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
            )
            .ok()
            .map(|_| Self)
    }
}

impl Drop for InstallGuard {
    fn drop(&mut self) {
        INSTALL_IN_FLIGHT.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

/// 下载并安装；成功时进程被 NSIS 安装器接管退出，随后的 restart() 可能
/// 不再执行，前端 Promise 可能永不 resolve，属预期。
pub async fn download_and_install<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let _guard = InstallGuard::acquire()
        .ok_or_else(|| "已有更新下载/安装正在进行，请稍候".to_string())?;
    let updater = app.updater_builder().build().map_err(|e| e.to_string())?;
    // 插件不缓存检查结果，这里重新 check 拿 Update 对象；latest.json 很小，开销可忽略
    let update = updater
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "当前没有可安装的更新".to_string())?;

    let emitter = app.clone();
    let mut downloaded: u64 = 0;
    update
        .download_and_install(
            move |chunk, total| {
                downloaded += chunk as u64;
                let _ = emitter.emit(
                    "update://progress",
                    UpdateProgress {
                        downloaded,
                        total,
                        percent: progress_percent(downloaded, total),
                    },
                );
            },
            || { /* Windows 上即将退出进程；无需处理 */ },
        )
        .await
        .map_err(|e| e.to_string())?;
    app.restart();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_error_kinds() {
        assert_eq!(classify_error("signature verification failed"), "signature");
        assert_eq!(classify_error("Minisign parse error"), "signature");
        assert_eq!(classify_error("Connection refused"), "network");
        assert_eq!(classify_error("request timeout while fetching"), "network");
        assert_eq!(classify_error("dns error resolving github.com"), "network");
        assert_eq!(classify_error("weird failure"), "unknown");
    }

    #[test]
    fn progress_percent_bounds() {
        assert_eq!(progress_percent(0, Some(200)), Some(0));
        assert_eq!(progress_percent(100, Some(200)), Some(50));
        assert_eq!(progress_percent(250, Some(200)), Some(100)); // 不超过 100
        assert_eq!(progress_percent(10, None), None);
        assert_eq!(progress_percent(10, Some(0)), None); // total=0 视为未知
    }

    #[test]
    fn progress_percent_u64_max_no_overflow() {
        assert_eq!(progress_percent(u64::MAX, Some(u64::MAX)), Some(100));
    }

    #[test]
    fn progress_percent_near_max_no_overflow() {
        assert_eq!(progress_percent(u64::MAX - 5, Some(u64::MAX)), Some(99));
    }

    // 评审 2026-09-10 I7：下载/安装进行中必须拒绝二次进入（RAII 释放 panic 安全）
    #[test]
    fn install_guard_is_exclusive_and_releases_on_drop() {
        {
            let _g = InstallGuard::acquire().expect("首次获取应成功");
            assert!(
                InstallGuard::acquire().is_none(),
                "持有期间二次获取必须被拒绝"
            );
        }
        assert!(
            InstallGuard::acquire().is_some(),
            "Drop 后必须自动释放（含 panic 路径）"
        );
    }
}

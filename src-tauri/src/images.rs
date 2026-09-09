use std::collections::HashSet;
use std::path::{Path, PathBuf};

use base64::Engine;
use tauri::Wry;

/// 图片文件目录：<data_dir>/images。与 data.json 一致走 resolve_data_dir，
/// 保证 PROMPTMATE_DATA_DIR 隔离对图片同样生效（E2E 不污染真实用户数据）
pub fn dir(app: &tauri::AppHandle) -> PathBuf {
    crate::store::resolve_data_dir(app)
        .map(|d| d.join("images"))
        .unwrap_or_else(|_| std::env::temp_dir().join("prompt-tool-images"))
}

fn ensure_dir(app: &tauri::AppHandle) -> PathBuf {
    let d = dir(app);
    let _ = std::fs::create_dir_all(&d);
    d
}

/// 保存 RGBA 原图并生成宽 160 的缩略图，返回文件名
pub fn save_png(
    app: &tauri::AppHandle,
    id: &str,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> Result<String, String> {
    // 零尺寸会让缩略图计算除零得 inf、按 u32::MAX 分配缓冲直接 abort
    if width == 0 || height == 0 {
        return Err("图片尺寸无效".into());
    }
    let dir = ensure_dir(app);
    let img = image::RgbaImage::from_raw(width, height, rgba.to_vec())
        .ok_or("图片数据尺寸与宽高不匹配")?;
    let file = format!("{id}.png");
    img.save(dir.join(&file)).map_err(|e| format!("保存图片失败: {e}"))?;

    let tw = 160u32;
    let th = (((height as f64) * (tw as f64) / (width as f64)).round() as u32).max(1);
    let thumb = image::imageops::resize(&img, tw, th, image::imageops::FilterType::Triangle);
    thumb
        .save(dir.join(format!("{id}_t.png")))
        .map_err(|e| format!("保存缩略图失败: {e}"))?;
    Ok(file)
}

pub fn read_png(app: &tauri::AppHandle<Wry>, file: &str) -> Result<Vec<u8>, String> {
    // 文件名由程序生成（uuid.png），防御性校验防止路径穿越
    if file.contains("..") || file.contains('/') || file.contains('\\') {
        return Err("非法文件名".into());
    }
    std::fs::read(dir(app).join(file)).map_err(|e| format!("读取图片失败: {e}"))
}

pub fn thumb_name(file: &str) -> String {
    file.strip_suffix(".png")
        .map(|base| format!("{base}_t.png"))
        .unwrap_or_else(|| file.to_string())
}

pub fn png_base64(bytes: &[u8]) -> String {
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

pub fn delete_files(app: &tauri::AppHandle, id: &str) {
    let d = dir(app);
    let _ = std::fs::remove_file(d.join(format!("{id}.png")));
    let _ = std::fs::remove_file(d.join(format!("{id}_t.png")));
}

/// 判断文件名是否为「无引用孤儿」：仅认 `<id>.png` / `<id>_t.png` 命名
/// （id 限 uuid 字符集），其余文件一律不动；引用集合存主图文件名
/// （`<id>.png`，与 ImageRef.file 同形），主图有引用则连同缩略图一起保留
fn orphan_id(name: &str, referenced: &HashSet<String>) -> Option<String> {
    let base = name.strip_suffix(".png")?;
    let id = base.strip_suffix("_t").unwrap_or(base);
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        return None;
    }
    (!referenced.contains(&format!("{id}.png"))).then(|| id.to_string())
}

/// 启动对账：删除 images/ 下没有被任何剪贴板图片条目引用的孤儿文件。
/// 崩溃残留（图已写盘、条目未入库）、数据损坏隔离、.bak 手动恢复都会产生
/// 这类文件，程序此前只漏不补地占用磁盘（评审 rc-M4）。在用图片有引用
/// 集合兜底，不受影响；返回删除的文件数
pub fn gc_orphans_in(dir: &Path, referenced: &HashSet<String>) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else { continue };
        if orphan_id(name, referenced).is_none() {
            continue;
        }
        if std::fs::remove_file(entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(dir: &Path, name: &str) {
        std::fs::write(dir.join(name), b"x").unwrap();
    }

    fn uuid(n: u8) -> String {
        // 生成合法形 uuid（仅 hex 字符 + 连字符），满足命名白名单
        let w = n as u64;
        format!("{:08x}-{:04x}-{:04x}-{:04x}-{:012x}", w, w, w, w, w)
    }

    #[test]
    fn gc_removes_only_unreferenced_png_files() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b, c, d) = (uuid(1), uuid(2), uuid(3), uuid(4));
        for name in [
            format!("{a}.png"),
            format!("{a}_t.png"),
            format!("{b}.png"),
            format!("{c}.png"),
            format!("{c}_t.png"),
            format!("{d}_t.png"),
            "notes.txt".into(),
        ] {
            touch(dir.path(), &name);
        }
        // 引用集合与生产同形：lib.rs 收集的是 ImageRef.file，即「<id>.png」文件名
        let referenced: HashSet<String> = [format!("{a}.png"), format!("{b}.png")].into();

        let removed = gc_orphans_in(dir.path(), &referenced);

        assert_eq!(removed, 3, "应删除 c.png、c_t.png、d_t.png");
        assert!(dir.path().join(format!("{a}.png")).exists());
        assert!(dir.path().join(format!("{a}_t.png")).exists());
        assert!(dir.path().join(format!("{b}.png")).exists());
        assert!(!dir.path().join(format!("{c}.png")).exists());
        assert!(!dir.path().join(format!("{c}_t.png")).exists());
        assert!(!dir.path().join(format!("{d}_t.png")).exists());
        assert!(dir.path().join("notes.txt").exists(), "非图片文件一律不动");
    }

    #[test]
    fn gc_keeps_everything_when_all_referenced() {
        let dir = tempfile::tempdir().unwrap();
        let a = uuid(5);
        touch(dir.path(), &format!("{a}.png"));
        touch(dir.path(), &format!("{a}_t.png"));
        let referenced: HashSet<String> = [format!("{a}.png")].into();
        assert_eq!(gc_orphans_in(dir.path(), &referenced), 0);
    }

    #[test]
    // 回归边界（真机裂图根因）：引用集合是「<id>.png」文件名形状时，
    // 在用图片的主图与缩略图都必须在启动对账后存活
    fn gc_keeps_referenced_pair_with_production_file_shape() {
        let dir = tempfile::tempdir().unwrap();
        let a = uuid(6);
        touch(dir.path(), &format!("{a}.png"));
        touch(dir.path(), &format!("{a}_t.png"));
        let referenced: HashSet<String> = [format!("{a}.png")].into();
        assert_eq!(gc_orphans_in(dir.path(), &referenced), 0);
        assert!(dir.path().join(format!("{a}.png")).exists());
        assert!(dir.path().join(format!("{a}_t.png")).exists());
    }

    #[test]
    fn gc_removes_pair_when_nothing_referenced() {
        let dir = tempfile::tempdir().unwrap();
        let a = uuid(7);
        touch(dir.path(), &format!("{a}.png"));
        touch(dir.path(), &format!("{a}_t.png"));
        assert_eq!(gc_orphans_in(dir.path(), &HashSet::new()), 2);
    }

    #[test]
    fn orphan_id_matches_reference_by_main_image_file_name() {
        let id = uuid(8);
        let referenced: HashSet<String> = [format!("{id}.png")].into();
        assert!(
            orphan_id(&format!("{id}.png"), &referenced).is_none(),
            "主图文件名在引用集合中则不删"
        );
        assert!(
            orphan_id(&format!("{id}_t.png"), &referenced).is_none(),
            "缩略图随主图保留"
        );
        assert_eq!(
            orphan_id(&format!("{id}.png"), &HashSet::new()).as_deref(),
            Some(id.as_str()),
            "引用集合不含其主图时判为孤儿"
        );
    }

    #[test]
    fn gc_tolerates_missing_dir() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(gc_orphans_in(&dir.path().join("nonexistent"), &HashSet::new()), 0);
    }

    #[test]
    fn orphan_id_rejects_non_uuid_names() {
        let referenced: HashSet<String> = HashSet::new();
        assert!(orphan_id("notes.png", &referenced).is_none());
        assert!(orphan_id("readme_t.png", &referenced).is_none(), "非 uuid 字符不删");
        assert!(orphan_id(".png", &referenced).is_none());
        assert_eq!(
            orphan_id("a1b2c3d4-0000-1111-2222-333344445555.png", &referenced).as_deref(),
            Some("a1b2c3d4-0000-1111-2222-333344445555")
        );
    }
}

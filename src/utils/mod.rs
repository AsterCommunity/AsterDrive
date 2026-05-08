//! 工具模块导出。

pub mod hash;
pub mod id;
pub mod numbers;
pub mod paths;

use crate::errors::{AsterError, Result};
use unicode_normalization::UnicodeNormalization;

/// 校验资源归属权，不匹配则返回 403
pub fn verify_owner(entity_user_id: i64, user_id: i64, entity_name: &str) -> Result<()> {
    if entity_user_id != user_id {
        return Err(AsterError::auth_forbidden(format!(
            "not your {entity_name}"
        )));
    }
    Ok(())
}

/// 校验可为空的 owner 字段；团队空间对象通常没有 personal owner。
pub fn verify_optional_owner(
    entity_user_id: Option<i64>,
    user_id: i64,
    entity_name: &str,
) -> Result<()> {
    verify_owner(
        entity_user_id.ok_or_else(|| {
            AsterError::auth_forbidden(format!("{entity_name} has no personal owner"))
        })?,
        user_id,
        entity_name,
    )
}

/// 清理临时文件/目录，失败时记录 warn 日志而不是静默忽略
pub async fn cleanup_temp_file(path: &str) {
    if let Err(e) = tokio::fs::remove_file(path).await
        && e.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!("failed to cleanup temp file {path}: {e}");
    }
}

pub async fn cleanup_temp_dir(path: &str) {
    // macOS Spotlight/Finder 可能在删除过程中往目录里塞 .DS_Store 等文件，
    // 导致 remove_dir_all 的最终 rmdir 返回 ENOTEMPTY，重试即可。
    for _ in 0..3 {
        match tokio::fs::remove_dir_all(path).await {
            Ok(()) => return,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
            Err(e) if e.kind() == std::io::ErrorKind::DirectoryNotEmpty => {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
            Err(e) => {
                tracing::warn!("failed to cleanup temp dir {path}: {e}");
                return;
            }
        }
    }
    if let Err(e) = tokio::fs::remove_dir_all(path).await
        && e.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!("failed to cleanup temp dir {path}: {e}");
    }
}

/// 启动时只清理短命 runtime 临时目录，不碰任务产物和其他 temp 内容。
pub async fn cleanup_runtime_temp_root(temp_root: &str) {
    cleanup_temp_dir(&paths::runtime_temp_dir(temp_root)).await;
}

/// 文件名最大长度
const MAX_FILENAME_LEN: usize = 255;

/// 文件/文件夹名禁止字符
const FORBIDDEN_CHARS: &[char] = &['/', '\\', '\0', ':', '*', '?', '"', '<', '>', '|'];

/// Windows 保留设备名（大小写不敏感，带扩展名也无效）
const WINDOWS_RESERVED_BASENAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

pub fn normalize_name(name: &str) -> String {
    name.nfc().collect()
}

pub fn normalize_validate_name(name: &str) -> Result<String> {
    let normalized = normalize_name(name);
    validate_normalized_name(&normalized)?;
    Ok(normalized)
}

/// 校验文件/文件夹名合法性
pub fn validate_name(name: &str) -> Result<()> {
    let normalized = normalize_name(name);
    validate_normalized_name(&normalized)
}

fn validate_normalized_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(AsterError::validation_error("name cannot be empty"));
    }
    if name.len() > MAX_FILENAME_LEN {
        return Err(AsterError::validation_error(format!(
            "name too long (max {MAX_FILENAME_LEN} chars)"
        )));
    }
    if name == "." || name == ".." {
        return Err(AsterError::validation_error("invalid name"));
    }
    if is_windows_reserved_name(name) {
        return Err(AsterError::validation_error(
            "name cannot use a Windows reserved device name",
        ));
    }
    if let Some(c) = name.chars().find(|c| FORBIDDEN_CHARS.contains(c)) {
        return Err(AsterError::validation_error(format!(
            "name contains forbidden character '{c}'"
        )));
    }
    if name.chars().any(|c| c.is_ascii_control()) {
        return Err(AsterError::validation_error(
            "name contains control characters",
        ));
    }
    if name != name.trim() || name.ends_with('.') {
        return Err(AsterError::validation_error(
            "name cannot start/end with spaces or end with a dot",
        ));
    }
    Ok(())
}

fn is_windows_reserved_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name);
    let upper = stem.to_ascii_uppercase();
    WINDOWS_RESERVED_BASENAMES.contains(&upper.as_str())
}

/// 根据 blob key 计算分片存储路径：`ab/cd/abcdef...`
pub fn storage_path_from_blob_key(blob_key: &str) -> String {
    format!("{}/{}/{}", &blob_key[..2], &blob_key[2..4], blob_key)
}

/// macOS / Office 生成的隐藏文件名，不在目录列表中显示
pub fn is_hidden_name(name: &str) -> bool {
    name.starts_with("._")
        || name.starts_with("~$")
        || name == ".DS_Store"
        || name == ".Spotlight-V100"
        || name == ".Trashes"
}

/// 生成副本名称（macOS/Windows 风格）
///
/// 规则：
/// - `test.txt` → `test (1).txt`
/// - `test (1).txt` → `test (2).txt`
/// - `test (99).txt` → `test (100).txt`
/// - `folder` → `folder (1)` （无扩展名）
/// - `folder (3)` → `folder (4)`
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CopyNameTemplate {
    pub base_name: String,
    pub ext: Option<String>,
    pub next_copy_number: u32,
}

pub(crate) fn copy_name_template(name: &str) -> CopyNameTemplate {
    let (stem, ext) = match name.rfind('.') {
        Some(dot) if dot > 0 => (&name[..dot], Some(name[dot..].to_string())),
        _ => (name, None),
    };

    let (base_name, next_copy_number) = if let Some(paren_start) = stem.rfind(" (") {
        let after_paren = &stem[paren_start + 2..];
        if let Some(num_str) = after_paren.strip_suffix(')') {
            if let Ok(n) = num_str.parse::<u32>() {
                (stem[..paren_start].to_string(), n + 1)
            } else {
                (stem.to_string(), 1)
            }
        } else {
            (stem.to_string(), 1)
        }
    } else {
        (stem.to_string(), 1)
    };

    CopyNameTemplate {
        base_name,
        ext,
        next_copy_number,
    }
}

pub(crate) fn format_copy_name(template: &CopyNameTemplate, copy_number: u32) -> String {
    match template.ext.as_deref() {
        Some(ext) => format!("{} ({copy_number}){ext}", template.base_name),
        None => format!("{} ({copy_number})", template.base_name),
    }
}

pub fn next_copy_name(name: &str) -> String {
    let template = copy_name_template(name);
    format_copy_name(&template, template.next_copy_number)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_is_hidden_name() {
        assert!(is_hidden_name("._file.txt"));
        assert!(is_hidden_name("~$document.docx"));
        assert!(is_hidden_name(".DS_Store"));
        assert!(is_hidden_name(".Spotlight-V100"));
        assert!(is_hidden_name(".Trashes"));
        assert!(!is_hidden_name("normal.txt"));
    }

    #[test]
    fn test_validate_name() {
        // 有效名称
        assert!(validate_name("hello.txt").is_ok());
        assert!(validate_name(".gitignore").is_ok());
        assert!(validate_name("file (1).txt").is_ok());
        assert!(validate_name("cafe\u{0301}.txt").is_ok());

        // 空名
        assert!(validate_name("").is_err());

        // 禁止字符
        assert!(validate_name("a/b").is_err());
        assert!(validate_name("a\\b").is_err());
        assert!(validate_name("a:b").is_err());
        assert!(validate_name("a*b").is_err());
        assert!(validate_name("a?b").is_err());
        assert!(validate_name("a\"b").is_err());
        assert!(validate_name("a<b").is_err());
        assert!(validate_name("a>b").is_err());
        assert!(validate_name("a|b").is_err());

        // 特殊名称
        assert!(validate_name(".").is_err());
        assert!(validate_name("..").is_err());

        // 控制字符
        assert!(validate_name("a\x01b").is_err());
        assert!(validate_name("a\nb").is_err());
        assert!(validate_name("a\tb").is_err());

        // 首尾空格 / 末尾点号
        assert!(validate_name(" leading").is_err());
        assert!(validate_name("trailing ").is_err());
        assert!(validate_name("ends.").is_err());

        // 超长
        let long_name = "a".repeat(256);
        assert!(validate_name(&long_name).is_err());
        let ok_name = "a".repeat(255);
        assert!(validate_name(&ok_name).is_ok());
    }

    #[test]
    fn test_normalize_validate_name_normalizes_nfd_to_nfc() {
        let normalized = normalize_validate_name("cafe\u{0301}.txt").unwrap();
        assert_eq!(normalized, "caf\u{00e9}.txt");
    }

    #[test]
    fn test_validate_name_rejects_windows_reserved_names() {
        for name in [
            "CON", "con", "PRN.txt", "aux", "NUL.log", "COM1", "com9.txt", "LPT1", "lpt9.prn",
        ] {
            assert!(validate_name(name).is_err(), "{name} should be rejected");
        }

        assert!(validate_name("console.txt").is_ok());
        assert!(validate_name("LPT10.txt").is_ok());
    }

    #[test]
    fn test_next_copy_name() {
        assert_eq!(next_copy_name("test.txt"), "test (1).txt");
        assert_eq!(next_copy_name("test (1).txt"), "test (2).txt");
        assert_eq!(next_copy_name("test (99).txt"), "test (100).txt");
        assert_eq!(next_copy_name("folder"), "folder (1)");
        assert_eq!(next_copy_name("folder (3)"), "folder (4)");
        assert_eq!(next_copy_name("my.file.tar.gz"), "my.file.tar (1).gz");
        assert_eq!(next_copy_name("photo (1).jpg"), "photo (2).jpg");
        assert_eq!(next_copy_name(".hidden"), ".hidden (1)");
    }

    #[test]
    fn test_copy_name_template_parses_existing_suffix() {
        let template = copy_name_template("photo (41).jpg");
        assert_eq!(template.base_name, "photo");
        assert_eq!(template.ext.as_deref(), Some(".jpg"));
        assert_eq!(template.next_copy_number, 42);
        assert_eq!(
            format_copy_name(&template, template.next_copy_number),
            "photo (42).jpg"
        );
    }

    #[test]
    fn test_storage_path_from_blob_key() {
        let hash = "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890";
        assert_eq!(storage_path_from_blob_key(hash), format!("ab/cd/{hash}"));
    }

    #[tokio::test]
    async fn test_cleanup_runtime_temp_root_only_removes_runtime_namespace() {
        let temp_root =
            std::env::temp_dir().join(format!("aster-drive-utils-{}", uuid::Uuid::new_v4()));
        let temp_root = temp_root.to_string_lossy().into_owned();
        let runtime_dir = PathBuf::from(paths::runtime_temp_dir(&temp_root));
        let task_dir = PathBuf::from(paths::task_temp_dir(&temp_root, 42));

        tokio::fs::create_dir_all(&runtime_dir).await.unwrap();
        tokio::fs::create_dir_all(&task_dir).await.unwrap();
        tokio::fs::write(runtime_dir.join("session.tmp"), b"runtime")
            .await
            .unwrap();
        tokio::fs::write(task_dir.join("artifact.bin"), b"task")
            .await
            .unwrap();

        cleanup_runtime_temp_root(&temp_root).await;

        assert!(!runtime_dir.exists());
        assert!(task_dir.exists());
        assert!(task_dir.join("artifact.bin").exists());

        cleanup_temp_dir(&temp_root).await;
    }
}

use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

use aster_drive_storage::{MapStorageErr, Result, StorageErrorKind, storage_driver_error};

use super::DEFAULT_LOCAL_STORAGE_PATH;

pub fn effective_base_path(base_path: &str) -> PathBuf {
    if base_path.is_empty() {
        PathBuf::from(DEFAULT_LOCAL_STORAGE_PATH)
    } else {
        PathBuf::from(base_path)
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match normalized.components().next_back() {
                Some(Component::Normal(_)) => {
                    normalized.pop();
                }
                Some(Component::RootDir) | Some(Component::Prefix(_)) => {}
                _ => normalized.push(component.as_os_str()),
            },
            Component::RootDir | Component::Prefix(_) | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }

    if normalized.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        normalized
    }
}

fn absolute_path(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_storage_err_ctx(
                StorageErrorKind::Misconfigured,
                "resolve local storage current_dir",
            )?
            .join(path)
    };
    Ok(normalize_path(&absolute))
}

fn resolve_existing_path(path: &Path) -> Result<PathBuf> {
    let mut probe = absolute_path(path)?;
    let mut missing_suffix = Vec::<OsString>::new();

    loop {
        match std::fs::symlink_metadata(&probe) {
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let Some(name) = probe.file_name() else {
                    return Err(storage_driver_error(
                        StorageErrorKind::Misconfigured,
                        format!("local path has no existing ancestor: {}", path.display()),
                    ));
                };
                missing_suffix.push(name.to_os_string());
                let Some(parent) = probe.parent() else {
                    return Err(storage_driver_error(
                        StorageErrorKind::Misconfigured,
                        format!("local path has no parent: {}", path.display()),
                    ));
                };
                probe = parent.to_path_buf();
            }
            Err(error) => {
                return Err(storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    format!("inspect local path {}: {error}", probe.display()),
                ));
            }
        }
    }

    let mut resolved = std::fs::canonicalize(&probe)
        .map_storage_err_ctx(StorageErrorKind::Misconfigured, "canonicalize local path")?;
    for segment in missing_suffix.into_iter().rev() {
        resolved.push(segment);
    }
    Ok(resolved)
}

pub(super) fn resolve_path_within_root(
    root: &Path,
    relative: &Path,
    requested_path: &str,
) -> Result<PathBuf> {
    let candidate = root.join(relative);
    let resolved = resolve_existing_path(&candidate)?;
    if resolved.starts_with(root) {
        Ok(resolved)
    } else {
        Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            format!("resolved storage path escapes base path: {requested_path}"),
        ))
    }
}

pub fn resolved_base_path(base_path: &str) -> Result<PathBuf> {
    resolve_existing_path(&effective_base_path(base_path))
}

/// 校验 driver 输入路径，拒绝绝对路径、Windows 盘符前缀以及任何 `..` 段，
/// 防止攻击者通过污染 storage_path 逃出 base_path。
pub(super) fn sanitize_relative_path(path: &str) -> Result<PathBuf> {
    let trimmed = path.trim_start_matches('/');
    let candidate = Path::new(trimmed);
    let mut safe = PathBuf::new();
    for component in candidate.components() {
        match component {
            Component::Normal(segment) => safe.push(segment),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    format!("invalid storage path: {path}"),
                ));
            }
        }
    }
    Ok(safe)
}

pub fn upload_staging_path(base_path: &str, name: &str) -> Result<PathBuf> {
    let root = resolved_base_path(base_path)?;
    let safe = sanitize_relative_path(name).unwrap_or_else(|_| PathBuf::from("_invalid"));
    resolve_path_within_root(&root, &Path::new(".staging").join(safe), name)
}

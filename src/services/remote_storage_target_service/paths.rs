use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use crate::api::api_error_code::ApiErrorCode;
use crate::errors::{AsterError, MapAsterErr, Result, validation_error_with_code};
use crate::storage::field_contract::{
    RelativeLocalPathNormalizationError, normalize_relative_local_target_path,
};

pub(in crate::services::remote_storage_target_service) fn resolve_remote_storage_target_local_path(
    root: &str,
    relative: &str,
) -> Result<PathBuf> {
    let trimmed_root = root.trim();
    if trimmed_root.is_empty() {
        return Err(AsterError::config_error(
            "server.follower.remote_storage_target_local_root cannot be empty",
        ));
    }
    let normalized = normalize_relative_local_path(relative)?;
    let root_path = Path::new(trimmed_root);
    fs::create_dir_all(root_path).map_aster_err_ctx(
        &format!(
            "create server.follower.remote_storage_target_local_root '{}'",
            root_path.display()
        ),
        AsterError::config_error,
    )?;
    let canonical_root = fs::canonicalize(root_path).map_aster_err_ctx(
        &format!(
            "canonicalize server.follower.remote_storage_target_local_root '{}'",
            root_path.display()
        ),
        AsterError::config_error,
    )?;
    let candidate = if normalized == "." {
        root_path.to_path_buf()
    } else {
        root_path.join(normalized)
    };

    let mut existing_ancestor = candidate.clone();
    let mut missing_components = Vec::<OsString>::new();
    loop {
        match fs::metadata(&existing_ancestor) {
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let Some(name) = existing_ancestor.file_name() else {
                    return Err(AsterError::config_error(format!(
                        "remote storage target local path has no existing ancestor: {}",
                        candidate.display()
                    )));
                };
                missing_components.push(name.to_os_string());
                let Some(parent) = existing_ancestor.parent() else {
                    return Err(AsterError::config_error(format!(
                        "remote storage target local path has no parent: {}",
                        candidate.display()
                    )));
                };
                existing_ancestor = parent.to_path_buf();
            }
            Err(error) => {
                return Err(AsterError::config_error(format!(
                    "inspect remote storage target local path '{}': {error}",
                    existing_ancestor.display()
                )));
            }
        }
    }

    let mut resolved = fs::canonicalize(&existing_ancestor).map_aster_err_ctx(
        &format!(
            "canonicalize remote storage target local path '{}'",
            existing_ancestor.display()
        ),
        AsterError::config_error,
    )?;
    for component in missing_components.into_iter().rev() {
        resolved.push(component);
    }

    if resolved.starts_with(&canonical_root) {
        Ok(resolved)
    } else {
        Err(AsterError::config_error(format!(
            "local remote storage target base_path '{}' escapes server.follower.remote_storage_target_local_root '{}'",
            relative,
            root_path.display()
        )))
    }
}

pub(in crate::services::remote_storage_target_service) fn normalize_relative_local_path(
    value: &str,
) -> Result<String> {
    match normalize_relative_local_target_path(value) {
        Ok(normalized) => Ok(normalized),
        Err(RelativeLocalPathNormalizationError::Blank) => Err(AsterError::validation_error(
            "base_path cannot be blank for local remote storage targets",
        )),
        Err(RelativeLocalPathNormalizationError::EscapesRoot) => Err(validation_error_with_code(
            ApiErrorCode::ManagedIngressLocalPathInvalid,
            "local remote storage target base_path must stay within server.follower.remote_storage_target_local_root",
        )),
    }
}

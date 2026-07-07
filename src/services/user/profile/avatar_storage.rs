//! 用户资料服务子模块：`avatar_storage`。

use std::path::Component;
use std::path::{Path, PathBuf};

use crate::config::avatar;
use crate::entities::user_profile;
use crate::runtime::SharedRuntimeState;
use crate::types::AvatarSource;

use super::shared::{AVATAR_SIZE_LG, AVATAR_SIZE_SM, stored_avatar_prefix};

pub(super) fn avatar_variant_file_path(prefix: &Path, size: u32) -> PathBuf {
    prefix.join(format!("{size}.webp"))
}

pub(super) fn user_avatar_prefix(user_id: i64, version: i32) -> String {
    format!("user/{user_id}/v{version}")
}

pub(super) fn user_avatar_dir(root_dir: &Path, user_id: i64, version: i32) -> PathBuf {
    root_dir.join(user_avatar_prefix(user_id, version))
}

fn normalize_absolute_path(path: &Path) -> Option<PathBuf> {
    if !path.is_absolute() {
        return None;
    }

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    return None;
                }
            }
            Component::Normal(part) => normalized.push(part),
        }
    }

    Some(normalized)
}

fn normalized_avatar_root(root_dir: &Path) -> Option<PathBuf> {
    normalize_absolute_path(root_dir)
}

fn expected_user_avatar_prefix_path(user_id: i64, version: i32) -> PathBuf {
    PathBuf::from(user_avatar_prefix(user_id, version))
}

pub(super) fn resolve_stored_avatar_prefix_path(
    root_dir: &Path,
    profile: &user_profile::Model,
) -> Option<PathBuf> {
    let stored_prefix = stored_avatar_prefix(Some(profile))?;
    let expected_relative =
        expected_user_avatar_prefix_path(profile.user_id, profile.avatar_version);
    let normalized_root = normalized_avatar_root(root_dir)?;
    let stored_path = Path::new(stored_prefix);

    if stored_path.is_absolute() || stored_path != expected_relative {
        return None;
    }

    Some(normalized_root.join(expected_relative))
}

pub(super) fn resolve_stored_avatar_variant_path(
    root_dir: &Path,
    profile: &user_profile::Model,
    size: u32,
) -> Option<PathBuf> {
    resolve_stored_avatar_prefix_path(root_dir, profile)
        .map(|prefix| avatar_variant_file_path(&prefix, size))
}

async fn cleanup_empty_avatar_dirs(prefix_dir: &Path, root_dir: &Path) {
    let Some(mut current) = normalize_absolute_path(prefix_dir) else {
        tracing::warn!(
            "skip avatar dir cleanup for non-absolute prefix {}",
            prefix_dir.display()
        );
        return;
    };
    let Some(root_dir) = normalize_absolute_path(root_dir) else {
        tracing::warn!(
            "skip avatar dir cleanup for non-absolute root {}",
            root_dir.display()
        );
        return;
    };

    if current == root_dir || !current.starts_with(&root_dir) {
        tracing::warn!(
            "skip avatar dir cleanup outside avatar root: prefix={}, root={}",
            current.display(),
            root_dir.display()
        );
        return;
    }

    while current != root_dir {
        match tokio::fs::remove_dir(&current).await {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) if e.kind() == std::io::ErrorKind::DirectoryNotEmpty => break,
            Err(e) => {
                tracing::warn!("failed to cleanup avatar dir {}: {e}", current.display());
                break;
            }
        }

        let Some(parent) = current.parent() else {
            break;
        };
        current = parent.to_path_buf();
    }
}

async fn delete_local_avatar_files(prefix: &Path) {
    for size in [AVATAR_SIZE_SM, AVATAR_SIZE_LG] {
        let path = avatar_variant_file_path(prefix, size);
        if let Err(e) = tokio::fs::remove_file(&path).await
            && e.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!("failed to delete avatar file {}: {e}", path.display());
        }
    }
}

pub(super) async fn cleanup_local_avatar_prefix(prefix: &Path, root_dir: &Path) {
    delete_local_avatar_files(prefix).await;
    cleanup_empty_avatar_dirs(prefix, root_dir).await;
}

pub(super) async fn delete_upload_objects(
    state: &impl SharedRuntimeState,
    profile: &user_profile::Model,
) {
    if profile.avatar_source != AvatarSource::Upload {
        return;
    }

    if stored_avatar_prefix(Some(profile)).is_none() {
        return;
    }

    match avatar::resolve_local_avatar_root_dir(state.runtime_config()) {
        Ok(root_dir) => {
            let Some(prefix_path) = resolve_stored_avatar_prefix_path(&root_dir, profile) else {
                tracing::warn!(
                    user_id = profile.user_id,
                    avatar_version = profile.avatar_version,
                    "skip avatar cleanup for invalid stored avatar key"
                );
                return;
            };

            delete_local_avatar_files(&prefix_path).await;
            cleanup_empty_avatar_dirs(&prefix_path, &root_dir).await;
        }
        Err(e) => {
            tracing::warn!(
                user_id = profile.user_id,
                avatar_version = profile.avatar_version,
                "failed to resolve avatar root for local avatar cleanup: {e}"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;

    fn profile_with_key(user_id: i64, version: i32, avatar_key: &str) -> user_profile::Model {
        user_profile::Model {
            user_id,
            display_name: None,
            wopi_user_info: None,
            avatar_source: AvatarSource::Upload,
            avatar_key: Some(avatar_key.to_string()),
            avatar_version: version,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn avatar_test_root() -> PathBuf {
        std::env::temp_dir().join("asterdrive-avatar")
    }

    #[test]
    fn resolve_stored_avatar_prefix_accepts_expected_relative_key() {
        let root = avatar_test_root();
        let profile = profile_with_key(42, 3, "user/42/v3");

        assert_eq!(
            resolve_stored_avatar_prefix_path(&root, &profile),
            Some(root.join("user/42/v3"))
        );
    }

    #[test]
    fn resolve_stored_avatar_prefix_rejects_absolute_key_under_root() {
        let root = avatar_test_root();
        let avatar_prefix = root.join("user/42/v3");
        let profile = profile_with_key(42, 3, &avatar_prefix.to_string_lossy());

        assert!(resolve_stored_avatar_prefix_path(&root, &profile).is_none());
    }

    #[test]
    fn resolve_stored_avatar_prefix_rejects_key_outside_root() {
        let root = avatar_test_root();
        let outside_prefix = std::env::temp_dir().join("other-avatar/user/42/v3");
        let profile = profile_with_key(42, 3, &outside_prefix.to_string_lossy());

        assert!(resolve_stored_avatar_prefix_path(&root, &profile).is_none());
    }

    #[test]
    fn resolve_stored_avatar_prefix_rejects_wrong_user_or_version() {
        let root = avatar_test_root();
        let wrong_user = profile_with_key(42, 3, "user/43/v3");
        let wrong_version = profile_with_key(42, 3, "user/42/v4");

        assert!(resolve_stored_avatar_prefix_path(&root, &wrong_user).is_none());
        assert!(resolve_stored_avatar_prefix_path(&root, &wrong_version).is_none());
    }
}

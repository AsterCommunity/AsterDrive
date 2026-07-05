//! Shared storage descriptor field semantics and pure normalization helpers.
//!
//! This module intentionally does not define a universal product descriptor.
//! Storage policy connectors and managed ingress targets keep their own DTOs,
//! but common field meanings and normalization rules live here so the two
//! surfaces do not drift.

use std::path::{Component, Path, PathBuf};

use crate::errors::{AsterError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageDescriptorFieldKind {
    Text,
    Secret,
    Select,
    Boolean,
    Number,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageSecretEditSemantics {
    /// A secret is required while creating the resource, but an omitted value
    /// while editing preserves the currently stored secret.
    RequiredOnCreatePreserveWhenOmittedOnEdit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StorageDescriptorFieldSemantics {
    pub kind: StorageDescriptorFieldKind,
    pub required: bool,
    pub secret: bool,
    pub secret_edit_semantics: Option<StorageSecretEditSemantics>,
}

impl StorageDescriptorFieldSemantics {
    pub const fn text(required: bool) -> Self {
        Self {
            kind: StorageDescriptorFieldKind::Text,
            required,
            secret: false,
            secret_edit_semantics: None,
        }
    }

    pub const fn secret(required: bool) -> Self {
        Self {
            kind: StorageDescriptorFieldKind::Secret,
            required,
            secret: true,
            secret_edit_semantics: Some(
                StorageSecretEditSemantics::RequiredOnCreatePreserveWhenOmittedOnEdit,
            ),
        }
    }

    pub const fn boolean(required: bool) -> Self {
        Self {
            kind: StorageDescriptorFieldKind::Boolean,
            required,
            secret: false,
            secret_edit_semantics: None,
        }
    }

    pub const fn from_descriptor_bits(
        kind: StorageDescriptorFieldKind,
        required: bool,
        secret: bool,
    ) -> Self {
        let secret_edit_semantics = if secret {
            Some(StorageSecretEditSemantics::RequiredOnCreatePreserveWhenOmittedOnEdit)
        } else {
            None
        };
        Self {
            kind,
            required,
            secret,
            secret_edit_semantics,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelativeLocalPathNormalizationError {
    Blank,
    EscapesRoot,
}

pub fn normalize_required_storage_field(field: &str, value: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AsterError::validation_error(format!(
            "{field} cannot be blank"
        )));
    }
    Ok(trimmed.to_string())
}

pub fn preserve_secret_when_omitted(
    field: &str,
    existing: &str,
    replacement: Option<String>,
) -> Result<String> {
    match replacement {
        Some(value) => normalize_required_storage_field(field, &value),
        None => Ok(existing.to_string()),
    }
}

pub fn normalize_object_storage_prefix(value: &str) -> String {
    value.trim().trim_matches('/').to_string()
}

pub fn normalize_storage_policy_max_file_size(value: i64) -> Result<i64> {
    if value < 0 {
        return Err(AsterError::validation_error(
            "max_file_size must be non-negative",
        ));
    }
    Ok(value)
}

pub fn normalize_relative_local_target_path(
    value: &str,
) -> std::result::Result<String, RelativeLocalPathNormalizationError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(RelativeLocalPathNormalizationError::Blank);
    }

    let safe_value = trimmed.replace('\\', "/");
    let candidate = Path::new(&safe_value);
    let mut normalized = PathBuf::new();
    for component in candidate.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(segment) => normalized.push(segment),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(RelativeLocalPathNormalizationError::EscapesRoot);
            }
        }
    }

    if normalized.as_os_str().is_empty() {
        Ok(".".to_string())
    } else {
        Ok(normalized.to_string_lossy().replace('\\', "/"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_field_semantics_marks_secret_edit_contract() {
        let secret = StorageDescriptorFieldSemantics::secret(true);

        assert_eq!(secret.kind, StorageDescriptorFieldKind::Secret);
        assert!(secret.required);
        assert!(secret.secret);
        assert_eq!(
            secret.secret_edit_semantics,
            Some(StorageSecretEditSemantics::RequiredOnCreatePreserveWhenOmittedOnEdit)
        );

        let text = StorageDescriptorFieldSemantics::text(false);
        assert_eq!(text.kind, StorageDescriptorFieldKind::Text);
        assert!(!text.required);
        assert!(!text.secret);
        assert_eq!(text.secret_edit_semantics, None);
    }

    #[test]
    fn required_storage_field_trims_and_rejects_blank_values() {
        assert_eq!(
            normalize_required_storage_field("secret_key", " value ").unwrap(),
            "value"
        );

        let error = normalize_required_storage_field("secret_key", " \t ").unwrap_err();
        assert!(error.message().contains("secret_key cannot be blank"));
    }

    #[test]
    fn omitted_secret_preserves_existing_value_and_replacement_is_normalized() {
        assert_eq!(
            preserve_secret_when_omitted("secret_key", "stored", None).unwrap(),
            "stored"
        );
        assert_eq!(
            preserve_secret_when_omitted("secret_key", "stored", Some(" next ".to_string()))
                .unwrap(),
            "next"
        );
        assert!(
            preserve_secret_when_omitted("secret_key", "stored", Some(" ".to_string())).is_err()
        );
    }

    #[test]
    fn object_storage_prefix_trims_outer_slashes_only() {
        assert_eq!(
            normalize_object_storage_prefix(" /tenant/archive/ "),
            "tenant/archive"
        );
        assert_eq!(normalize_object_storage_prefix(""), "");
        assert_eq!(normalize_object_storage_prefix("///"), "");
    }

    #[test]
    fn max_file_size_zero_means_unlimited_and_negative_is_invalid() {
        assert_eq!(normalize_storage_policy_max_file_size(0).unwrap(), 0);
        assert_eq!(normalize_storage_policy_max_file_size(42).unwrap(), 42);
        assert!(normalize_storage_policy_max_file_size(-1).is_err());
    }

    #[test]
    fn relative_local_target_path_normalizes_safe_segments() {
        assert_eq!(
            normalize_relative_local_target_path(" ./archive/2026 ").unwrap(),
            "archive/2026"
        );
        assert_eq!(normalize_relative_local_target_path("././").unwrap(), ".");
    }

    #[test]
    fn relative_local_target_path_rejects_blank_and_escape_segments() {
        assert_eq!(
            normalize_relative_local_target_path(" ").unwrap_err(),
            RelativeLocalPathNormalizationError::Blank
        );
        assert_eq!(
            normalize_relative_local_target_path("../secret").unwrap_err(),
            RelativeLocalPathNormalizationError::EscapesRoot
        );
        assert_eq!(
            normalize_relative_local_target_path("..\\secret").unwrap_err(),
            RelativeLocalPathNormalizationError::EscapesRoot
        );
    }
}

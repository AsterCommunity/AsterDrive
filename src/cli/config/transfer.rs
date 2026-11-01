//! Versioned portable values, deliberately independent of presentation DTOs.
use std::collections::BTreeSet;
use std::path::Path;

use aster_forge_config::{ConfigSource, ConfigValue, ConfigVisibility};
use aster_forge_db::system_config;
use serde::{Deserialize, Serialize};

use crate::config::definitions::CONFIG_REGISTRY;
use crate::errors::{AsterError, Result};

pub const BACKUP_FORMAT: &str = "asterdrive.runtime-config";
pub const BACKUP_VERSION: u32 = 1;
const MAX_BACKUP_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ImportItem {
    pub key: String,
    pub value: ConfigValue,
    pub source: ConfigSource,
    pub visibility: ConfigVisibility,
    pub is_sensitive: bool,
}

impl std::fmt::Debug for ImportItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImportItem")
            .field("key", &self.key)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigBackup {
    pub format: String,
    pub version: u32,
    pub configs: Vec<ImportItem>,
    pub omitted_sensitive_keys: Vec<String>,
}

impl ConfigBackup {
    pub fn from_models(models: Vec<system_config::Model>, include_sensitive: bool) -> Result<Self> {
        let mut backup = Self {
            format: BACKUP_FORMAT.into(),
            version: BACKUP_VERSION,
            configs: Vec::new(),
            omitted_sensitive_keys: Vec::new(),
        };
        for model in models {
            let model = crate::config::system_config::apply_definition(model);
            if model.is_sensitive && !include_sensitive {
                backup.omitted_sensitive_keys.push(model.key);
                continue;
            }
            let value = ConfigValue::from_storage(model.value_type, model.value).map_err(|_| {
                AsterError::validation_error(
                    "stored configuration has an invalid value; export aborted",
                )
            })?;
            let item = ImportItem {
                key: model.key,
                value,
                source: model.source,
                visibility: model.visibility,
                is_sensitive: model.is_sensitive,
            };
            validate_item(&item)?;
            backup.configs.push(item);
        }
        backup.configs.sort_by(|a, b| a.key.cmp(&b.key));
        backup.omitted_sensitive_keys.sort();
        // Include the final stdout newline: never produce a backup larger
        // than the reader accepts, even when JSON escaping expands a value.
        let encoded = serde_json::to_vec_pretty(&backup)
            .map_err(|_| AsterError::config_error("cannot encode configuration backup"))?;
        if encoded.len() as u64 >= MAX_BACKUP_BYTES {
            return Err(AsterError::validation_error(
                "configuration backup exceeds 16 MiB limit; export aborted",
            ));
        }
        Ok(backup)
    }
}

pub fn read_import_items(path: &Path) -> Result<Vec<ImportItem>> {
    let file = std::fs::File::open(path)
        .map_err(|_| AsterError::config_error("cannot open configuration backup"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = file
            .metadata()
            .map_err(|_| AsterError::config_error("cannot inspect configuration backup"))?;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(AsterError::validation_error(
                "configuration backup must be private (chmod 600)",
            ));
        }
    }
    use std::io::Read;
    let mut bytes = Vec::new();
    file.take(MAX_BACKUP_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| AsterError::config_error("cannot read configuration backup"))?;
    if bytes.len() as u64 > MAX_BACKUP_BYTES {
        return Err(AsterError::validation_error(
            "configuration backup exceeds 16 MiB limit",
        ));
    }
    parse_backup(&bytes)
}

fn parse_backup(bytes: &[u8]) -> Result<Vec<ImportItem>> {
    let backup: ConfigBackup = serde_json::from_slice(bytes).map_err(|error| {
        AsterError::validation_error(format!(
            "invalid configuration backup at line {}, column {} (values redacted); expected versioned {} format",
            error.line(), error.column(), BACKUP_FORMAT))
    })?;
    if backup.format != BACKUP_FORMAT || backup.version != BACKUP_VERSION {
        return Err(AsterError::validation_error(
            "unsupported configuration backup format or version",
        ));
    }
    let mut keys = BTreeSet::new();
    for item in &backup.configs {
        validate_item(item)?;
        if !keys.insert(&item.key) {
            return Err(AsterError::validation_error(
                "duplicate configuration key in backup",
            ));
        }
    }
    for key in &backup.omitted_sensitive_keys {
        if key.is_empty() || key.len() > 255 || key.chars().any(char::is_control) {
            return Err(AsterError::validation_error(
                "invalid omitted configuration key",
            ));
        }
        if !keys.insert(key) {
            return Err(AsterError::validation_error(
                "duplicate or overlapping omitted configuration key",
            ));
        }
    }
    Ok(backup.configs)
}

pub fn validate_item(item: &ImportItem) -> Result<()> {
    if item.key.is_empty() || item.key.len() > 255 || item.key.chars().any(char::is_control) {
        return Err(AsterError::validation_error(
            "configuration key must contain 1..255 bytes without control characters",
        ));
    }
    if item.is_sensitive && item.visibility != ConfigVisibility::Private {
        return Err(AsterError::validation_error(
            "sensitive configuration must remain private",
        ));
    }
    let definition = CONFIG_REGISTRY.get(&item.key);
    match (item.source, definition) {
        (ConfigSource::System, None) => {
            return Err(AsterError::validation_error(
                "unknown system configuration key",
            ));
        }
        (ConfigSource::Custom, Some(_)) => {
            return Err(AsterError::validation_error(
                "custom configuration cannot overwrite a system key",
            ));
        }
        _ => {}
    }
    if let Some(definition) = definition
        && (item.visibility != definition.visibility
            || item.is_sensitive != definition.is_sensitive)
    {
        return Err(AsterError::validation_error(
            "system configuration metadata is read-only",
        ));
    }
    if item.is_sensitive && item.value == ConfigValue::redacted() {
        return Err(AsterError::validation_error(
            "redacted placeholders cannot be imported as secrets",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn valid_backup() -> Value {
        let def = CONFIG_REGISTRY.get("mail_smtp_password").unwrap();
        json!({ "format": BACKUP_FORMAT, "version": BACKUP_VERSION,
            "configs": [{"key":def.key,"value":"credential-marker",
                "source":"system","visibility":def.visibility,"is_sensitive":true}],
            "omitted_sensitive_keys":[] })
    }

    #[test]
    fn rejects_versions_unknown_system_keys_readonly_metadata_duplicates_and_placeholders() {
        let original = valid_backup();
        assert!(parse_backup(&serde_json::to_vec(&original).unwrap()).is_ok());
        let mut invalid = Vec::new();
        let mut value = original.clone();
        value["version"] = json!(99);
        invalid.push(value);
        let mut value = original.clone();
        value["format"] = json!("other");
        invalid.push(value);
        let mut value = original.clone();
        value["configs"][0]["key"] = json!("unknown.system");
        invalid.push(value);
        let mut value = original.clone();
        value["configs"][0]["source"] = json!("custom");
        invalid.push(value);
        let mut value = original.clone();
        value["configs"][0]["is_sensitive"] = json!(false);
        invalid.push(value);
        let mut value = original.clone();
        value["configs"][0]["visibility"] = json!("public");
        invalid.push(value);
        let mut value = original.clone();
        value["configs"][0]["value"] = json!(ConfigValue::REDACTED);
        invalid.push(value);
        let mut value = original.clone();
        value["configs"][0]["updated_at"] = json!("read-only");
        invalid.push(value);
        let mut value = original.clone();
        value["configs"]
            .as_array_mut()
            .unwrap()
            .push(original["configs"][0].clone());
        invalid.push(value);
        let mut value = original.clone();
        value["omitted_sensitive_keys"] = json!(["mail_smtp_password"]);
        invalid.push(value);
        let mut value = original.clone();
        value["omitted_sensitive_keys"] = json!(["custom.omitted", "custom.omitted"]);
        invalid.push(value);
        invalid.push(original["configs"].clone());
        invalid.push(json!({"configs": original["configs"]}));
        for input in invalid {
            let error = parse_backup(&serde_json::to_vec(&input).unwrap()).unwrap_err();
            assert!(!format!("{error:?}").contains("credential-marker"));
        }
        let invalid = br#"{"format":"credential-marker","version":"credential-marker"}"#;
        assert!(
            !parse_backup(invalid)
                .unwrap_err()
                .message()
                .contains("credential-marker")
        );
        assert!(
            !format!(
                "{:?}",
                parse_backup(&serde_json::to_vec(&original).unwrap()).unwrap()
            )
            .contains("credential-marker")
        );
    }

    #[test]
    fn accepts_explicit_custom_keys_and_preserves_values_visibility_and_sensitivity() {
        let value = json!({"format":BACKUP_FORMAT,"version":BACKUP_VERSION,
            "configs":[{"key":"custom.data","value":"  first\nsecond  ","source":"custom","visibility":"authenticated","is_sensitive":false},
                {"key":"custom.secret","value":"credential-marker","source":"custom","visibility":"private","is_sensitive":true}],
            "omitted_sensitive_keys":["custom.omitted"]});
        let entries = parse_backup(&serde_json::to_vec(&value).unwrap()).unwrap();
        assert_eq!(
            entries[0].value,
            ConfigValue::String("  first\nsecond  ".into())
        );
        assert_eq!(entries[0].visibility, ConfigVisibility::Authenticated);
        assert!(entries[1].is_sensitive);
    }

    #[test]
    fn export_refuses_oversized_encoded_values_instead_of_producing_an_unreadable_backup() {
        let model = system_config::Model {
            id: 1,
            key: "custom.large".into(),
            value: "\n".repeat(usize::try_from(MAX_BACKUP_BYTES / 2).unwrap()),
            value_type: aster_forge_config::ConfigValueType::String,
            requires_restart: false,
            is_sensitive: false,
            source: ConfigSource::Custom,
            visibility: ConfigVisibility::Private,
            namespace: String::new(),
            category: String::new(),
            description: String::new(),
            updated_at: chrono::Utc::now(),
            updated_by: None,
        };
        let error = ConfigBackup::from_models(vec![model], true).unwrap_err();
        assert!(error.message().contains("16 MiB"));
    }
}

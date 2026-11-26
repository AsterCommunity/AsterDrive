//! Connector-owned localization resources.
//!
//! Descriptors carry stable message ids. Connector implementations provide
//! the corresponding localized text, while AsterDrive exposes it through an
//! authenticated resource endpoint. No connector may inject executable UI
//! code or choose an arbitrary resource URL.

use std::collections::{BTreeMap, BTreeSet};

use aster_drive_model::types::LocaleTag;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::ConnectorId;

const MAX_LOCALES: usize = 32;
const MAX_MESSAGES: usize = 512;
const MAX_MESSAGE_ID_LENGTH: usize = 128;
const MAX_MESSAGE_LENGTH: usize = 4_096;
const MAX_REVISION_LENGTH: usize = 128;

/// One connector-owned message and all translations shipped by that plugin.
///
/// The static declaration format is intentionally part of the storage crate's
/// public contract so built-in connectors and separately packaged connectors
/// construct localization resources through the same validated path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StorageConnectorLocalizationMessage<'a> {
    /// Stable descriptor message id; validated against the id charset and
    /// length limits by [`StorageConnectorLocalization::from_messages`].
    pub message_id: &'a str,
    /// All locale variants shipped for this message; each locale may appear
    /// at most once.
    pub translations: &'a [StorageConnectorLocalizationTranslation<'a>],
}

impl<'a> StorageConnectorLocalizationMessage<'a> {
    /// Builds a message declaration from static connector-owned slices.
    pub const fn new(
        message_id: &'a str,
        translations: &'a [StorageConnectorLocalizationTranslation<'a>],
    ) -> Self {
        Self {
            message_id,
            translations,
        }
    }
}

/// One locale variant in a connector-owned Rust message declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StorageConnectorLocalizationTranslation<'a> {
    /// BCP-47-style locale tag; parsed and validated as a [`LocaleTag`].
    pub locale: &'a str,
    /// Localized text; may contain `{{placeholder}}` interpolations that must
    /// match the default locale's placeholders.
    pub value: &'a str,
}

impl<'a> StorageConnectorLocalizationTranslation<'a> {
    /// Builds one locale variant from static connector-owned slices.
    pub const fn new(locale: &'a str, value: &'a str) -> Self {
        Self { locale, value }
    }
}

/// Declare connector localization in Rust without manually assembling maps or
/// JSON. The locale-list form stays open so adding a product locale never
/// changes the plugin contract or forces unrelated plugins to add it.
#[macro_export]
macro_rules! storage_connector_message {
    ($message_id:literal, $en:literal, $zh:literal $(,)?) => {
        $crate::StorageConnectorLocalizationMessage::new(
            $message_id,
            &[
                $crate::StorageConnectorLocalizationTranslation::new("en", $en),
                $crate::StorageConnectorLocalizationTranslation::new("zh", $zh),
            ],
        )
    };
    ($message_id:literal; $($locale:literal => $value:literal),+ $(,)?) => {
        $crate::StorageConnectorLocalizationMessage::new(
            $message_id,
            &[
                $($crate::StorageConnectorLocalizationTranslation::new($locale, $value)),+
            ],
        )
    };
}

/// Validated localization resource owned by one connector.
///
/// Construction always runs [`Self::validate`], so an instance guarantees:
/// non-empty revision, 1..=`MAX_LOCALES` locales, every locale carrying the
/// same message id set as the default locale, matching interpolation
/// placeholders across locales, and per-message id/text limits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageConnectorLocalization {
    connector_id: ConnectorId,
    default_locale: LocaleTag,
    revision: String,
    resources: BTreeMap<LocaleTag, BTreeMap<String, String>>,
}

impl StorageConnectorLocalization {
    /// Build a validated resource from connector-owned Rust declarations.
    ///
    /// The revision is derived from normalized locale/message ordering, making
    /// it deterministic even when a plugin composes multiple message slices.
    pub fn from_messages<'a>(
        connector_id: ConnectorId,
        default_locale: &str,
        messages: impl IntoIterator<Item = &'a StorageConnectorLocalizationMessage<'a>>,
    ) -> Result<Self, StorageConnectorLocalizationError> {
        let default_locale = LocaleTag::parse(default_locale)
            .map_err(|error| StorageConnectorLocalizationError(error.to_string()))?;
        let mut message_ids = BTreeSet::new();
        let mut resources = BTreeMap::<LocaleTag, BTreeMap<String, String>>::new();

        for message in messages {
            if !message_ids.insert(message.message_id) {
                return Err(StorageConnectorLocalizationError(format!(
                    "connector '{}' declares localization message '{}' more than once",
                    connector_id, message.message_id
                )));
            }
            let mut translated_locales = BTreeSet::new();
            for translation in message.translations {
                let locale = LocaleTag::parse(translation.locale)
                    .map_err(|error| StorageConnectorLocalizationError(error.to_string()))?;
                if !translated_locales.insert(locale.clone()) {
                    return Err(StorageConnectorLocalizationError(format!(
                        "connector '{}' message '{}' declares locale '{}' more than once",
                        connector_id,
                        message.message_id,
                        locale.as_str()
                    )));
                }
                resources.entry(locale).or_default().insert(
                    message.message_id.to_string(),
                    translation.value.to_string(),
                );
            }
        }

        let mut revision_hasher = Sha256::new();
        revision_hasher.update(connector_id.as_str().as_bytes());
        revision_hasher.update([0]);
        revision_hasher.update(default_locale.as_str().as_bytes());
        revision_hasher.update([0]);
        for (locale, localized_messages) in &resources {
            revision_hasher.update(locale.as_str().as_bytes());
            revision_hasher.update([0]);
            for (message_id, value) in localized_messages {
                revision_hasher.update(message_id.as_bytes());
                revision_hasher.update([0]);
                revision_hasher.update(value.as_bytes());
                revision_hasher.update([0]);
            }
        }

        Self::new(
            connector_id,
            default_locale,
            hex::encode(revision_hasher.finalize()),
            resources,
        )
    }

    /// Builds a resource from already-assembled maps and validates it.
    ///
    /// `revision` is caller-chosen (1..=`MAX_REVISION_LENGTH` bytes, not
    /// blank); use [`Self::from_messages`] for a deterministic hash-derived
    /// revision. Returns [`StorageConnectorLocalizationError`] when the
    /// connector id, revision, locale coverage, or per-message limits fail
    /// validation.
    pub fn new(
        connector_id: ConnectorId,
        default_locale: LocaleTag,
        revision: impl Into<String>,
        resources: BTreeMap<LocaleTag, BTreeMap<String, String>>,
    ) -> Result<Self, StorageConnectorLocalizationError> {
        let localization = Self {
            connector_id,
            default_locale,
            revision: revision.into(),
            resources,
        };
        localization.validate()?;
        Ok(localization)
    }

    /// Returns the connector that owns this resource.
    pub fn connector_id(&self) -> &ConnectorId {
        &self.connector_id
    }

    /// Returns the resource revision; equal revisions imply identical
    /// localized content.
    pub fn revision(&self) -> &str {
        &self.revision
    }

    /// Builds the cacheable manifest advertised through the localization API.
    ///
    /// The manifest carries metadata only (namespace, locales, revision); the
    /// actual messages are delivered through [`Self::bundle`].
    pub fn manifest(&self) -> StorageConnectorLocalizationManifest {
        StorageConnectorLocalizationManifest {
            namespace: self.connector_id.as_str().to_string(),
            default_locale: self.default_locale.clone(),
            supported_locales: self.resources.keys().cloned().collect(),
            revision: self.revision.clone(),
        }
    }

    /// Resolves the best matching locale and returns its full message map.
    ///
    /// Resolution walks the requested tag up through its parent tags
    /// (`zh-CN` -> `zh`) with ASCII case-insensitive comparison and falls back
    /// to the default locale when nothing matches, so the returned bundle is
    /// always complete.
    pub fn bundle(&self, requested_locale: &LocaleTag) -> StorageConnectorLocalizationBundle {
        let resolved_locale = self.resolve_locale(requested_locale);
        StorageConnectorLocalizationBundle {
            connector_id: self.connector_id.clone(),
            namespace: self.connector_id.as_str().to_string(),
            requested_locale: requested_locale.clone(),
            resolved_locale: resolved_locale.clone(),
            revision: self.revision.clone(),
            messages: self.resources[&resolved_locale].clone(),
        }
    }

    /// Checks that every descriptor message id exists in the default locale.
    ///
    /// Callers (descriptor validation) pass the message ids a connector's
    /// admin UI references; a missing id returns
    /// [`StorageConnectorLocalizationError`] naming the connector and id.
    pub fn validate_message_ids<'a>(
        &self,
        message_ids: impl IntoIterator<Item = &'a str>,
    ) -> Result<(), StorageConnectorLocalizationError> {
        let default_messages = &self.resources[&self.default_locale];
        for message_id in message_ids {
            if !default_messages.contains_key(message_id) {
                return Err(StorageConnectorLocalizationError(format!(
                    "connector '{}' localization is missing descriptor message id '{}'",
                    self.connector_id, message_id
                )));
            }
        }
        Ok(())
    }

    fn resolve_locale(&self, requested_locale: &LocaleTag) -> LocaleTag {
        let mut candidate = requested_locale.as_str();
        loop {
            if let Some(locale) = self
                .resources
                .keys()
                .find(|locale| locale.as_str().eq_ignore_ascii_case(candidate))
            {
                return locale.clone();
            }
            let Some((parent, _)) = candidate.rsplit_once('-') else {
                break;
            };
            candidate = parent;
        }
        self.default_locale.clone()
    }

    fn validate(&self) -> Result<(), StorageConnectorLocalizationError> {
        self.connector_id
            .validate()
            .map_err(|error| StorageConnectorLocalizationError(error.to_string()))?;
        if self.revision.trim().is_empty() || self.revision.len() > MAX_REVISION_LENGTH {
            return Err(StorageConnectorLocalizationError(format!(
                "connector '{}' localization revision must be 1..={MAX_REVISION_LENGTH} bytes",
                self.connector_id
            )));
        }
        if self.resources.is_empty() || self.resources.len() > MAX_LOCALES {
            return Err(StorageConnectorLocalizationError(format!(
                "connector '{}' localization must contain 1..={MAX_LOCALES} locales",
                self.connector_id
            )));
        }
        let Some(default_messages) = self.resources.get(&self.default_locale) else {
            return Err(StorageConnectorLocalizationError(format!(
                "connector '{}' localization is missing default locale '{}'",
                self.connector_id,
                self.default_locale.as_str()
            )));
        };
        validate_messages(&self.connector_id, &self.default_locale, default_messages)?;
        let default_keys = default_messages.keys().collect::<BTreeSet<_>>();
        let default_placeholders = default_messages
            .iter()
            .map(|(key, value)| (key, interpolation_placeholders(value)))
            .collect::<BTreeMap<_, _>>();

        for (locale, messages) in &self.resources {
            validate_messages(&self.connector_id, locale, messages)?;
            let keys = messages.keys().collect::<BTreeSet<_>>();
            if keys != default_keys {
                return Err(StorageConnectorLocalizationError(format!(
                    "connector '{}' locale '{}' message ids differ from default locale '{}'",
                    self.connector_id,
                    locale.as_str(),
                    self.default_locale.as_str()
                )));
            }
            for (key, value) in messages {
                if interpolation_placeholders(value) != default_placeholders[key] {
                    return Err(StorageConnectorLocalizationError(format!(
                        "connector '{}' locale '{}' message '{}' uses different interpolation placeholders",
                        self.connector_id,
                        locale.as_str(),
                        key
                    )));
                }
            }
        }
        Ok(())
    }
}

/// Metadata describing one connector's localization resource, without the
/// message bodies.
///
/// Clients compare `revision` to decide whether to re-fetch bundles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorLocalizationManifest {
    /// Namespace the resource is served under; equals the connector id.
    pub namespace: String,
    /// Locale used when no supported locale matches the request.
    pub default_locale: LocaleTag,
    /// Locales the resource provides, sorted.
    pub supported_locales: Vec<LocaleTag>,
    /// Opaque revision token; content changes imply a new revision.
    pub revision: String,
}

/// One connector's resolved message set for a locale request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorLocalizationBundle {
    /// Connector that owns the messages.
    pub connector_id: ConnectorId,
    /// Serving namespace; equals the connector id.
    pub namespace: String,
    /// Locale the client asked for, before fallback.
    pub requested_locale: LocaleTag,
    /// Locale actually served after fallback resolution.
    pub resolved_locale: LocaleTag,
    /// Resource revision the messages belong to.
    pub revision: String,
    /// Message id to localized text for the resolved locale.
    pub messages: BTreeMap<String, String>,
}

/// Aggregated localization response across every connector mounted on a policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorLocalizationCatalog {
    /// Locale requested by the client; each bundle records its own resolution.
    pub requested_locale: LocaleTag,
    /// Per-connector bundles for the requested locale.
    pub resources: Vec<StorageConnectorLocalizationBundle>,
}

/// Validation or lookup failure for a connector localization resource.
///
/// The message text is user-facing and names the connector plus the offending
/// id, locale, or limit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageConnectorLocalizationError(String);

impl std::fmt::Display for StorageConnectorLocalizationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for StorageConnectorLocalizationError {}

fn validate_messages(
    connector_id: &ConnectorId,
    locale: &LocaleTag,
    messages: &BTreeMap<String, String>,
) -> Result<(), StorageConnectorLocalizationError> {
    if messages.is_empty() || messages.len() > MAX_MESSAGES {
        return Err(StorageConnectorLocalizationError(format!(
            "connector '{}' locale '{}' must contain 1..={MAX_MESSAGES} messages",
            connector_id,
            locale.as_str()
        )));
    }
    for (message_id, value) in messages {
        if message_id.is_empty()
            || message_id.len() > MAX_MESSAGE_ID_LENGTH
            || !message_id.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
            })
        {
            return Err(StorageConnectorLocalizationError(format!(
                "connector '{}' locale '{}' has invalid message id '{}'",
                connector_id,
                locale.as_str(),
                message_id
            )));
        }
        if value.trim().is_empty()
            || value.len() > MAX_MESSAGE_LENGTH
            || value
                .chars()
                .any(|character| character.is_control() && character != '\n' && character != '\t')
        {
            return Err(StorageConnectorLocalizationError(format!(
                "connector '{}' locale '{}' message '{}' has invalid text",
                connector_id,
                locale.as_str(),
                message_id
            )));
        }
    }
    Ok(())
}

fn interpolation_placeholders(value: &str) -> BTreeSet<&str> {
    let mut placeholders = BTreeSet::new();
    let mut remaining = value;
    while let Some(start) = remaining.find("{{") {
        remaining = &remaining[start + 2..];
        let Some(end) = remaining.find("}}") else {
            break;
        };
        let placeholder = remaining[..end].trim();
        if !placeholder.is_empty() {
            placeholders.insert(placeholder);
        }
        remaining = &remaining[end + 2..];
    }
    placeholders
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use aster_drive_model::types::LocaleTag;

    use super::{StorageConnectorLocalization, StorageConnectorLocalizationMessage};
    use crate::ConnectorId;

    fn localization(
        resources: BTreeMap<LocaleTag, BTreeMap<String, String>>,
    ) -> Result<StorageConnectorLocalization, super::StorageConnectorLocalizationError> {
        StorageConnectorLocalization::new(
            ConnectorId::declared("com.example.storage"),
            LocaleTag::parse("en").unwrap(),
            "1",
            resources,
        )
    }

    #[test]
    fn resolves_exact_parent_and_default_locales() {
        let localization = localization(BTreeMap::from([
            (
                LocaleTag::parse("en").unwrap(),
                BTreeMap::from([("label".to_string(), "Storage".to_string())]),
            ),
            (
                LocaleTag::parse("zh").unwrap(),
                BTreeMap::from([("label".to_string(), "存储".to_string())]),
            ),
        ]))
        .unwrap();

        assert_eq!(
            localization
                .bundle(&LocaleTag::parse("zh-CN").unwrap())
                .resolved_locale
                .as_str(),
            "zh"
        );
        assert_eq!(
            localization
                .bundle(&LocaleTag::parse("fr-FR").unwrap())
                .resolved_locale
                .as_str(),
            "en"
        );
    }

    #[test]
    fn rejects_missing_keys_placeholder_drift_and_invalid_bounds() {
        assert!(
            localization(BTreeMap::from([
                (
                    LocaleTag::parse("en").unwrap(),
                    BTreeMap::from([("summary".to_string(), "Open {{name}}".to_string())]),
                ),
                (
                    LocaleTag::parse("zh").unwrap(),
                    BTreeMap::from([("label".to_string(), "打开".to_string())]),
                ),
            ]))
            .is_err()
        );
        assert!(
            localization(BTreeMap::from([
                (
                    LocaleTag::parse("en").unwrap(),
                    BTreeMap::from([("summary".to_string(), "Open {{name}}".to_string())]),
                ),
                (
                    LocaleTag::parse("zh").unwrap(),
                    BTreeMap::from([("summary".to_string(), "打开 {{path}}".to_string())]),
                ),
            ]))
            .is_err()
        );
        assert!(
            localization(BTreeMap::from([(
                LocaleTag::parse("en").unwrap(),
                BTreeMap::from([("INVALID KEY".to_string(), "value".to_string())]),
            )]))
            .is_err()
        );
    }

    #[test]
    fn rust_message_declarations_are_deterministic_and_locale_open() {
        const MESSAGES: &[StorageConnectorLocalizationMessage<'static>] = &[
            crate::storage_connector_message!(
                "summary";
                "en" => "Open {{name}}",
                "zh" => "打开 {{name}}",
                "ja" => "{{name}} を開く",
            ),
            crate::storage_connector_message!(
                "title";
                "en" => "Storage",
                "zh" => "存储",
                "ja" => "ストレージ",
            ),
        ];
        let connector_id = ConnectorId::declared("com.example.storage");
        let forward = StorageConnectorLocalization::from_messages(
            connector_id.clone(),
            "en",
            MESSAGES.iter(),
        )
        .unwrap();
        let reverse =
            StorageConnectorLocalization::from_messages(connector_id, "en", MESSAGES.iter().rev())
                .unwrap();

        assert_eq!(forward.revision(), reverse.revision());
        assert_eq!(
            forward
                .bundle(&LocaleTag::parse("ja-JP").unwrap())
                .resolved_locale
                .as_str(),
            "ja"
        );
        assert_eq!(
            forward
                .bundle(&LocaleTag::parse("fr").unwrap())
                .resolved_locale
                .as_str(),
            "en"
        );
    }

    #[test]
    fn rust_message_declarations_reject_duplicates_and_locale_drift() {
        const DUPLICATE_IDS: &[StorageConnectorLocalizationMessage<'static>] = &[
            crate::storage_connector_message!("title", "Storage", "存储"),
            crate::storage_connector_message!("title", "Duplicate", "重复"),
        ];
        let error = StorageConnectorLocalization::from_messages(
            ConnectorId::declared("com.example.storage"),
            "en",
            DUPLICATE_IDS.iter(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("more than once"));

        const DUPLICATE_LOCALE: &[StorageConnectorLocalizationMessage<'static>] =
            &[crate::storage_connector_message!(
                "title";
                "en" => "Storage",
                "EN" => "Duplicate",
            )];
        let error = StorageConnectorLocalization::from_messages(
            ConnectorId::declared("com.example.storage"),
            "en",
            DUPLICATE_LOCALE.iter(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("locale 'en' more than once"));

        const KEY_DRIFT: &[StorageConnectorLocalizationMessage<'static>] = &[
            crate::storage_connector_message!("title"; "en" => "Storage", "zh" => "存储"),
            crate::storage_connector_message!("summary"; "en" => "Open storage"),
        ];
        let error = StorageConnectorLocalization::from_messages(
            ConnectorId::declared("com.example.storage"),
            "en",
            KEY_DRIFT.iter(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("message ids differ"));
    }
}

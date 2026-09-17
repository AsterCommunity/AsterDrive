use std::str::FromStr;

use aster_drive_model::entities::folder;
use aster_drive_model::types::FolderIconKind;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::errors::{AsterError, Result};

pub const MAX_FOLDER_EMOJI_BYTES: usize = 64;

macro_rules! folder_builtin_icons {
    ($($variant:ident => $key:literal),+ $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        #[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
        #[serde(rename_all = "snake_case")]
        pub enum FolderBuiltinIcon {
            $($variant),+
        }

        impl FolderBuiltinIcon {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $key),+
                }
            }
        }

        impl FromStr for FolderBuiltinIcon {
            type Err = AsterError;

            fn from_str(value: &str) -> Result<Self> {
                match value {
                    $($key => Ok(Self::$variant)),+,
                    _ => Err(AsterError::validation_error("unsupported folder builtin icon")),
                }
            }
        }
    };
}

folder_builtin_icons! {
    Documents => "documents",
    Images => "images",
    Music => "music",
    Videos => "videos",
    Work => "work",
    Home => "home",
    Archive => "archive",
    Library => "library",
    Database => "database",
    Calendar => "calendar",
    Ideas => "ideas",
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum FolderIcon {
    Default {},
    Builtin { key: FolderBuiltinIcon },
    Emoji { value: String },
}

impl Default for FolderIcon {
    fn default() -> Self {
        Self::Default {}
    }
}

impl FolderIcon {
    pub fn normalize(self) -> Result<(FolderIconKind, Option<String>)> {
        match self {
            Self::Default {} => Ok((FolderIconKind::Default, None)),
            Self::Builtin { key } => Ok((FolderIconKind::Builtin, Some(key.as_str().to_owned()))),
            Self::Emoji { value } => {
                if value.is_empty() || value.len() > MAX_FOLDER_EMOJI_BYTES {
                    return Err(AsterError::validation_error(format!(
                        "folder emoji must be 1 to {MAX_FOLDER_EMOJI_BYTES} UTF-8 bytes"
                    )));
                }
                let emoji = emojis::get(&value).ok_or_else(|| {
                    AsterError::validation_error("folder emoji must be exactly one RGI emoji")
                })?;
                Ok((FolderIconKind::Emoji, Some(emoji.as_str().to_owned())))
            }
        }
    }

    pub fn from_persisted(kind: FolderIconKind, value: Option<&str>) -> Result<Self> {
        match (kind, value) {
            (FolderIconKind::Default, None) => Ok(Self::Default {}),
            (FolderIconKind::Builtin, Some(value)) => Ok(Self::Builtin {
                key: FolderBuiltinIcon::from_str(value)?,
            }),
            (FolderIconKind::Emoji, Some(value)) => {
                let (kind, value) = Self::Emoji {
                    value: value.to_owned(),
                }
                .normalize()?;
                debug_assert_eq!(kind, FolderIconKind::Emoji);
                Ok(Self::Emoji {
                    value: value.unwrap_or_default(),
                })
            }
            _ => Err(AsterError::internal_error(
                "folder icon kind and value violate the persistence contract",
            )),
        }
    }

    pub fn from_model(model: &folder::Model) -> Self {
        Self::from_persisted(model.icon_kind, model.icon_value.as_deref()).unwrap_or_else(|error| {
            tracing::error!(folder_id = model.id, %error, "invalid persisted folder icon");
            Self::Default {}
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_keys_round_trip_and_reject_unknown_values() {
        for icon in FolderBuiltinIcon::ALL {
            assert_eq!(FolderBuiltinIcon::from_str(icon.as_str()).unwrap(), *icon);
        }
        assert!(FolderBuiltinIcon::from_str("FcDocument").is_err());
        assert!(FolderBuiltinIcon::from_str("documents ").is_err());
        assert!(FolderBuiltinIcon::from_str("unknown").is_err());
    }

    #[test]
    fn emoji_normalization_accepts_rgi_sequences() {
        for value in ["📚", "👍🏽", "👩🏿‍❤️‍👨🏼", "🇨🇳", "1️⃣", "✌️"]
        {
            let normalized = FolderIcon::Emoji {
                value: value.to_owned(),
            }
            .normalize()
            .unwrap();
            assert_eq!(normalized.0, FolderIconKind::Emoji);
            assert!(normalized.1.is_some());
        }
    }

    #[test]
    fn emoji_normalization_rejects_non_rgi_and_multiple_values() {
        for value in ["", "A", "📚📁", " 📚", "📚 ", "\u{200d}", "\n", "☕x"] {
            assert!(
                FolderIcon::Emoji {
                    value: value.to_owned()
                }
                .normalize()
                .is_err(),
                "{value:?}"
            );
        }
        let oversized = "📚".repeat((MAX_FOLDER_EMOJI_BYTES / "📚".len()) + 1);
        assert!(FolderIcon::Emoji { value: oversized }.normalize().is_err());
    }

    #[test]
    fn persisted_kind_value_pairs_are_strict() {
        assert_eq!(
            FolderIcon::from_persisted(FolderIconKind::Default, None).unwrap(),
            FolderIcon::Default {}
        );
        assert!(FolderIcon::from_persisted(FolderIconKind::Default, Some("documents")).is_err());
        assert!(FolderIcon::from_persisted(FolderIconKind::Builtin, None).is_err());
        assert!(FolderIcon::from_persisted(FolderIconKind::Emoji, Some("text")).is_err());
    }

    #[test]
    fn wire_format_rejects_fields_from_other_variants() {
        assert!(
            serde_json::from_value::<FolderIcon>(serde_json::json!({
                "kind": "default",
                "value": "ignored"
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<FolderIcon>(serde_json::json!({
                "kind": "builtin",
                "key": "documents",
                "value": "ignored"
            }))
            .is_err()
        );
    }
}

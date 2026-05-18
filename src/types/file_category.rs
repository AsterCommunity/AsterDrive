use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// File category persisted on `files` for indexed browsing and search filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(32))")]
#[serde(rename_all = "lowercase")]
pub enum FileCategory {
    #[sea_orm(string_value = "image")]
    Image,
    #[sea_orm(string_value = "video")]
    Video,
    #[sea_orm(string_value = "audio")]
    Audio,
    #[sea_orm(string_value = "document")]
    Document,
    #[sea_orm(string_value = "spreadsheet")]
    Spreadsheet,
    #[sea_orm(string_value = "presentation")]
    Presentation,
    #[sea_orm(string_value = "archive")]
    Archive,
    #[sea_orm(string_value = "code")]
    Code,
    #[sea_orm(string_value = "other")]
    Other,
}

impl FileCategory {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Video => "video",
            Self::Audio => "audio",
            Self::Document => "document",
            Self::Spreadsheet => "spreadsheet",
            Self::Presentation => "presentation",
            Self::Archive => "archive",
            Self::Code => "code",
            Self::Other => "other",
        }
    }
}

impl std::str::FromStr for FileCategory {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "image" => Ok(Self::Image),
            "video" => Ok(Self::Video),
            "audio" => Ok(Self::Audio),
            "document" => Ok(Self::Document),
            "spreadsheet" => Ok(Self::Spreadsheet),
            "presentation" => Ok(Self::Presentation),
            "archive" => Ok(Self::Archive),
            "code" => Ok(Self::Code),
            "other" => Ok(Self::Other),
            _ => Err(()),
        }
    }
}

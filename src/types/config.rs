use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use std::fmt;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// 运行时配置值类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(32))")]
#[serde(rename_all = "snake_case")]
pub enum SystemConfigValueType {
    #[sea_orm(string_value = "string")]
    String,
    #[sea_orm(string_value = "multiline")]
    Multiline,
    #[sea_orm(string_value = "string_array")]
    StringArray,
    #[sea_orm(string_value = "string_enum_set")]
    StringEnumSet,
    #[sea_orm(string_value = "number")]
    Number,
    #[sea_orm(string_value = "boolean")]
    Boolean,
}

impl SystemConfigValueType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Multiline => "multiline",
            Self::StringArray => "string_array",
            Self::StringEnumSet => "string_enum_set",
            Self::Number => "number",
            Self::Boolean => "boolean",
        }
    }

    pub fn from_str_name(value: &str) -> Option<Self> {
        match value {
            "string" => Some(Self::String),
            "multiline" => Some(Self::Multiline),
            "string_array" => Some(Self::StringArray),
            "string_enum_set" => Some(Self::StringEnumSet),
            "number" => Some(Self::Number),
            "boolean" => Some(Self::Boolean),
            _ => None,
        }
    }

    pub const fn is_multiline(self) -> bool {
        matches!(self, Self::Multiline)
    }

    pub const fn is_string_array(self) -> bool {
        matches!(self, Self::StringArray)
    }

    pub const fn is_string_enum_set(self) -> bool {
        matches!(self, Self::StringEnumSet)
    }

    pub const fn is_string_list(self) -> bool {
        matches!(self, Self::StringArray | Self::StringEnumSet)
    }
}

impl fmt::Display for SystemConfigValueType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 运行时配置来源
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "snake_case")]
pub enum SystemConfigSource {
    #[sea_orm(string_value = "system")]
    System,
    #[sea_orm(string_value = "custom")]
    Custom,
}

impl SystemConfigSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Custom => "custom",
        }
    }

    pub fn from_str_name(value: &str) -> Option<Self> {
        match value {
            "system" => Some(Self::System),
            "custom" => Some(Self::Custom),
            _ => None,
        }
    }
}

impl fmt::Display for SystemConfigSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::{SystemConfigSource, SystemConfigValueType};

    #[test]
    fn system_config_value_type_round_trips_string_names() {
        let cases = [
            (SystemConfigValueType::String, "string"),
            (SystemConfigValueType::Multiline, "multiline"),
            (SystemConfigValueType::StringArray, "string_array"),
            (SystemConfigValueType::StringEnumSet, "string_enum_set"),
            (SystemConfigValueType::Number, "number"),
            (SystemConfigValueType::Boolean, "boolean"),
        ];

        for (value_type, name) in cases {
            assert_eq!(value_type.as_str(), name);
            assert_eq!(value_type.to_string(), name);
            assert_eq!(SystemConfigValueType::from_str_name(name), Some(value_type));
        }
        assert_eq!(SystemConfigValueType::from_str_name("unknown"), None);
    }

    #[test]
    fn system_config_value_type_classifies_multiline_and_arrays() {
        assert!(SystemConfigValueType::Multiline.is_multiline());
        assert!(!SystemConfigValueType::String.is_multiline());

        assert!(SystemConfigValueType::StringArray.is_string_array());
        assert!(!SystemConfigValueType::Boolean.is_string_array());

        assert!(SystemConfigValueType::StringEnumSet.is_string_enum_set());
        assert!(!SystemConfigValueType::StringArray.is_string_enum_set());

        assert!(SystemConfigValueType::StringArray.is_string_list());
        assert!(SystemConfigValueType::StringEnumSet.is_string_list());
        assert!(!SystemConfigValueType::String.is_string_list());
    }

    #[test]
    fn system_config_source_round_trips_string_names() {
        let cases = [
            (SystemConfigSource::System, "system"),
            (SystemConfigSource::Custom, "custom"),
        ];

        for (source, name) in cases {
            assert_eq!(source.as_str(), name);
            assert_eq!(source.to_string(), name);
            assert_eq!(SystemConfigSource::from_str_name(name), Some(source));
        }
        assert_eq!(SystemConfigSource::from_str_name("tenant"), None);
    }
}

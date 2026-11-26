//! 归档条目文件名解码策略。

use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// Archive entry filename decoding strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ArchiveFilenameEncoding {
    /// Trust the ZIP UTF-8 flag first, then try UTF-8 and legacy CJK / DOS
    /// candidates from the raw bytes instead of rejecting the entry.
    #[default]
    Auto,
    /// Decode as UTF-8 only; invalid byte sequences fail the entry.
    Utf8,
    /// Force GB18030 (simplified Chinese) decoding.
    Gb18030,
    /// Force CP437 (original IBM PC / MS-DOS) decoding.
    Cp437,
    /// Force CP850 (Western European MS-DOS) decoding.
    Cp850,
    /// Force Shift_JIS (Japanese) decoding.
    ShiftJis,
    /// Force Big5 (traditional Chinese) decoding.
    Big5,
    /// Force EUC-KR (Korean) decoding.
    EucKr,
    /// Force Windows-1252 (Western European) decoding.
    #[serde(rename = "windows_1252")]
    #[cfg_attr(
        all(debug_assertions, feature = "openapi"),
        schema(rename = "windows_1252")
    )]
    Windows1252,
}

impl ArchiveFilenameEncoding {
    /// Returns the snake_case strategy name used by configuration and diagnostics.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Utf8 => "utf8",
            Self::Gb18030 => "gb18030",
            Self::Cp437 => "cp437",
            Self::Cp850 => "cp850",
            Self::ShiftJis => "shift_jis",
            Self::Big5 => "big5",
            Self::EucKr => "euc_kr",
            Self::Windows1252 => "windows_1252",
        }
    }
}

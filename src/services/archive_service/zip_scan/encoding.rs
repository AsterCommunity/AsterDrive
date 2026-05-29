use std::io::Read;

use encoding_rs::{BIG5, EUC_KR, Encoding, GB18030, SHIFT_JIS, WINDOWS_1252};
use oem_cp::{
    code_table::{DECODING_TABLE_CP437, DECODING_TABLE_CP850},
    decode_string_complete_table,
};
use zip::HasZipMetadata;

use crate::errors::{AsterError, Result};
use crate::types::ArchiveFilenameEncoding;

pub(super) fn decode_zip_entry_name<R: Read>(
    entry: &zip::read::ZipFile<'_, R>,
    filename_encoding: ArchiveFilenameEncoding,
) -> Result<String> {
    let raw = entry.name_raw();
    decode_zip_entry_name_parts(
        raw,
        entry.name(),
        entry.get_metadata().is_utf8,
        filename_encoding,
    )
}

pub(super) fn decode_zip_entry_name_parts(
    raw: &[u8],
    display_name: &str,
    raw_name_utf8: bool,
    filename_encoding: ArchiveFilenameEncoding,
) -> Result<String> {
    match filename_encoding {
        ArchiveFilenameEncoding::Auto => {
            decode_zip_entry_name_auto_parts(raw, display_name, raw_name_utf8)
        }
        ArchiveFilenameEncoding::Utf8 => decode_zip_entry_name_utf8(raw, display_name),
        ArchiveFilenameEncoding::Gb18030 => decode_gb18030(raw).ok_or_else(|| {
            AsterError::validation_error(format!(
                "archive entry '{}' filename is not valid GB18030",
                display_name
            ))
        }),
        ArchiveFilenameEncoding::Cp437 => {
            Ok(decode_string_complete_table(raw, &DECODING_TABLE_CP437))
        }
        ArchiveFilenameEncoding::Cp850 => {
            Ok(decode_string_complete_table(raw, &DECODING_TABLE_CP850))
        }
        ArchiveFilenameEncoding::ShiftJis => {
            decode_zip_entry_name_legacy_encoding(raw, display_name, SHIFT_JIS, "Shift_JIS")
        }
        ArchiveFilenameEncoding::Big5 => {
            decode_zip_entry_name_legacy_encoding(raw, display_name, BIG5, "Big5")
        }
        ArchiveFilenameEncoding::EucKr => {
            decode_zip_entry_name_legacy_encoding(raw, display_name, EUC_KR, "EUC-KR")
        }
        ArchiveFilenameEncoding::Windows1252 => {
            decode_zip_entry_name_legacy_encoding(raw, display_name, WINDOWS_1252, "Windows-1252")
        }
    }
}

fn decode_zip_entry_name_auto_parts(
    raw: &[u8],
    display_name: &str,
    raw_name_utf8: bool,
) -> Result<String> {
    if raw_name_utf8 {
        return decode_zip_entry_name_utf8(raw, display_name);
    }

    if let Ok(name) = std::str::from_utf8(raw) {
        return Ok(name.to_string());
    }

    if raw.iter().any(|byte| *byte >= 0x80)
        && let Some(name) = decode_gb18030(raw)
        && contains_gb18030_cjk_signal(&name)
    {
        return Ok(name);
    }

    Ok(display_name.to_string())
}

fn decode_zip_entry_name_utf8(raw: &[u8], display_name: &str) -> Result<String> {
    std::str::from_utf8(raw)
        .map(|value| value.to_string())
        .map_err(|_| {
            AsterError::validation_error(format!(
                "archive entry '{}' filename is not valid UTF-8",
                display_name
            ))
        })
}

fn decode_gb18030(raw: &[u8]) -> Option<String> {
    GB18030
        .decode_without_bom_handling_and_without_replacement(raw)
        .map(|value| value.into_owned())
}

fn decode_zip_entry_name_legacy_encoding(
    raw: &[u8],
    display_name: &str,
    encoding: &'static Encoding,
    encoding_label: &str,
) -> Result<String> {
    encoding
        .decode_without_bom_handling_and_without_replacement(raw)
        .map(|value| value.into_owned())
        .ok_or_else(|| {
            AsterError::validation_error(format!(
                "archive entry '{}' filename is not valid {}",
                display_name, encoding_label
            ))
        })
}

fn contains_gb18030_cjk_signal(value: &str) -> bool {
    value.chars().any(|ch| {
        matches!(
            ch,
            '\u{2e80}'..='\u{2eff}'
                | '\u{3000}'..='\u{303f}'
                | '\u{3400}'..='\u{4dbf}'
                | '\u{4e00}'..='\u{9fff}'
                | '\u{f900}'..='\u{faff}'
                | '\u{ff00}'..='\u{ffef}'
        )
    })
}

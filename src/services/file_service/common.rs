//! 文件服务子模块：`common`。

use crate::entities::file;
use crate::errors::Result;
use crate::services::workspace_storage_service;
use crate::utils::http_validators;

const INLINE_SANDBOX_CSP: &str = "sandbox";

pub(crate) fn inline_sandbox_csp() -> &'static str {
    INLINE_SANDBOX_CSP
}

pub(crate) fn requires_inline_sandbox(mime_type: &str) -> bool {
    let normalized = mime_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();

    matches!(
        normalized.as_str(),
        "text/html" | "application/xhtml+xml" | "image/svg+xml"
    )
}

pub(crate) fn ensure_personal_file_scope(file: &file::Model) -> Result<()> {
    workspace_storage_service::ensure_personal_file_scope(file)
}

pub(crate) fn if_none_match_matches_value(if_none_match: &str, etag_value: &str) -> bool {
    http_validators::if_none_match_header_matches(if_none_match, true, Some(etag_value))
        .unwrap_or(false)
}

pub(crate) fn if_none_match_matches(if_none_match: &str, blob_hash: &str) -> bool {
    if_none_match_matches_value(if_none_match, blob_hash)
}

#[cfg(test)]
mod tests {
    use super::requires_inline_sandbox;

    #[test]
    fn dangerous_same_origin_inline_mime_types_require_sandbox() {
        assert!(requires_inline_sandbox("text/html"));
        assert!(requires_inline_sandbox("application/xhtml+xml"));
        assert!(requires_inline_sandbox("image/svg+xml"));
        assert!(requires_inline_sandbox("text/html; charset=utf-8"));
        assert!(!requires_inline_sandbox("text/plain"));
        assert!(!requires_inline_sandbox("application/pdf"));
    }
}

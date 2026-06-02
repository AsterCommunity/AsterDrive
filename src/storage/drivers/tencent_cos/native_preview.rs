use async_trait::async_trait;
use chrono::{TimeZone, Utc};

use crate::errors::{AsterError, MapAsterErr, Result};
use crate::storage::extensions::{
    NativePreviewMode, NativePreviewOpenMode, NativePreviewRequest, NativePreviewResult,
    NativePreviewStorageDriver,
};

use super::signing::clamp_cos_ttl;
use super::{
    COS_NATIVE_PREVIEW_PROVIDER, COS_NATIVE_PREVIEW_VERSION, MAX_COS_PREVIEW_TTL, TencentCosDriver,
};

impl TencentCosDriver {
    pub(super) fn signed_ci_preview_url(
        &self,
        path: &str,
        expires: std::time::Duration,
    ) -> Result<NativePreviewResult> {
        let expires = clamp_cos_ttl(expires, MAX_COS_PREVIEW_TTL, "preview");
        let now = Utc::now();
        let start = now.timestamp();
        let end = (now
            + chrono::Duration::from_std(expires)
                .map_aster_err_ctx("COS preview expiry", AsterError::storage_driver_error)?)
        .timestamp();
        let key_time = format!("{start};{end}");
        let ci_params = [("ci-process", "doc-preview"), ("dstType", "html")];
        let (url, key) = self.signed_cos_query_url(path, &ci_params, &key_time)?;

        Ok(NativePreviewResult {
            url: String::from(url),
            provider: COS_NATIVE_PREVIEW_PROVIDER.to_string(),
            expires_at: Utc.timestamp_opt(end, 0).single().unwrap_or(now),
            cache_key: Some(format!(
                "{COS_NATIVE_PREVIEW_PROVIDER}:{COS_NATIVE_PREVIEW_VERSION}:{key}"
            )),
            version: Some(COS_NATIVE_PREVIEW_VERSION.to_string()),
            open_mode: NativePreviewOpenMode::Iframe,
        })
    }
}

pub(super) fn is_cos_doc_preview_candidate(file_name: &str, mime_type: &str) -> bool {
    const EXTENSIONS: &[&str] = &[
        "doc", "docx", "ppt", "pptx", "xls", "xlsx", "pdf", "txt", "rtf", "odt", "ods", "odp",
    ];
    let extension = std::path::Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.trim().to_ascii_lowercase());

    extension
        .as_deref()
        .map(|value| EXTENSIONS.contains(&value))
        .unwrap_or(false)
        || mime_type == "application/pdf"
}

#[async_trait]
impl NativePreviewStorageDriver for TencentCosDriver {
    async fn create_native_preview(
        &self,
        request: &NativePreviewRequest,
    ) -> Result<Option<NativePreviewResult>> {
        if request.mode != NativePreviewMode::HtmlDocument
            || !is_cos_doc_preview_candidate(&request.source_file_name, &request.source_mime_type)
        {
            return Ok(None);
        }

        self.signed_ci_preview_url(&request.storage_path, request.expires)
            .map(Some)
    }
}

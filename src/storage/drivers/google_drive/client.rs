//! Google Drive HTTP, pool validation and application-owned object lookup.
use super::*;

pub(crate) fn http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        // Drive uses 308 as a resumable acknowledgement, not a redirect.
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(120))
        .user_agent(crate::config::OUTBOUND_HTTP_USER_AGENT)
        .build()
        .map_err(|_| {
            storage_driver_error(
                StorageErrorKind::Misconfigured,
                "cannot build Google Drive HTTP client",
            )
        })
}

#[derive(Deserialize)]
pub(super) struct FileInfo {
    pub(super) id: String,
    // Drive encodes int64 sizes as JSON strings.
    pub(super) size: Option<String>,
    #[serde(rename = "mimeType")]
    pub(super) mime_type: Option<String>,
    name: Option<String>,
    #[serde(rename = "driveId")]
    drive_id: Option<String>,
    capabilities: Option<FolderCapabilities>,
}
#[derive(Deserialize)]
struct FolderCapabilities {
    #[serde(rename = "canAddChildren", default)]
    can_add_children: bool,
}
#[derive(Deserialize)]
struct FileList {
    #[serde(default)]
    files: Vec<FileInfo>,
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
    #[serde(rename = "incompleteSearch", default)]
    incomplete_search: bool,
}
pub(super) fn invalid(message: &str) -> StorageError {
    storage_driver_error(StorageErrorKind::Precondition, message)
}
pub(super) fn transient(message: &str) -> StorageError {
    storage_driver_error(StorageErrorKind::Transient, message)
}
fn escape_query(value: &str) -> String {
    // Drive q has its own escaping rules, separate from URL query encoding.
    value.replace('\\', "\\\\").replace('\'', "\\'")
}
pub(super) fn validate_path(path: &str) -> Result<()> {
    // Drive limits each property key + value to 124 UTF-8 bytes.
    if path.is_empty() || APP_KEY.len() + path.len() > 124 || path.chars().any(char::is_control) {
        return Err(invalid("invalid Google Drive logical object key"));
    }
    Ok(())
}
impl GoogleDriveDriver {
    pub(super) async fn auth(&self, request: reqwest::RequestBuilder) -> Result<reqwest::Response> {
        request
            .bearer_auth(self.token_provider.access_token().await?)
            .send()
            .await
            .map_err(|e| map_transport("Google Drive request", e))
    }
    pub(super) async fn response<T: for<'de> Deserialize<'de>>(
        &self,
        response: reqwest::Response,
    ) -> Result<T> {
        if !response.status().is_success() {
            return Err(map_http(response).await);
        }
        response
            .json()
            .await
            .map_err(|_| transient("invalid Google Drive JSON response"))
    }
    pub(super) fn object_query(&self, path: &str) -> Result<String> {
        validate_path(path)?;
        Ok(format!(
            "appProperties has {{ key='{APP_KEY}' and value='{}' }} and trashed=false and '{}' in parents",
            escape_query(path),
            escape_query(self.root())
        ))
    }
    pub(super) async fn find(&self, path: &str) -> Result<Option<FileInfo>> {
        let query = self.object_query(path)?;
        let mut found: Option<FileInfo> = None;
        let mut page_token: Option<String> = None;
        // files.list may return short or empty pages. A token is continuation,
        // not evidence of duplicate objects. Bound provider work and fail closed
        // when a search cannot prove absence or uniqueness.
        // https://developers.google.com/workspace/drive/api/reference/rest/v3/files/list
        // MAX_LOOKUP_PAGES is our request budget, not a Google pagination limit.
        for _ in 0..MAX_LOOKUP_PAGES {
            let mut request = self.http.get(format!("{}/files", self.api)).query(&[
                ("q", query.clone()),
                (
                    "fields",
                    "nextPageToken,incompleteSearch,files(id,size,mimeType)".into(),
                ),
                ("pageSize", "2".into()),
                (
                    "spaces",
                    if self.app_data_folder {
                        "appDataFolder"
                    } else {
                        "drive"
                    }
                    .into(),
                ),
                ("supportsAllDrives", "true".into()),
                ("includeItemsFromAllDrives", "true".into()),
            ]);
            if let Some(drive) = &self.shared_drive_id {
                request = request.query(&[("driveId", drive.as_str()), ("corpora", "drive")]);
            }
            if let Some(token) = &page_token {
                request = request.query(&[("pageToken", token)]);
            }
            let list: FileList = self.response(self.auth(request).await?).await?;
            if list.incomplete_search {
                return Err(transient("Google Drive object search is incomplete"));
            }
            for info in list.files {
                if info.id.is_empty() {
                    return Err(transient("Google Drive returned an empty file ID"));
                }
                if found
                    .as_ref()
                    .is_some_and(|existing| existing.id != info.id)
                {
                    return Err(storage_driver_error(
                        StorageErrorKind::Misconfigured,
                        "multiple Google Drive objects have the same AsterDrive key",
                    ));
                }
                found = Some(info);
            }
            match list.next_page_token.filter(|token| !token.is_empty()) {
                None => return Ok(found),
                Some(next) if page_token.as_ref() == Some(&next) => {
                    return Err(transient(
                        "Google Drive object search repeated a page token",
                    ));
                }
                next => page_token = next,
            }
        }
        Err(transient(
            "Google Drive object search exceeded the page limit",
        ))
    }
    pub(super) fn file_url(&self, id: &str) -> String {
        format!("{}/files/{}", self.api, urlencoding::encode(id))
    }
    pub(crate) async fn validate_root(&self) -> Result<(String, Option<String>)> {
        if self.app_data_folder {
            // appDataFolder is a reserved space, not a normal folder resource.
            // Listing proves access; the saved-policy write/delete connection
            // test separately proves data-plane permissions.
            let _: FileList = self
                .response(
                    self.auth(self.http.get(format!("{}/files", self.api)).query(&[
                        ("spaces", "appDataFolder"),
                        ("pageSize", "1"),
                        ("fields", "files(id)"),
                    ]))
                    .await?,
                )
                .await?;
            return Ok(("appDataFolder".into(), Some("Application data".into())));
        }
        let info: FileInfo = self
            .response(
                self.auth(self.http.get(self.file_url(self.root())).query(&[
                    ("supportsAllDrives", "true"),
                    (
                        "fields",
                        "id,name,mimeType,driveId,capabilities(canAddChildren)",
                    ),
                ]))
                .await?,
            )
            .await?;
        if info.mime_type.as_deref() != Some("application/vnd.google-apps.folder") {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "Google Drive root must be a folder",
            ));
        }
        if self
            .shared_drive_id
            .as_ref()
            .is_some_and(|id| info.drive_id.as_ref() != Some(id))
        {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "Google Drive folder does not belong to the configured shared drive",
            ));
        }
        if !info.capabilities.is_some_and(|c| c.can_add_children) {
            return Err(storage_driver_error(
                StorageErrorKind::Permission,
                "Google Drive folder is not writable",
            ));
        }
        Ok((info.id, info.name))
    }
    pub(super) async fn download(
        &self,
        path: &str,
        range: Option<String>,
    ) -> Result<reqwest::Response> {
        let info = self.find(path).await?.ok_or_else(|| {
            storage_driver_error(StorageErrorKind::NotFound, "Google Drive object not found")
        })?;
        let mut request = self
            .http
            .get(self.file_url(&info.id))
            .query(&[("alt", "media"), ("supportsAllDrives", "true")]);
        if let Some(range) = &range {
            request = request.header(RANGE, range);
        }
        let response = self.auth(request).await?;
        if !response.status().is_success() {
            return Err(map_http(response).await);
        }
        if range.is_some() && response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
            return Err(transient("Google Drive ignored the requested byte range"));
        }
        Ok(response)
    }
}
pub(crate) async fn map_http(response: reqwest::Response) -> StorageError {
    let status = response.status();
    // Provider messages may contain private file names or upload URLs. Only inspect the reason.
    let reason = response
        .json::<serde_json::Value>()
        .await
        .ok()
        .and_then(|v| {
            v["error"]["errors"][0]["reason"]
                .as_str()
                .map(str::to_owned)
        });
    let kind = match status.as_u16() {
        401 => StorageErrorKind::Auth,
        403 if matches!(
            reason.as_deref(),
            Some("rateLimitExceeded" | "userRateLimitExceeded" | "dailyLimitExceeded")
        ) =>
        {
            StorageErrorKind::RateLimited
        }
        403 => StorageErrorKind::Permission,
        404 | 410 => StorageErrorKind::NotFound,
        408 => StorageErrorKind::Transient,
        412 | 416 => StorageErrorKind::Precondition,
        429 => StorageErrorKind::RateLimited,
        500..=599 => StorageErrorKind::Transient,
        _ => StorageErrorKind::Misconfigured,
    };
    storage_driver_error(
        kind,
        format!("Google Drive request failed (HTTP {})", status.as_u16()),
    )
}
pub(super) fn map_transport(context: &str, error: reqwest::Error) -> StorageError {
    storage_driver_error(
        StorageErrorKind::Transient,
        format!("{context}: {}", error.without_url()),
    )
}

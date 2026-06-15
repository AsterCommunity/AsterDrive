use chrono::Utc;
use futures::TryStreamExt;
use reqwest::header::{AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, RANGE};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncRead;
use tokio_util::io::StreamReader;

use crate::errors::{AsterError, MapAsterErr, Result};
use crate::storage::error::{StorageErrorKind, storage_driver_error};
use crate::storage::traits::driver::BlobMetadata;
use crate::storage::traits::extensions::{StorageCapacityInfo, StorageCapacityStatus};
use crate::utils::OUTBOUND_HTTP_USER_AGENT;

use super::error::{invalid_graph_url, map_graph_response_error, map_reqwest_error};

#[derive(Clone, Debug)]
pub struct MicrosoftGraphClient {
    config: MicrosoftGraphClientConfig,
    http: reqwest::Client,
}

#[derive(Clone, Debug)]
pub struct MicrosoftGraphClientConfig {
    pub graph_base_url: String,
    pub access_token: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MicrosoftGraphDriveItem {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub size: Option<i64>,
    #[serde(default)]
    #[serde(rename = "file")]
    pub file: Option<serde_json::Value>,
    #[serde(default)]
    #[serde(rename = "folder")]
    pub folder: Option<serde_json::Value>,
    #[serde(default)]
    #[serde(rename = "parentReference")]
    pub parent_reference: Option<MicrosoftGraphDriveItemParentReference>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MicrosoftGraphDriveItemParentReference {
    #[serde(default)]
    #[serde(rename = "driveId")]
    pub drive_id: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MicrosoftGraphDrive {
    #[serde(default)]
    quota: Option<MicrosoftGraphQuota>,
}

#[derive(Debug, Deserialize)]
struct MicrosoftGraphUploadSession {
    #[serde(rename = "uploadUrl")]
    upload_url: String,
}

#[derive(Debug, Serialize)]
struct MicrosoftGraphUploadSessionRequest {
    item: MicrosoftGraphUploadSessionItem,
}

#[derive(Debug, Serialize)]
struct MicrosoftGraphUploadSessionItem {
    #[serde(rename = "@microsoft.graph.conflictBehavior")]
    conflict_behavior: &'static str,
}

#[derive(Debug, Deserialize)]
struct MicrosoftGraphQuota {
    #[serde(default)]
    remaining: Option<i64>,
    #[serde(default)]
    total: Option<i64>,
    #[serde(default)]
    used: Option<i64>,
}

impl MicrosoftGraphClientConfig {
    pub fn new(graph_base_url: impl Into<String>, access_token: impl Into<String>) -> Self {
        Self {
            graph_base_url: graph_base_url.into(),
            access_token: access_token.into(),
        }
    }
}

impl MicrosoftGraphClient {
    pub fn new(config: MicrosoftGraphClientConfig) -> Result<Self> {
        if config.graph_base_url.trim().is_empty() {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "Microsoft Graph base URL cannot be empty",
            ));
        }
        if config.access_token.trim().is_empty() {
            return Err(storage_driver_error(
                StorageErrorKind::Auth,
                "Microsoft Graph access token cannot be empty",
            ));
        }
        let http = reqwest::ClientBuilder::new()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(60))
            .user_agent(OUTBOUND_HTTP_USER_AGENT)
            .build()
            .map_aster_err_ctx(
                "failed to build Microsoft Graph HTTP client",
                AsterError::internal_error,
            )?;
        Ok(Self { config, http })
    }

    pub async fn get_drive_item_by_id(
        &self,
        drive_id: &str,
        item_id: &str,
    ) -> Result<MicrosoftGraphDriveItem> {
        self.get_json(
            &format!(
                "/drives/{}/items/{}",
                encode_path_segment(drive_id),
                encode_path_segment(item_id)
            ),
            "get OneDrive item metadata",
        )
        .await
    }

    pub async fn get_drive_item(&self, path: &str) -> Result<MicrosoftGraphDriveItem> {
        self.get_json(path, "get OneDrive item metadata").await
    }

    pub async fn metadata(&self, path: &str) -> Result<BlobMetadata> {
        let item = self.get_drive_item(path).await?;
        let size = item.size.unwrap_or(0);
        if size < 0 {
            return Err(storage_driver_error(
                StorageErrorKind::Unknown,
                "Microsoft Graph returned negative item size",
            ));
        }
        Ok(BlobMetadata {
            size: u64::try_from(size).map_err(|_| {
                storage_driver_error(StorageErrorKind::Unknown, "OneDrive item size overflow")
            })?,
            content_type: None,
        })
    }

    pub async fn put_small_content(&self, content_path: &str, data: &[u8]) -> Result<()> {
        let url = self.url(content_path)?;
        let response = self
            .http
            .put(url)
            .header(AUTHORIZATION, self.authorization_header())
            .header(CONTENT_LENGTH, data.len().to_string())
            .header(CONTENT_TYPE, "application/octet-stream")
            .body(data.to_vec())
            .send()
            .await
            .map_err(|err| map_reqwest_error("put OneDrive small content", err))?;
        self.ensure_success(response, "put OneDrive small content")
            .await?;
        Ok(())
    }

    pub async fn create_upload_session(&self, upload_session_path: &str) -> Result<String> {
        let url = self.url(upload_session_path)?;
        let response = self
            .http
            .post(url)
            .header(AUTHORIZATION, self.authorization_header())
            .json(&MicrosoftGraphUploadSessionRequest {
                item: MicrosoftGraphUploadSessionItem {
                    conflict_behavior: "replace",
                },
            })
            .send()
            .await
            .map_err(|err| map_reqwest_error("create OneDrive upload session", err))?;
        let response = self
            .ensure_success(response, "create OneDrive upload session")
            .await?;
        let session = response
            .json::<MicrosoftGraphUploadSession>()
            .await
            .map_aster_err_ctx(
                "create OneDrive upload session: invalid Microsoft Graph JSON",
                AsterError::storage_driver_error,
            )?;
        if session.upload_url.trim().is_empty() {
            return Err(storage_driver_error(
                StorageErrorKind::Unknown,
                "Microsoft Graph returned empty uploadUrl",
            ));
        }
        Ok(session.upload_url)
    }

    pub async fn upload_session_fragment(
        &self,
        upload_url: &str,
        start: u64,
        total_size: u64,
        data: Vec<u8>,
    ) -> Result<()> {
        if data.is_empty() {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "OneDrive upload session fragment cannot be empty",
            ));
        }
        let len = u64::try_from(data.len()).map_err(|_| {
            storage_driver_error(
                StorageErrorKind::Misconfigured,
                "OneDrive upload session fragment length overflow",
            )
        })?;
        let end = start
            .checked_add(len)
            .and_then(|value| value.checked_sub(1))
            .ok_or_else(|| {
                storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    "OneDrive upload session fragment range overflow",
                )
            })?;
        let content_range = format!("bytes {start}-{end}/{total_size}");
        let url = reqwest::Url::parse(upload_url).map_err(invalid_graph_url)?;
        let response = self
            .http
            .put(url)
            .header(CONTENT_LENGTH, len.to_string())
            .header(CONTENT_TYPE, "application/octet-stream")
            .header("Content-Range", content_range)
            .body(data)
            .send()
            .await
            .map_err(|err| map_reqwest_error("upload OneDrive session fragment", err))?;
        self.ensure_success(response, "upload OneDrive session fragment")
            .await?;
        Ok(())
    }

    pub async fn get_stream(
        &self,
        content_path: &str,
        offset: Option<u64>,
        length: Option<u64>,
    ) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        let url = self.url(content_path)?;
        let mut request = self
            .http
            .get(url)
            .header(AUTHORIZATION, self.authorization_header());
        if let Some(range_header) = range_header(offset, length)? {
            request = request.header(RANGE, range_header);
        }
        let response = request
            .send()
            .await
            .map_err(|err| map_reqwest_error("get OneDrive content stream", err))?;
        self.ensure_success(response, "get OneDrive content stream")
            .await
            .map(|response| {
                let stream = response.bytes_stream().map_err(std::io::Error::other);
                Box::new(StreamReader::new(stream)) as Box<dyn AsyncRead + Unpin + Send>
            })
    }

    pub async fn get_bytes(&self, content_path: &str) -> Result<Vec<u8>> {
        let url = self.url(content_path)?;
        let response = self
            .http
            .get(url)
            .header(AUTHORIZATION, self.authorization_header())
            .send()
            .await
            .map_err(|err| map_reqwest_error("get OneDrive content", err))?;
        let response = self
            .ensure_success(response, "get OneDrive content")
            .await?;
        response
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(|err| map_reqwest_error("read OneDrive content", err))
    }

    pub async fn delete(&self, path: &str) -> Result<()> {
        let url = self.url(path)?;
        let response = self
            .http
            .delete(url)
            .header(AUTHORIZATION, self.authorization_header())
            .send()
            .await
            .map_err(|err| map_reqwest_error("delete OneDrive item", err))?;
        self.ensure_success(response, "delete OneDrive item")
            .await?;
        Ok(())
    }

    pub async fn exists(&self, path: &str) -> Result<bool> {
        match self.get_drive_item(path).await {
            Ok(_) => Ok(true),
            Err(error) if error.storage_error_kind() == Some(StorageErrorKind::NotFound) => {
                Ok(false)
            }
            Err(error) => Err(error),
        }
    }

    pub async fn capacity_info(&self, drive_id: &str) -> Result<StorageCapacityInfo> {
        let drive: MicrosoftGraphDrive = self
            .get_json(
                &format!("/drives/{}", encode_path_segment(drive_id)),
                "get OneDrive drive quota",
            )
            .await?;
        let Some(quota) = drive.quota else {
            return Ok(StorageCapacityInfo {
                status: StorageCapacityStatus::Unavailable,
                total_bytes: None,
                available_bytes: None,
                used_bytes: None,
                source: "microsoft_graph".to_string(),
                observed_at: Utc::now(),
            });
        };
        Ok(StorageCapacityInfo {
            status: StorageCapacityStatus::Supported,
            total_bytes: quota.total,
            available_bytes: quota.remaining,
            used_bytes: quota.used,
            source: "microsoft_graph".to_string(),
            observed_at: Utc::now(),
        })
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, path: &str, ctx: &str) -> Result<T> {
        let url = self.url(path)?;
        let response = self
            .http
            .get(url)
            .header(AUTHORIZATION, self.authorization_header())
            .send()
            .await
            .map_err(|err| map_reqwest_error(ctx, err))?;
        let response = self.ensure_success(response, ctx).await?;
        response.json::<T>().await.map_aster_err_ctx(
            &format!("{ctx}: invalid Microsoft Graph JSON"),
            AsterError::storage_driver_error,
        )
    }

    async fn ensure_success(
        &self,
        response: reqwest::Response,
        ctx: &str,
    ) -> Result<reqwest::Response> {
        if response.status().is_success() {
            return Ok(response);
        }
        Err(map_graph_response_error(ctx, response).await)
    }

    fn authorization_header(&self) -> String {
        format!("Bearer {}", self.config.access_token)
    }

    fn url(&self, path: &str) -> Result<reqwest::Url> {
        let base = self.config.graph_base_url.trim().trim_end_matches('/');
        let path = path.trim();
        let path = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{path}")
        };
        reqwest::Url::parse(&format!("{base}/v1.0{path}")).map_err(invalid_graph_url)
    }
}

fn encode_path_segment(value: &str) -> String {
    percent_encoding::utf8_percent_encode(value.trim(), percent_encoding::NON_ALPHANUMERIC)
        .to_string()
}

fn range_header(offset: Option<u64>, length: Option<u64>) -> Result<Option<String>> {
    let Some(offset) = offset else {
        return Ok(None);
    };
    if let Some(length) = length {
        if length == 0 {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "OneDrive range length cannot be zero",
            ));
        }
        let end = offset
            .checked_add(length)
            .and_then(|value| value.checked_sub(1))
            .ok_or_else(|| {
                storage_driver_error(StorageErrorKind::Misconfigured, "OneDrive range overflow")
            })?;
        Ok(Some(format!("bytes={offset}-{end}")))
    } else {
        Ok(Some(format!("bytes={offset}-")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_header_formats_bounded_and_open_ranges() {
        assert_eq!(range_header(None, None).unwrap(), None);
        assert_eq!(
            range_header(Some(10), Some(20)).unwrap().as_deref(),
            Some("bytes=10-29")
        );
        assert_eq!(
            range_header(Some(10), None).unwrap().as_deref(),
            Some("bytes=10-")
        );
    }

    #[test]
    fn client_rejects_missing_access_token() {
        let error = MicrosoftGraphClient::new(MicrosoftGraphClientConfig::new(
            "https://graph.microsoft.com",
            " ",
        ))
        .unwrap_err();

        assert_eq!(error.storage_error_kind(), Some(StorageErrorKind::Auth));
    }
}

use super::{
    QiniuDriver,
    http::{self, ServiceKind},
    invalid,
};
use aster_drive_storage::traits::extensions::ListStorageDriver;
use aster_drive_storage::{
    BlobMetadata, Result, StorageDriver, StorageDriverExtensions, StorageErrorKind,
    StoragePathVisitControl, StoragePathVisitor, storage_driver_error,
};
use futures::TryStreamExt;
use reqwest::Method;
use serde::Deserialize;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio_util::io::StreamReader;

#[derive(Deserialize)]
struct Stat {
    fsize: u64,
    #[serde(rename = "mimeType")]
    mime_type: Option<String>,
}

impl QiniuDriver {
    pub(crate) async fn head_metadata(&self, path: &str) -> Result<BlobMetadata> {
        let url = self.private_url(
            &self.config.download_origin,
            path,
            self.source_read_deadline()?,
            &[],
        )?;
        let response = http::checked(
            self.control_client
                .head(url)
                .header("accept-encoding", "identity")
                .send()
                .await
                .map_err(http::transport_error)?,
        )
        .await?;
        let size = response
            .headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| invalid("Kodo origin HEAD returned no valid Content-Length"))?;
        Ok(BlobMetadata {
            size,
            content_type: response
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .map(str::to_string),
        })
    }

    async fn read_response(&self, path: &str, range: Option<String>) -> Result<reqwest::Response> {
        let url = self.private_url(
            &self.config.download_origin,
            path,
            self.source_read_deadline()?,
            &[],
        )?;
        let mut request = self
            .download_client
            .get(url)
            .header("accept-encoding", "identity");
        if let Some(range) = &range {
            request = request.header("range", range);
        }
        let response = http::checked(request.send().await.map_err(http::transport_error)?).await?;
        if range.is_some() && response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
            return Err(storage_driver_error(
                StorageErrorKind::Precondition,
                "Kodo origin ignored the requested byte range",
            ));
        }
        Ok(response)
    }

    fn response_reader(response: reqwest::Response) -> Box<dyn AsyncRead + Unpin + Send> {
        let stream = response
            .bytes_stream()
            .map_err(|_| std::io::Error::other("Kodo response stream failed"));
        Box::new(StreamReader::new(stream))
    }
}

#[async_trait::async_trait]
impl StorageDriver for QiniuDriver {
    async fn put(&self, path: &str, data: &[u8]) -> Result<String> {
        self.upload_form(
            path,
            reqwest::Body::from(data.to_vec()),
            u64::try_from(data.len()).map_err(|_| invalid("Kodo object size overflow"))?,
        )
        .await
    }

    async fn get(&self, path: &str) -> Result<Vec<u8>> {
        let mut reader = self.get_stream(path).await?;
        let mut data = Vec::new();
        reader.read_to_end(&mut data).await.map_err(|_| {
            storage_driver_error(StorageErrorKind::Transient, "Kodo response stream failed")
        })?;
        Ok(data)
    }

    async fn get_stream(&self, path: &str) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        Ok(Self::response_reader(self.read_response(path, None).await?))
    }

    async fn get_range(
        &self,
        path: &str,
        offset: u64,
        length: Option<u64>,
    ) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        self.key(path)?;
        if length == Some(0) {
            return Ok(Box::new(tokio::io::empty()));
        }
        let range = match length {
            Some(length) => format!(
                "bytes={offset}-{}",
                offset
                    .checked_add(length - 1)
                    .ok_or_else(|| invalid("Kodo range overflow"))?
            ),
            None => format!("bytes={offset}-"),
        };
        let response = self.read_response(path, Some(range)).await?;
        let valid_range = response
            .headers()
            .get("content-range")
            .and_then(|v| v.to_str().ok())
            .and_then(|value| value.strip_prefix("bytes "))
            .and_then(|value| value.split_once('/'))
            .and_then(|(range, total)| {
                range
                    .split_once('-')
                    .map(|(start, end)| (start, end, total))
            })
            .and_then(|(start, end, total)| {
                Some((
                    start.parse::<u64>().ok()?,
                    end.parse::<u64>().ok()?,
                    total.parse::<u64>().ok()?,
                ))
            })
            .is_some_and(|(start, end, total)| {
                start == offset
                    && end >= start
                    && end < total
                    && length.is_none_or(|length| end - start < length)
                    && response
                        .content_length()
                        .is_none_or(|size| size == end - start + 1)
            });
        if !valid_range {
            return Err(storage_driver_error(
                StorageErrorKind::Precondition,
                "Kodo origin returned an inconsistent Content-Range",
            ));
        }
        let reader = Self::response_reader(response);
        Ok(match length {
            Some(length) => Box::new(reader.take(length)),
            None => reader,
        })
    }

    fn supports_efficient_range(&self) -> bool {
        true
    }

    fn max_single_put_size(&self) -> Option<u64> {
        Some(1024 * 1024 * 1024)
    }

    async fn delete(&self, path: &str) -> Result<()> {
        let mut url = self.endpoint(ServiceKind::Management).await?;
        url.set_path(&format!("/delete/{}", self.entry(path)?));
        match self.management(Method::POST, url).await {
            Ok(_) => Ok(()),
            Err(error) if error.kind() == StorageErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        match self.metadata(path).await {
            Ok(_) => Ok(true),
            Err(error) if error.kind() == StorageErrorKind::NotFound => Ok(false),
            Err(error) => Err(error),
        }
    }

    async fn metadata(&self, path: &str) -> Result<BlobMetadata> {
        let mut url = self.endpoint(ServiceKind::Management).await?;
        url.set_path(&format!("/stat/{}", self.entry(path)?));
        let stat: Stat = http::decode(self.management(Method::GET, url).await?).await?;
        Ok(BlobMetadata {
            size: stat.fsize,
            content_type: stat.mime_type,
        })
    }

    async fn copy_object(&self, src_path: &str, dest_path: &str) -> Result<String> {
        let mut url = self.endpoint(ServiceKind::Management).await?;
        url.set_path(&format!(
            "/copy/{}/{}/force/true",
            self.entry(src_path)?,
            self.entry(dest_path)?
        ));
        self.management(Method::POST, url).await?;
        Ok(dest_path.to_string())
    }

    fn extensions(&self) -> StorageDriverExtensions<'_> {
        StorageDriverExtensions {
            direct_download: Some(self),
            provider_resumable: Some(self),
            list: Some(self),
            stream_upload: Some(self),
            multipart: Some(self),
            ..Default::default()
        }
    }
}

#[derive(Deserialize)]
struct ListResponse {
    #[serde(default)]
    marker: String,
    #[serde(default)]
    items: Vec<ListItem>,
}

#[derive(Deserialize)]
struct ListItem {
    key: String,
}

#[async_trait::async_trait]
impl ListStorageDriver for QiniuDriver {
    async fn list_paths(&self, prefix: Option<&str>) -> Result<Vec<String>> {
        struct Collector(Vec<String>);

        #[async_trait::async_trait]
        impl StoragePathVisitor for Collector {
            async fn visit_path(&mut self, path: String) -> Result<StoragePathVisitControl> {
                self.0.push(path);
                Ok(StoragePathVisitControl::Continue)
            }
        }

        let mut collector = Collector(Vec::new());
        self.scan_paths(prefix, &mut collector).await?;
        collector.0.sort();
        Ok(collector.0)
    }

    async fn scan_paths(
        &self,
        prefix: Option<&str>,
        visitor: &mut dyn StoragePathVisitor,
    ) -> Result<()> {
        let prefix = match prefix {
            Some("") | None => {
                if self.config.base_path.is_empty() {
                    String::new()
                } else {
                    format!("{}/", self.config.base_path)
                }
            }
            Some(prefix) => self.key(prefix)?,
        };
        let mut marker = String::new();
        loop {
            let mut url = self.endpoint(ServiceKind::List).await?;
            url.set_path("/list");
            url.query_pairs_mut()
                .append_pair("bucket", &self.config.bucket)
                .append_pair("prefix", &prefix)
                .append_pair("limit", "1000")
                .append_pair("marker", &marker);
            let response: ListResponse =
                http::decode(self.management(Method::GET, url).await?).await?;
            for item in response.items {
                if let Some(path) = aster_drive_storage::object_key::strip_key_prefix(
                    &self.config.base_path,
                    &item.key,
                ) && !path.is_empty()
                    && matches!(
                        visitor.visit_path(path.to_string()).await?,
                        StoragePathVisitControl::Stop
                    )
                {
                    return Ok(());
                }
            }
            if response.marker.is_empty() {
                return Ok(());
            }
            if response.marker == marker {
                return Err(storage_driver_error(
                    StorageErrorKind::Precondition,
                    "Kodo list cursor did not advance",
                ));
            }
            marker = response.marker;
        }
    }
}

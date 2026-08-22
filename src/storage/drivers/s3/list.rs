use async_trait::async_trait;

use aster_drive_storage::traits::extensions::ListStorageDriver;
use aster_drive_storage::{StorageErrorKind, storage_driver_error};

use super::S3Driver;

// =============================================================================
// ListStorageDriver 扩展
// =============================================================================

#[async_trait]
impl ListStorageDriver for S3Driver {
    async fn list_paths(&self, prefix: Option<&str>) -> aster_drive_storage::Result<Vec<String>> {
        let mut paths = Vec::new();
        self.scan_paths_v2_with(prefix, |path| {
            paths.push(path);
            Ok(())
        })
        .await?;
        paths.sort();
        Ok(paths)
    }

    async fn scan_paths(
        &self,
        prefix: Option<&str>,
        visitor: &mut dyn aster_drive_storage::traits::driver::StoragePathVisitor,
    ) -> aster_drive_storage::Result<()> {
        self.scan_paths_v2_with(prefix, |path| visitor.visit_path(path))
            .await
    }
}

impl S3Driver {
    async fn scan_paths_v2_with(
        &self,
        prefix: Option<&str>,
        mut visit: impl FnMut(String) -> aster_drive_storage::Result<()>,
    ) -> aster_drive_storage::Result<()> {
        let full_prefix = prefix
            .map(|prefix| self.full_key(prefix))
            .unwrap_or_else(|| self.base_path.trim_end_matches('/').to_string());
        let mut continuation: Option<String> = None;

        loop {
            let mut request = self.client.list_objects_v2().bucket(&self.bucket);
            if !full_prefix.is_empty() {
                request = request.prefix(full_prefix.clone());
            }
            if let Some(token) = continuation.as_deref() {
                request = request.continuation_token(token);
            }

            let response = request
                .send()
                .await
                .map_err(|err| Self::map_sdk_error("S3 list_objects_v2 failed", err))?;

            for object in response.contents() {
                let Some(key) = object.key() else {
                    continue;
                };
                if let Some(path) = self.relative_key(key) {
                    visit(path.to_string())?;
                }
            }

            let truncated = response.is_truncated().unwrap_or(false);
            continuation = response.next_continuation_token().map(ToOwned::to_owned);
            if !truncated || continuation.is_none() {
                break;
            }
        }

        Ok(())
    }
}

impl S3Driver {
    /// List paths through the marker-based ListObjects API used by Huawei OBS.
    ///
    /// OBS' native SDK and API contract expose `marker`/`NextMarker`, not S3's
    /// `list-type=2` continuation-token protocol. Keep this provider-specific
    /// pagination out of the generic S3 extension so ordinary S3 drivers retain
    /// ListObjectsV2.
    pub(in crate::storage::drivers) async fn list_paths_v1(
        &self,
        prefix: Option<&str>,
    ) -> aster_drive_storage::Result<Vec<String>> {
        let mut paths = Vec::new();
        self.scan_paths_v1_with(prefix, |path| {
            paths.push(path);
            Ok(())
        })
        .await?;
        paths.sort();
        Ok(paths)
    }

    pub(in crate::storage::drivers) async fn scan_paths_v1(
        &self,
        prefix: Option<&str>,
        visitor: &mut dyn aster_drive_storage::traits::driver::StoragePathVisitor,
    ) -> aster_drive_storage::Result<()> {
        self.scan_paths_v1_with(prefix, |path| visitor.visit_path(path))
            .await
    }

    async fn scan_paths_v1_with(
        &self,
        prefix: Option<&str>,
        mut visit: impl FnMut(String) -> aster_drive_storage::Result<()>,
    ) -> aster_drive_storage::Result<()> {
        let full_prefix = prefix
            .map(|prefix| self.full_key(prefix))
            .unwrap_or_else(|| self.base_path.trim_end_matches('/').to_string());
        let mut marker: Option<String> = None;

        loop {
            let mut request = self.client.list_objects().bucket(&self.bucket);
            if !full_prefix.is_empty() {
                request = request.prefix(full_prefix.clone());
            }
            if let Some(value) = marker.as_deref() {
                request = request.marker(value);
            }

            let response = request
                .send()
                .await
                .map_err(|err| Self::map_sdk_error("OBS list_objects failed", err))?;
            let fallback_marker = response
                .contents()
                .iter()
                .filter_map(|object| object.key())
                .next_back()
                .map(ToOwned::to_owned);

            for object in response.contents() {
                let Some(key) = object.key() else {
                    continue;
                };
                if let Some(path) = self.relative_key(key) {
                    visit(path.to_string())?;
                }
            }

            if !response.is_truncated().unwrap_or(false) {
                break;
            }
            let next_marker = response
                .next_marker()
                .map(ToOwned::to_owned)
                .or(fallback_marker)
                .ok_or_else(|| {
                    storage_driver_error(
                        StorageErrorKind::Transient,
                        "OBS returned a truncated object listing without a next marker",
                    )
                })?;
            if marker.as_deref() == Some(next_marker.as_str()) {
                return Err(storage_driver_error(
                    StorageErrorKind::Transient,
                    "OBS object listing did not advance its next marker",
                ));
            }
            marker = Some(next_marker);
        }

        Ok(())
    }
}

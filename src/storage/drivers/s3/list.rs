use async_trait::async_trait;

use crate::errors::Result;
use crate::storage::traits::extensions::ListStorageDriver;

use super::S3Driver;

// =============================================================================
// ListStorageDriver 扩展
// =============================================================================

#[async_trait]
impl ListStorageDriver for S3Driver {
    async fn list_paths(&self, prefix: Option<&str>) -> Result<Vec<String>> {
        let full_prefix = prefix
            .map(|prefix| self.full_key(prefix))
            .unwrap_or_else(|| self.base_path.trim_end_matches('/').to_string());
        let mut continuation: Option<String> = None;
        let mut paths = Vec::new();

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
                    paths.push(path.to_string());
                }
            }

            let truncated = response.is_truncated().unwrap_or(false);
            continuation = response.next_continuation_token().map(ToOwned::to_owned);
            if !truncated || continuation.is_none() {
                break;
            }
        }

        paths.sort();
        Ok(paths)
    }

    async fn scan_paths(
        &self,
        prefix: Option<&str>,
        visitor: &mut dyn crate::storage::traits::driver::StoragePathVisitor,
    ) -> Result<()> {
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
                    visitor.visit_path(path.to_string())?;
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

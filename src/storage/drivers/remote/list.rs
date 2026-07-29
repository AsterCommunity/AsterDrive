use async_trait::async_trait;

use aster_drive_storage::traits::extensions::ListStorageDriver;

use super::RemoteDriver;

#[async_trait]
impl ListStorageDriver for RemoteDriver {
    async fn list_paths(&self, prefix: Option<&str>) -> aster_drive_storage::Result<Vec<String>> {
        let full_prefix = prefix.map(|value| self.object_key(value));
        let paths = self.client.list_paths(full_prefix.as_deref()).await?;
        Ok(paths
            .into_iter()
            .filter_map(|path| self.strip_base_path(&path).map(str::to_string))
            .collect())
    }
}

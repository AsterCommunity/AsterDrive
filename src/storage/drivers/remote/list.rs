use async_trait::async_trait;

use aster_drive_storage::object_key;
use aster_drive_storage::traits::{
    StoragePathVisitControl, StoragePathVisitor, extensions::ListStorageDriver,
};

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

    async fn scan_paths(
        &self,
        prefix: Option<&str>,
        visitor: &mut dyn StoragePathVisitor,
    ) -> aster_drive_storage::Result<()> {
        let full_prefix = prefix.map(|value| self.object_key(value));
        let mut prefix_visitor = PrefixVisitor {
            base_path: &self.base_path,
            visitor,
        };
        self.client
            .scan_paths(full_prefix.as_deref(), &mut prefix_visitor)
            .await
            .map_err(aster_drive_storage::StorageError::from)
    }
}

struct PrefixVisitor<'a> {
    base_path: &'a str,
    visitor: &'a mut dyn StoragePathVisitor,
}

#[async_trait]
impl StoragePathVisitor for PrefixVisitor<'_> {
    async fn visit_path(
        &mut self,
        path: String,
    ) -> aster_drive_storage::Result<StoragePathVisitControl> {
        if let Some(relative) = object_key::strip_key_prefix(self.base_path, &path) {
            return self.visitor.visit_path(relative.to_string()).await;
        }
        Ok(StoragePathVisitControl::Continue)
    }
}

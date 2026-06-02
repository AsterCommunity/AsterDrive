use async_trait::async_trait;

use crate::errors::{AsterError, MapAsterErr, Result};
use crate::storage::traits::driver::StoragePathVisitor;
use crate::storage::traits::extensions::ListStorageDriver;

use super::LocalDriver;

fn collect_local_paths(
    root: &std::path::Path,
    current: &std::path::Path,
    output: &mut Vec<String>,
) -> std::io::Result<()> {
    if !current.exists() {
        return Ok(());
    }

    for entry in std::fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_local_paths(root, &path, output)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }

        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        output.push(relative);
    }

    Ok(())
}

#[async_trait]
impl ListStorageDriver for LocalDriver {
    async fn list_paths(&self, prefix: Option<&str>) -> Result<Vec<String>> {
        let root = self.base_path.clone();
        let start = match prefix {
            Some(prefix) => self.full_path(prefix)?,
            None => root.clone(),
        };

        tokio::task::spawn_blocking(move || {
            let mut paths = Vec::new();
            collect_local_paths(&root, &start, &mut paths)?;
            paths.sort();
            Ok::<Vec<String>, std::io::Error>(paths)
        })
        .await
        .map_aster_err_ctx("list local paths", AsterError::storage_driver_error)?
        .map_aster_err_ctx("list local paths", AsterError::storage_driver_error)
    }

    async fn scan_paths(
        &self,
        prefix: Option<&str>,
        visitor: &mut dyn StoragePathVisitor,
    ) -> Result<()> {
        let root = self.base_path.clone();
        let start = match prefix {
            Some(prefix) => self.full_path(prefix)?,
            None => root.clone(),
        };
        let metadata = match tokio::fs::metadata(&start).await {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(AsterError::storage_driver_error(format!(
                    "scan local paths metadata: {error}"
                )));
            }
        };

        if metadata.is_file() {
            let relative = start
                .strip_prefix(&root)
                .unwrap_or(&start)
                .to_string_lossy()
                .replace('\\', "/");
            visitor.visit_path(relative)?;
            return Ok(());
        }

        let mut pending_dirs = vec![start];
        while let Some(current_dir) = pending_dirs.pop() {
            let mut entries = tokio::fs::read_dir(&current_dir).await.map_aster_err_ctx(
                "scan local paths read_dir",
                AsterError::storage_driver_error,
            )?;
            let mut child_dirs = Vec::new();
            let mut child_files = Vec::new();

            while let Some(entry) = entries.next_entry().await.map_aster_err_ctx(
                "scan local paths next_entry",
                AsterError::storage_driver_error,
            )? {
                let path = entry.path();
                let file_type = entry.file_type().await.map_aster_err_ctx(
                    "scan local paths file_type",
                    AsterError::storage_driver_error,
                )?;

                if file_type.is_dir() {
                    child_dirs.push(path);
                } else if file_type.is_file() {
                    child_files.push(path);
                }
            }

            child_dirs.sort();
            child_files.sort();

            for file_path in child_files {
                let relative = file_path
                    .strip_prefix(&root)
                    .unwrap_or(&file_path)
                    .to_string_lossy()
                    .replace('\\', "/");
                visitor.visit_path(relative)?;
            }

            for child_dir in child_dirs.into_iter().rev() {
                pending_dirs.push(child_dir);
            }
        }

        Ok(())
    }
}

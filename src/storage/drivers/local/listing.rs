use async_trait::async_trait;

use aster_drive_storage::traits::driver::StoragePathVisitor;
use aster_drive_storage::traits::extensions::ListStorageDriver;
use aster_drive_storage::{MapStorageErr, StorageErrorKind, storage_driver_error};

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
    async fn list_paths(&self, prefix: Option<&str>) -> aster_drive_storage::Result<Vec<String>> {
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
        .map_storage_err_ctx(StorageErrorKind::Transient, "list local paths")?
        .map_storage_err_ctx(StorageErrorKind::Transient, "list local paths")
    }

    async fn scan_paths(
        &self,
        prefix: Option<&str>,
        visitor: &mut dyn StoragePathVisitor,
    ) -> aster_drive_storage::Result<()> {
        let root = self.base_path.clone();
        let start = match prefix {
            Some(prefix) => self.full_path(prefix)?,
            None => root.clone(),
        };
        let metadata = match tokio::fs::metadata(&start).await {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(storage_driver_error(
                    StorageErrorKind::Transient,
                    format!("scan local paths metadata: {error}"),
                ));
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
            let mut entries = tokio::fs::read_dir(&current_dir)
                .await
                .map_storage_err_ctx(StorageErrorKind::Transient, "scan local paths read_dir")?;
            let mut child_dirs = Vec::new();
            let mut child_files = Vec::new();

            while let Some(entry) = entries
                .next_entry()
                .await
                .map_storage_err_ctx(StorageErrorKind::Transient, "scan local paths next_entry")?
            {
                let path = entry.path();
                let file_type = entry.file_type().await.map_storage_err_ctx(
                    StorageErrorKind::Transient,
                    "scan local paths file_type",
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

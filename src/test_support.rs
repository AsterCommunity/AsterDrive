use std::{collections::BTreeSet, path::Path};

pub(crate) fn snapshot_dir_tree(path: &Path) -> std::io::Result<BTreeSet<String>> {
    fn walk(root: &Path, current: &Path, entries: &mut BTreeSet<String>) -> std::io::Result<()> {
        for entry in std::fs::read_dir(current)? {
            let entry = entry?;
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            if entry.file_type()?.is_dir() {
                entries.insert(format!("{relative}/"));
                walk(root, &path, entries)?;
            } else {
                entries.insert(relative);
            }
        }
        Ok(())
    }

    let mut entries = BTreeSet::new();
    if path.exists() {
        walk(path, path, &mut entries)?;
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dir(label: &str) -> (PathBuf, aster_forge_utils::raii::TempDirGuard) {
        let path = std::env::temp_dir().join(format!(
            "asterdrive-snapshot-dir-tree-{label}-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&path).expect("snapshot test directory should be created");
        let guard = aster_forge_utils::raii::TempDirGuard::new(
            path.clone(),
            "directory snapshot helper test",
        );
        (path, guard)
    }

    #[test]
    fn missing_directory_has_empty_snapshot() {
        let (root, _guard) = temp_dir("missing");

        assert!(snapshot_dir_tree(&root.join("missing")).unwrap().is_empty());
    }

    #[test]
    fn empty_directory_has_empty_snapshot() {
        let (root, _guard) = temp_dir("empty");

        assert!(snapshot_dir_tree(&root).unwrap().is_empty());
    }

    #[test]
    fn snapshot_normalizes_nested_files_and_marks_directories() {
        let (root, _guard) = temp_dir("nested");
        std::fs::create_dir_all(root.join("nested/empty"))
            .expect("nested directories should be created");
        std::fs::write(root.join("root.txt"), b"root").expect("root file should be written");
        std::fs::write(root.join("nested/file.txt"), b"nested")
            .expect("nested file should be written");

        assert_eq!(
            snapshot_dir_tree(&root).unwrap(),
            BTreeSet::from([
                "nested/".to_string(),
                "nested/empty/".to_string(),
                "nested/file.txt".to_string(),
                "root.txt".to_string(),
            ])
        );
    }

    #[test]
    fn file_root_preserves_read_dir_error() {
        let (root, _guard) = temp_dir("file-root");
        let file = root.join("file.txt");
        std::fs::write(&file, b"file").expect("file root should be written");

        assert!(snapshot_dir_tree(&file).is_err());
    }
}

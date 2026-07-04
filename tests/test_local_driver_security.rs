#[cfg(unix)]
mod tests {
    use aster_drive::entities::storage_policy;
    use aster_drive::storage::StorageDriver;
    use aster_drive::storage::drivers::local::{LocalDriver, upload_staging_path};
    use aster_drive::types::{
        DriverType, StoredStoragePolicyAllowedTypes, StoredStoragePolicyOptions,
    };
    use std::path::{Path, PathBuf};

    fn build_policy(base: &Path) -> storage_policy::Model {
        storage_policy::Model {
            id: 1,
            name: "local".into(),
            driver_type: DriverType::Local,
            endpoint: String::new(),
            bucket: String::new(),
            access_key: String::new(),
            secret_key: String::new(),
            base_path: base.to_string_lossy().into_owned(),
            remote_node_id: None,
            remote_storage_target_key: None,
            max_file_size: 0,
            allowed_types: StoredStoragePolicyAllowedTypes::empty(),
            options: StoredStoragePolicyOptions::empty(),
            is_default: false,
            chunk_size: 0,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    fn temp_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "asterdrive-{name}-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ))
    }

    #[tokio::test]
    async fn local_driver_rejects_symlink_escape_on_put() {
        let temp_root = temp_root("local-driver-symlink-put");
        let base = temp_root.join("storage");
        let outside = temp_root.join("outside");
        std::fs::create_dir_all(&base).expect("storage root should exist");
        std::fs::create_dir_all(&outside).expect("outside dir should exist");
        std::os::unix::fs::symlink(&outside, base.join("escape"))
            .expect("symlink escape should be created");

        let driver = LocalDriver::new(&build_policy(&base)).expect("driver should initialize");
        let result = driver.put("escape/pwned.txt", b"nope").await;

        assert!(result.is_err());
        assert!(!outside.join("pwned.txt").exists());

        let _ = std::fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn local_staging_path_rejects_symlink_escape() {
        let temp_root = temp_root("local-driver-staging-symlink");
        let base = temp_root.join("storage");
        let outside = temp_root.join("outside");
        std::fs::create_dir_all(&base).expect("storage root should exist");
        std::fs::create_dir_all(&outside).expect("outside dir should exist");
        std::os::unix::fs::symlink(&outside, base.join(".staging"))
            .expect("staging symlink should be created");

        let result = upload_staging_path(&build_policy(&base), "token.upload");
        assert!(result.is_err());

        let _ = std::fs::remove_dir_all(&temp_root);
    }
}

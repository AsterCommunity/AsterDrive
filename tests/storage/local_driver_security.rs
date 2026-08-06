#[cfg(unix)]
mod tests {
    use aster_drive::storage::drivers::local::{LocalDriver, upload_staging_path};
    use aster_drive_storage::StorageDriver;
    use std::path::PathBuf;

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

        let driver = LocalDriver::new(&base.to_string_lossy()).expect("driver should initialize");
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

        let result = upload_staging_path(&base.to_string_lossy(), "token.upload");
        assert!(result.is_err());

        let _ = std::fs::remove_dir_all(&temp_root);
    }
}

//! CLI node integration contracts.
use super::*;

#[test]
fn node_inputs_override_environment_and_fail_before_bootstrap_when_incomplete() {
    use std::io::Write;
    use std::process::Stdio;
    let root = aster_forge_utils::raii::TempDirGuard::new(
        std::env::temp_dir().join(format!("aster-node-inputs-{}", uuid::Uuid::new_v4())),
        "CLI enrollment input fixture",
    );
    std::fs::create_dir_all(root.path()).unwrap();
    for args in [
        vec!["node", "enroll"],
        vec!["node", "enroll", "https://example.invalid"],
    ] {
        let output = Command::new(aster_drive_bin())
            .args(args)
            .current_dir(root.path())
            .env_remove("ASTER_CLI_MASTER_URL")
            .env_remove("ASTER_CLI_ENROLLMENT_TOKEN")
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(!root.path().join("data").exists());
    }
    std::fs::create_dir(root.path().join("data")).unwrap();
    let config = "[server]\nstart_mode = \"primary\"\n";
    std::fs::write(root.path().join("data/config.toml"), config).unwrap();
    let mut child = Command::new(aster_drive_bin())
        .args([
            "node",
            "enroll",
            "https://example.invalid",
            "--token-stdin",
            "--yes",
        ])
        .env("ASTER_CLI_MASTER_URL", "invalid environment URL")
        .env(
            "ASTER_CLI_ENROLLMENT_TOKEN",
            "environment-credential-marker",
        )
        .current_dir(root.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"stdin-credential-marker\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("start_mode"), "{stderr}");
    assert!(!stderr.contains("credential-marker"));
    assert!(output.stdout.is_empty());
    // Accepted credentials reach the existing startup path, which may fill
    // missing signing keys before rejecting primary mode. It must not enroll.
    assert!(
        std::fs::read_to_string(root.path().join("data/config.toml"))
            .unwrap()
            .starts_with(config)
    );
    assert_eq!(
        std::fs::read_dir(root.path().join("data")).unwrap().count(),
        1
    );
}

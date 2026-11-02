//! CLI input and redaction integration contracts.
use super::*;

#[tokio::test]
async fn config_missing_terminal_input_and_invalid_clear_never_write() {
    let database_url = setup_database_url().await;
    let before = run_aster_drive(&[
        "config",
        "--database-url",
        &database_url,
        "export",
        "--include-sensitive",
    ]);
    for action in [
        vec!["set"],
        vec!["set", "branding_title"],
        vec!["set", "mail_smtp_password"],
        vec!["set", "branding_title", "--value-prompt"],
        vec!["set", "auth_cookie_secure", "--clear"],
        vec!["validate", "branding_title"],
    ] {
        let args: Vec<_> = ["config", "--database-url", &database_url]
            .into_iter()
            .chain(action)
            .collect();
        let output = run_aster_drive(&args);
        assert!(!output.status.success(), "{output:?}");
        assert!(output.stdout.is_empty());
    }
    let after = run_aster_drive(&[
        "config",
        "--database-url",
        &database_url,
        "export",
        "--include-sensitive",
    ]);
    assert_eq!(before.stdout, after.stdout);
}

#[cfg(unix)]
#[tokio::test]
async fn cli_terminal_input_visibility_retry_keep_cancel_selection_and_hashing() {
    let database_url = setup_database_url().await;
    let root = aster_forge_utils::raii::TempDirGuard::new(
        std::env::temp_dir().join(format!("aster-cli-interactive-{}", uuid::Uuid::new_v4())),
        "CLI configuration interaction fixture",
    );
    std::fs::create_dir_all(root.path().join("data")).unwrap();
    std::fs::write(
        root.path().join("data/config.toml"),
        "[server]\nstart_mode = \"primary\"\n",
    )
    .unwrap();
    let output = Command::new("python3")
        .args([
            "-c",
            include_str!("../cli_interaction.py"),
            aster_drive_bin(),
            &database_url,
        ])
        .arg(root.path())
        .output()
        .expect("Python 3 PTY test support must be available on Unix");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let hashes: Vec<(String, String)> = serde_json::from_slice(&output.stdout)
        .expect("PTY harness must return verifier cases as JSON");
    assert!(!hashes.is_empty(), "PTY harness returned no verifier cases");
    for (password, hash) in hashes {
        assert!(
            aster_forge_crypto::verify_password(&password, &hash).unwrap(),
            "interactive verifier did not preserve the entered credential"
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn config_backup_round_trips_values_and_redacts_secrets_by_default() {
    use std::os::unix::fs::PermissionsExt;

    let source = setup_database_url().await;
    let target_without_secrets = setup_database_url().await;
    let target_with_secrets = setup_database_url().await;
    for (key, value) in [
        ("custom.backup_roundtrip", "line one\nline two"),
        ("mail_smtp_password", "backup-secret-marker"),
    ] {
        let output = run_aster_drive(&[
            "config",
            "--database-url",
            &source,
            "set",
            "--key",
            key,
            "--value",
            value,
        ]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        if key == "mail_smtp_password" {
            assert!(!String::from_utf8_lossy(&output.stdout).contains(value));
            assert!(!String::from_utf8_lossy(&output.stderr).contains(value));
        }
    }

    let sanitized = run_aster_drive(&["config", "--database-url", &source, "export"]);
    assert!(sanitized.status.success());
    let backup: Value = serde_json::from_slice(&sanitized.stdout).unwrap();
    assert_eq!(backup["format"], "asterdrive.runtime-config");
    assert_eq!(backup["version"], 1);
    assert!(
        backup["omitted_sensitive_keys"]
            .as_array()
            .unwrap()
            .iter()
            .any(|key| key == "mail_smtp_password")
    );
    assert!(!String::from_utf8_lossy(&sanitized.stdout).contains("backup-secret-marker"));

    let dir = aster_forge_utils::raii::TempDirGuard::new(
        std::env::temp_dir().join(format!("aster-cli-backup-{}", uuid::Uuid::new_v4())),
        "config backup fixture",
    );
    std::fs::create_dir_all(dir.path()).unwrap();
    let sanitized_path = dir.path().join("sanitized.json");
    std::fs::write(&sanitized_path, &sanitized.stdout).unwrap();
    std::fs::set_permissions(&sanitized_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let imported = run_aster_drive(&[
        "config",
        "--database-url",
        &target_without_secrets,
        "import",
        "--input-file",
        sanitized_path.to_str().unwrap(),
    ]);
    assert!(
        imported.status.success(),
        "{}",
        String::from_utf8_lossy(&imported.stderr)
    );
    assert!(!String::from_utf8_lossy(&imported.stdout).contains("backup-secret-marker"));
    let value = run_aster_drive(&[
        "config",
        "--database-url",
        &target_without_secrets,
        "get",
        "--key",
        "custom.backup_roundtrip",
    ]);
    let value: Value = serde_json::from_slice(&value.stdout).unwrap();
    assert_eq!(value["data"]["value"], "line one\nline two");

    let complete = run_aster_drive(&[
        "config",
        "--database-url",
        &source,
        "export",
        "--include-sensitive",
    ]);
    assert!(String::from_utf8_lossy(&complete.stdout).contains("backup-secret-marker"));
    let complete_path = dir.path().join("complete.json");
    std::fs::write(&complete_path, &complete.stdout).unwrap();
    std::fs::set_permissions(&complete_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let imported = run_aster_drive(&[
        "config",
        "--database-url",
        &target_with_secrets,
        "import",
        "--input-file",
        complete_path.to_str().unwrap(),
    ]);
    assert!(
        imported.status.success(),
        "{}",
        String::from_utf8_lossy(&imported.stderr)
    );
    assert!(!String::from_utf8_lossy(&imported.stdout).contains("backup-secret-marker"));
    let restored = run_aster_drive(&[
        "config",
        "--database-url",
        &target_with_secrets,
        "export",
        "--include-sensitive",
    ]);
    let before: Value = serde_json::from_slice(&complete.stdout).unwrap();
    let after: Value = serde_json::from_slice(&restored.stdout).unwrap();
    let before = before["configs"].as_array().unwrap();
    let after = after["configs"].as_array().unwrap();
    assert_eq!(before.len(), after.len());
    for (original, restored) in before.iter().zip(after) {
        assert_eq!(original["key"], restored["key"]);
        if original["key"] == "media_processing_registry_json" {
            // Processor extension order is a set; import canonicalizes that
            // order. Every other registry property must survive unchanged.
            let mut first: Value =
                serde_json::from_str(original["value"].as_str().unwrap()).unwrap();
            let mut second: Value =
                serde_json::from_str(restored["value"].as_str().unwrap()).unwrap();
            for registry in [&mut first, &mut second] {
                for processor in registry["processors"].as_array_mut().unwrap() {
                    if let Some(extensions) = processor
                        .get_mut("extensions")
                        .and_then(Value::as_array_mut)
                    {
                        extensions.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
                    }
                }
            }
            assert!(
                first == second,
                "processor configuration changed during round trip"
            );
        } else {
            assert!(
                original == restored,
                "configuration changed during round trip: {}",
                original["key"]
            );
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn config_secret_file_input_requires_private_permissions_and_never_echoes_value() {
    use std::os::unix::fs::PermissionsExt;

    let database_url = setup_database_url().await;
    let dir = aster_forge_utils::raii::TempDirGuard::new(
        std::env::temp_dir().join(format!("aster-cli-secret-input-{}", uuid::Uuid::new_v4())),
        "secret config input fixture",
    );
    std::fs::create_dir_all(dir.path()).unwrap();
    let secret = "secret-file-marker";
    let private_path = dir.path().join("private");
    std::fs::write(&private_path, secret).unwrap();
    std::fs::set_permissions(&private_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let output = run_aster_drive(&[
        "config",
        "--database-url",
        &database_url,
        "set",
        "--key",
        "mail_smtp_password",
        "--value-file",
        private_path.to_str().unwrap(),
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains(secret));
    assert!(!String::from_utf8_lossy(&output.stderr).contains(secret));

    let public_path = dir.path().join("public");
    std::fs::write(&public_path, secret).unwrap();
    std::fs::set_permissions(&public_path, std::fs::Permissions::from_mode(0o644)).unwrap();
    let output = run_aster_drive(&[
        "config",
        "--database-url",
        &database_url,
        "set",
        "--key",
        "mail_smtp_password",
        "--value-file",
        public_path.to_str().unwrap(),
    ]);
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).contains(secret));
    assert!(!String::from_utf8_lossy(&output.stderr).contains(secret));
}

#[test]
fn hash_password_reads_stdin_offline_and_rejects_invalid_input_without_echoing_it() {
    use std::io::Write;
    use std::process::Stdio;
    let dir = aster_forge_utils::raii::TempDirGuard::new(
        std::env::temp_dir().join(format!("aster-hash-cli-{}", uuid::Uuid::new_v4())),
        "hash CLI test",
    );
    std::fs::create_dir_all(dir.path()).unwrap();
    // An invalid server configuration proves this command never starts server
    // resources or logging, and never generates or repairs configuration files.
    std::fs::create_dir(dir.path().join("data")).unwrap();
    let invalid_config = "not valid TOML";
    std::fs::write(dir.path().join("data/config.toml"), invalid_config).unwrap();
    let run = |input: &[u8]| {
        let mut child = Command::new(aster_drive_bin())
            .arg("hash-password")
            .current_dir(dir.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(input).unwrap();
        child.wait_with_output().unwrap()
    };
    for (password, ending) in [
        ("scrape:secret".to_owned(), "\r\n"),
        ("x".repeat(4096), "\r\n"),
        ("  scrape:secret  ".to_owned(), ""),
        ("scrape:secret".to_owned(), "\n"),
        ("scrape:secret".to_owned(), "\r"),
    ] {
        let output = run(format!("{password}{ending}").as_bytes());
        assert!(
            output.status.success(),
            "hash command should succeed without exposing its input"
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert_eq!(stdout.lines().count(), 1);
        assert!(stdout.starts_with("$argon2id$"));
        assert!(aster_forge_crypto::verify_password(&password, stdout.trim()).unwrap());
        assert!(output.stderr.is_empty());
    }
    for password in [
        Vec::new(),
        b"\n".to_vec(),
        b"credential-marker\nsecond-line".to_vec(),
        b"credential-marker\t".to_vec(),
        b"credential-marker\0".to_vec(),
        vec![b'x'; 4097],
        vec![0xff],
    ] {
        let output = run(&password);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("credential-marker"));
    }
    assert_eq!(
        std::fs::read_to_string(dir.path().join("data/config.toml")).unwrap(),
        invalid_config
    );
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[tokio::test]
async fn cli_secret_stdin_modes_are_bounded_mutually_exclusive_and_redacted() {
    use std::io::Write;
    use std::process::Stdio;
    let database_url = setup_database_url().await;
    let run = |input: &[u8]| {
        let mut child = Command::new(aster_drive_bin())
            .args([
                "config",
                "--database-url",
                &database_url,
                "set",
                "--key",
                "mail_smtp_password",
                "--value-stdin",
                "--yes",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let written = child.stdin.take().unwrap().write_all(input);
        if let Err(error) = written {
            assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe);
        }
        child.wait_with_output().unwrap()
    };
    let output = run(b"stdin-secret-marker\r\n");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("stdin-secret-marker"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("stdin-secret-marker"));
    let complete = run_aster_drive(&[
        "config",
        "--database-url",
        &database_url,
        "export",
        "--include-sensitive",
    ]);
    let backup: Value = serde_json::from_slice(&complete.stdout).unwrap();
    let stored = backup["configs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["key"] == "mail_smtp_password")
        .unwrap();
    assert_eq!(stored["value"], "stdin-secret-marker");
    for bad_input in [vec![0xff], vec![b'x'; 1024 * 1024 + 1]] {
        let output = run(&bad_input);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("stdin-secret-marker"));
    }
    for args in [
        vec![
            "config",
            "--database-url",
            &database_url,
            "set",
            "--key",
            "mail_smtp_password",
            "--value",
            "conflict-secret-marker",
            "--value-stdin",
        ],
        vec![
            "node",
            "enroll",
            "--master-url",
            "https://example.invalid",
            "--token",
            "conflict-secret-marker",
            "--token-stdin",
        ],
    ] {
        let output = run_aster_drive(&args);
        assert!(!output.status.success());
        assert!(!String::from_utf8_lossy(&output.stdout).contains("conflict-secret-marker"));
        assert!(!String::from_utf8_lossy(&output.stderr).contains("conflict-secret-marker"));
    }
    for (args, variable) in [
        (vec!["config", "set", "--help"], "ASTER_CLI_CONFIG_VALUE"),
        (
            vec!["node", "enroll", "--help"],
            "ASTER_CLI_ENROLLMENT_TOKEN",
        ),
    ] {
        let output = run_aster_drive_with_env(&args, &[(variable, "environment-secret-marker")]);
        assert!(output.status.success());
        assert!(!String::from_utf8_lossy(&output.stdout).contains("environment-secret-marker"));
        assert!(!String::from_utf8_lossy(&output.stderr).contains("environment-secret-marker"));
    }
}

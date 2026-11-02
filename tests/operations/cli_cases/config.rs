//! CLI config integration contracts.
use super::*;

#[tokio::test]
async fn test_root_binary_config_set_and_get_round_trip() {
    let database_url = setup_database_url().await;

    let set_output = run_aster_drive(&[
        "config",
        "--database-url",
        &database_url,
        "set",
        "--key",
        "public_site_url",
        "--value",
        r#"[" HTTPS://Drive.EXAMPLE.com/ "]"#,
    ]);
    assert!(
        set_output.status.success(),
        "set stderr: {}",
        String::from_utf8_lossy(&set_output.stderr)
    );
    let set_json: Value = serde_json::from_slice(&set_output.stdout).expect("set output json");
    assert!(
        set_output.stderr.is_empty(),
        "structured config success stderr should be empty: {}",
        String::from_utf8_lossy(&set_output.stderr)
    );
    assert_eq!(set_json["ok"], true);
    assert_eq!(
        set_json["data"]["value"],
        serde_json::json!(["https://drive.example.com"])
    );

    let get_output = run_aster_drive(&[
        "config",
        "--database-url",
        &database_url,
        "get",
        "--key",
        "public_site_url",
    ]);
    assert!(
        get_output.status.success(),
        "get stderr: {}",
        String::from_utf8_lossy(&get_output.stderr)
    );
    let get_json: Value = serde_json::from_slice(&get_output.stdout).expect("get output json");
    assert_eq!(get_json["ok"], true);
    assert_eq!(get_json["data"]["key"], "public_site_url");
    assert_eq!(
        get_json["data"]["value"],
        serde_json::json!(["https://drive.example.com"])
    );
}

#[tokio::test]
async fn config_dry_run_shows_change_without_writing_or_publishing_reload() {
    let database_url = setup_database_url().await;
    let preview = run_aster_drive(&[
        "config",
        "--database-url",
        &database_url,
        "set",
        "custom.dry_run",
        "planned",
        "--dry-run",
    ]);
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    let preview: Value = serde_json::from_slice(&preview.stdout).unwrap();
    assert_eq!(preview["data"]["changes"][0]["action"], "would_add");
    assert_eq!(preview["data"]["changes"][0]["key"], "custom.dry_run");

    let missing = run_aster_drive(&[
        "config",
        "--database-url",
        &database_url,
        "get",
        "custom.dry_run",
    ]);
    assert!(!missing.status.success());

    let write = run_aster_drive(&[
        "config",
        "--database-url",
        &database_url,
        "set",
        "custom.dry_run",
        "planned",
    ]);
    assert!(
        write.status.success(),
        "{}",
        String::from_utf8_lossy(&write.stderr)
    );
    let preview_delete = run_aster_drive(&[
        "config",
        "--database-url",
        &database_url,
        "delete",
        "custom.dry_run",
        "--dry-run",
    ]);
    assert!(preview_delete.status.success());
    let preview_delete: Value = serde_json::from_slice(&preview_delete.stdout).unwrap();
    assert_eq!(
        preview_delete["data"]["changes"][0]["action"],
        "would_delete"
    );
    let still_present = run_aster_drive(&[
        "config",
        "--database-url",
        &database_url,
        "get",
        "custom.dry_run",
    ]);
    assert!(still_present.status.success());
}

#[tokio::test]
async fn config_write_requires_explicit_confirmation_without_a_terminal() {
    let database_url = setup_database_url().await;
    let output = Command::new(aster_drive_bin())
        .args([
            "config",
            "--database-url",
            &database_url,
            "set",
            "custom.confirmation",
            "value",
        ])
        .env_remove("ASTER_CLI_YES")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("pass --yes"));
    let missing = run_aster_drive(&[
        "config",
        "--database-url",
        &database_url,
        "get",
        "custom.confirmation",
    ]);
    assert!(!missing.status.success());
}

#[tokio::test]
async fn config_import_dry_run_previews_batch_without_importing_it() {
    let database_url = setup_database_url().await;
    let directory = aster_forge_utils::raii::TempDirGuard::new(
        std::env::temp_dir().join(format!("aster-cli-import-preview-{}", uuid::Uuid::new_v4())),
        "CLI import preview fixture",
    );
    std::fs::create_dir_all(directory.path()).unwrap();
    let input = directory.path().join("config.json");
    std::fs::write(
        &input,
        r#"{"format":"asterdrive.runtime-config","version":1,"configs":[{"key":"custom.import_preview","value":"planned","source":"custom","visibility":"private","is_sensitive":false}],"omitted_sensitive_keys":[]}"#,
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&input, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let preview = run_aster_drive(&[
        "config",
        "--database-url",
        &database_url,
        "import",
        "--input-file",
        input.to_str().unwrap(),
        "--dry-run",
    ]);
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    let preview: Value = serde_json::from_slice(&preview.stdout).unwrap();
    assert_eq!(preview["data"]["changes"][0]["action"], "would_add");
    assert_eq!(
        preview["data"]["changes"][0]["key"],
        "custom.import_preview"
    );
    let missing = run_aster_drive(&[
        "config",
        "--database-url",
        &database_url,
        "get",
        "custom.import_preview",
    ]);
    assert!(!missing.status.success());
}

#[tokio::test]
async fn config_positional_arguments_override_environment_and_keep_legacy_flags() {
    let database_url = setup_database_url().await;
    let environment = [
        ("ASTER_CLI_CONFIG_KEY", "branding_description"),
        ("ASTER_CLI_CONFIG_VALUE", "environment fallback"),
        ("ASTER_CLI_INPUT_FILE", "/missing-environment-backup.json"),
    ];
    for (arguments, expected) in [
        (
            vec!["set", "branding_title", "Positional title"],
            "Positional title",
        ),
        (
            vec!["set", "--key", "branding_title", "--value", "Flag title"],
            "Flag title",
        ),
        (vec!["set", "branding_title"], "environment fallback"),
        (vec!["set", "branding_title", ""], ""),
        (vec!["set", "branding_title", "--clear"], ""),
    ] {
        let args: Vec<_> = ["config", "--database-url", &database_url]
            .into_iter()
            .chain(arguments)
            .collect();
        let output = run_aster_drive_with_env(&args, &environment);
        assert!(output.status.success(), "{output:?}");
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["data"]["key"], "branding_title");
        assert_eq!(result["data"]["value"], expected);
    }
    let output = run_aster_drive_with_env(
        &["config", "--database-url", &database_url, "set"],
        &environment,
    );
    assert!(output.status.success(), "{output:?}");
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["data"]["key"], "branding_description");
    assert_eq!(result["data"]["value"], "environment fallback");
    for action in ["get", "validate"] {
        let mut args = vec![
            "config",
            "--database-url",
            &database_url,
            action,
            "branding_title",
        ];
        if action == "validate" {
            args.push("Preview title");
        }
        let output = run_aster_drive_with_env(&args, &environment);
        assert!(output.status.success(), "{output:?}");
        assert!(output.stderr.is_empty());
    }
    let backup = run_aster_drive(&["config", "--database-url", &database_url, "export"]);
    let directory = aster_forge_utils::raii::TempDirGuard::new(
        std::env::temp_dir().join(format!("aster-cli-backup-input-{}", uuid::Uuid::new_v4())),
        "CLI explicit backup source fixture",
    );
    std::fs::create_dir_all(directory.path()).unwrap();
    let file = directory.path().join("backup.json");
    std::fs::write(&file, backup.stdout).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let output = run_aster_drive_with_env(
        &[
            "config",
            "--database-url",
            &database_url,
            "validate",
            "--input-file",
            file.to_str().unwrap(),
        ],
        &environment,
    );
    assert!(output.status.success(), "{output:?}");
    for args in [
        vec!["set", "custom.positional", "value"],
        vec!["delete", "custom.positional"],
    ] {
        let args: Vec<_> = ["config", "--database-url", &database_url]
            .into_iter()
            .chain(args)
            .collect();
        assert!(run_aster_drive(&args).status.success());
    }
}

#[tokio::test]
async fn test_root_binary_config_get_human_output_is_readable() {
    let database_url = setup_database_url().await;

    let set_output = run_aster_drive(&[
        "config",
        "--database-url",
        &database_url,
        "set",
        "--key",
        "public_site_url",
        "--value",
        r#"[" HTTPS://Drive.EXAMPLE.com/ "]"#,
    ]);
    assert!(
        set_output.status.success(),
        "set stderr: {}",
        String::from_utf8_lossy(&set_output.stderr)
    );

    let output = run_aster_drive(&[
        "config",
        "--database-url",
        &database_url,
        "--output-format",
        "human",
        "get",
        "--key",
        "public_site_url",
    ]);
    assert!(
        output.status.success(),
        "get stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("human stdout should be utf-8");
    assert!(stdout.contains("Configuration value"));
    assert!(stdout.contains("Key:"));
    assert!(stdout.contains("Value:"));
    assert!(stdout.contains("Source:"));
    assert!(stdout.contains("public_site_url"));
    assert!(stdout.contains("https://drive.example.com"));
    assert!(stdout.contains("[system]"));
    assert!(serde_json::from_str::<Value>(&stdout).is_err());
}

#[tokio::test]
async fn test_root_binary_config_list_human_summarizes_multiline_and_masks_sensitive() {
    let database_url = setup_database_url().await;

    let set_output = run_aster_drive(&[
        "config",
        "--database-url",
        &database_url,
        "set",
        "--key",
        "mail_smtp_password",
        "--value",
        "super-secret-password",
    ]);
    assert!(
        set_output.status.success(),
        "set stderr: {}",
        String::from_utf8_lossy(&set_output.stderr)
    );

    let output = run_aster_drive(&[
        "config",
        "--database-url",
        &database_url,
        "--output-format",
        "human",
        "list",
    ]);
    assert!(
        output.status.success(),
        "list stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("human list stdout should be utf-8");
    assert!(stdout.contains("mail_smtp_password"));
    assert!(stdout.contains("[hidden sensitive value]"));
    assert!(!stdout.contains("super-secret-password"));
    assert!(stdout.contains("mail_template_register_activation_html"));
    assert!(stdout.contains("<html template:"));
    assert!(!stdout.contains("mail_template_register_activation_html = <!doctype html>"));
    assert!(stdout.contains("frontend_preview_apps_json"));
    assert!(stdout.contains("<json value:"));
    assert!(!stdout.contains("frontend_preview_apps_json = {"));
}

#[cfg(unix)]
#[tokio::test]
async fn config_import_rejects_invalid_batch_without_partial_writes() {
    use std::os::unix::fs::PermissionsExt;

    let database_url = setup_database_url().await;
    let dir = aster_forge_utils::raii::TempDirGuard::new(
        std::env::temp_dir().join(format!("aster-cli-invalid-backup-{}", uuid::Uuid::new_v4())),
        "invalid config backup fixture",
    );
    std::fs::create_dir_all(dir.path()).unwrap();
    let path = dir.path().join("invalid.json");
    std::fs::write(&path, r#"{"format":"asterdrive.runtime-config","version":1,"configs":[{"key":"custom.must_not_write","value":"first","source":"custom","visibility":"private","is_sensitive":false},{"key":"auth_cookie_secure","value":"yes","source":"system","visibility":"private","is_sensitive":false}],"omitted_sensitive_keys":[]}"#).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let output = run_aster_drive(&[
        "config",
        "--database-url",
        &database_url,
        "import",
        "--input-file",
        path.to_str().unwrap(),
    ]);
    assert!(!output.status.success());
    let get = run_aster_drive(&[
        "config",
        "--database-url",
        &database_url,
        "get",
        "--key",
        "custom.must_not_write",
    ]);
    assert!(!get.status.success());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("first"));

    let legacy_path = dir.path().join("legacy.json");
    std::fs::write(
        &legacy_path,
        r#"[{"key":"custom.legacy","value":"ignored"}]"#,
    )
    .unwrap();
    std::fs::set_permissions(&legacy_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let legacy = run_aster_drive(&[
        "config",
        "--database-url",
        &database_url,
        "import",
        "--input-file",
        legacy_path.to_str().unwrap(),
    ]);
    assert!(!legacy.status.success());
}

#[tokio::test]
async fn cli_database_target_precedence_is_argument_then_cli_env_then_instance_config() {
    let configured_database = setup_database_url().await;
    let environment_database = setup_database_url().await;
    let argument_database = setup_database_url().await;
    for (database, value) in [
        (&configured_database, "config-file"),
        (&environment_database, "cli-env"),
        (&argument_database, "argument"),
    ] {
        let output = run_aster_drive(&[
            "config",
            "--database-url",
            database,
            "set",
            "--key",
            "custom.target_precedence",
            "--value",
            value,
        ]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let data_dir =
        std::env::temp_dir().join(format!("aster-cli-config-target-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::write(
        data_dir.join("config.toml"),
        format!("[database]\nurl = \"{configured_database}\"\n"),
    )
    .unwrap();
    let data_arg = data_dir.to_str().unwrap();

    let get = |database: Option<&str>, env: &[(&str, &str)]| {
        let mut args = vec!["--data-dir", data_arg, "config"];
        if let Some(database) = database {
            args.extend(["--database-url", database]);
        }
        args.extend(["get", "--key", "custom.target_precedence"]);
        run_aster_drive_with_env(&args, env)
    };
    let from_config = get(None, &[]);
    assert_eq!(
        serde_json::from_slice::<Value>(&from_config.stdout).unwrap()["data"]["value"],
        "config-file"
    );
    let from_environment = get(None, &[("ASTER_CLI_DATABASE_URL", &environment_database)]);
    assert_eq!(
        serde_json::from_slice::<Value>(&from_environment.stdout).unwrap()["data"]["value"],
        "cli-env"
    );
    let from_argument = get(
        Some(&argument_database),
        &[("ASTER_CLI_DATABASE_URL", &environment_database)],
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&from_argument.stdout).unwrap()["data"]["value"],
        "argument"
    );

    std::fs::remove_dir_all(data_dir).unwrap();
}

#[tokio::test]
async fn test_root_binary_config_human_output_supports_forced_color() {
    let database_url = setup_database_url().await;

    let output = run_aster_drive_with_env(
        &[
            "config",
            "--database-url",
            &database_url,
            "--output-format",
            "human",
            "list",
        ],
        &[("CLICOLOR_FORCE", "1")],
    );
    assert!(
        output.status.success(),
        "list stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("forced-color stdout should be utf-8");
    assert!(stdout.contains("\u{1b}["));
    assert!(stdout.contains("Configuration list"));
}

#[tokio::test]
async fn test_root_binary_config_delete_rejects_system_config_key() {
    let database_url = setup_database_url().await;

    let output = run_aster_drive(&[
        "config",
        "--database-url",
        &database_url,
        "delete",
        "--key",
        "public_site_url",
    ]);
    assert!(
        !output.status.success(),
        "delete should fail for system config"
    );

    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf-8");
    let err_json: Value = serde_json::from_str(&stderr).expect("error output json");
    assert_eq!(err_json["ok"], false);
    assert_eq!(err_json["error"]["code"], "E013");
    assert!(
        err_json["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("cannot delete system configuration")
    );
}

#[tokio::test]
async fn test_root_binary_config_json_error_does_not_create_default_config() {
    let database_url = setup_database_url().await;
    let current_dir = std::env::temp_dir().join(format!(
        "asterdrive-cli-config-clean-json-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&current_dir).unwrap();

    let output = run_aster_drive_with_env_in_dir(
        &[
            "config",
            "--database-url",
            &database_url,
            "--output-format",
            "json",
            "delete",
            "--key",
            "public_site_url",
        ],
        &[],
        &current_dir,
    );

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let err_json: Value =
        serde_json::from_slice(&output.stderr).expect("stderr should be one JSON document");
    assert_eq!(err_json["ok"], false);
    assert_eq!(err_json["error"]["code"], "E013");
    assert!(!current_dir.join("data/config.toml").exists());
    assert!(!current_dir.join("data/asterdrive.db").exists());

    std::fs::remove_dir_all(current_dir).unwrap();
}

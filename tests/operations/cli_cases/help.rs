//! CLI help integration contracts.
use super::*;

#[test]
fn role_help_and_invalid_arguments_have_no_bootstrap_side_effects() {
    let dir = std::env::temp_dir().join(format!("asterdrive-role-cli-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    for (args, succeeds) in [
        (vec!["serve", "--help"], true),
        (vec!["--help"], true),
        (vec!["--version"], true),
        (vec!["serve", "invalid"], false),
        (vec!["serve", "api", "worker"], false),
    ] {
        let output = run_aster_drive_with_env_in_dir(&args, &[], &dir);
        assert_eq!(output.status.success(), succeeds, "{output:?}");
        assert!(
            !dir.join("data").exists(),
            "CLI parsing must precede configuration/database initialization"
        );
    }
    std::fs::remove_dir(&dir).unwrap();
}

#[test]
fn test_root_binary_help_lists_config_subcommand() {
    let output = run_aster_drive(&["--help"]);
    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("help stdout should be utf-8");
    assert_eq!(
        stdout.lines().next(),
        Some(concat!("AsterDrive ", env!("CARGO_PKG_VERSION")))
    );
    assert!(stdout.contains(env!("CARGO_PKG_DESCRIPTION")));
    assert!(stdout.contains("serve"));
    assert!(stdout.contains("Run the server with all, API, or worker responsibilities"));
    assert!(stdout.contains("config"));
    assert!(stdout.contains("View and manage runtime settings"));
    assert!(stdout.contains("doctor"));
    assert!(stdout.contains("Check instance health and configuration"));
    assert!(stdout.contains("database-migrate"));
    assert!(stdout.contains("Migrate data between database backends"));
}

#[test]
fn test_root_binary_config_help_lists_runtime_config_commands() {
    let output = run_aster_drive(&["config", "--help"]);
    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("config help stdout should be utf-8");
    for command in [
        "list", "get", "set", "delete", "validate", "export", "import",
    ] {
        assert!(
            stdout.contains(command),
            "config help should mention '{command}', got: {stdout}"
        );
    }
}

#[tokio::test]
async fn test_root_binary_serve_help_is_available() {
    let output = run_aster_drive(&["serve", "--help"]);
    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("serve help stdout should be utf-8");
    assert!(stdout.contains("Run the server with all, API, or worker responsibilities"));
}

#[tokio::test]
async fn test_root_binary_database_migrate_help_is_available() {
    let output = run_aster_drive(&["database-migrate", "--help"]);
    assert!(output.status.success());

    let stdout =
        String::from_utf8(output.stdout).expect("database-migrate help stdout should be utf-8");
    assert!(stdout.contains("Migrate data between database backends"));
    assert!(stdout.contains("--source-database-url"));
    assert!(stdout.contains("--target-database-url"));
    assert!(stdout.contains("--dry-run"));
    assert!(stdout.contains("--verify-only"));
}

#[tokio::test]
async fn test_root_binary_doctor_help_is_available() {
    let output = run_aster_drive(&["doctor", "--help"]);
    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("doctor help stdout should be utf-8");
    assert!(stdout.contains("Check instance health and configuration"));
    assert!(stdout.contains("--database-url"));
    assert!(stdout.contains("--output-format"));
    assert!(stdout.contains("--strict"));
    assert!(stdout.contains("--deep"));
    assert!(stdout.contains("--fix"));
    assert!(stdout.contains("--scope"));
    assert!(stdout.contains("--policy-id"));
}

#[test]
fn cli_help_banner_includes_version_at_every_command_depth_without_startup() {
    let root = aster_forge_utils::raii::TempDirGuard::new(
        std::env::temp_dir().join(format!("aster-cli-help-banner-{}", uuid::Uuid::new_v4())),
        "CLI help fixture",
    );
    std::fs::create_dir_all(root.path()).unwrap();
    for args in [
        vec!["-h"],
        vec!["--help"],
        vec!["serve", "--help"],
        vec!["doctor", "--help"],
        vec!["config", "--help"],
        vec!["config", "export", "--help"],
        vec!["node", "enroll", "--help"],
        vec!["database-migrate", "--help"],
    ] {
        let output = run_aster_drive_with_env_in_dir(&args, &[], root.path());
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let help = String::from_utf8(output.stdout).unwrap();
        assert_eq!(
            help.lines().next(),
            Some(concat!("AsterDrive ", env!("CARGO_PKG_VERSION")))
        );
        assert!(!help.contains("system_config"));
        assert!(!help.contains("[env:"));
        assert!(!root.path().join("data").exists());
    }
    let output = run_aster_drive_with_env_in_dir(&["--version"], &[], root.path());
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn root_help_is_compact_hides_environment_values_and_orders_common_commands_first() {
    for args in [vec!["-h"], vec!["--help"]] {
        let output = run_aster_drive_with_env(
            &args,
            &[
                ("ASTER_CLI_CONFIG_FILE", "/secret-path-marker"),
                ("ASTER_CLI_CONFIG_VALUE", "credential-marker"),
            ],
        );
        assert!(output.status.success());
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.starts_with(concat!(
            "AsterDrive ",
            env!("CARGO_PKG_VERSION"),
            "\n",
            env!("CARGO_PKG_DESCRIPTION"),
            "\n"
        )));
        assert!(
            !text.contains("[env:")
                && !text.contains("secret-path-marker")
                && !text.contains("credential-marker")
        );
        assert!(!text.contains("\n\n\n"));
        assert!(
            text.lines().count() <= 32,
            "root help is not compact: {text}"
        );
        let commands = [
            "  serve ",
            "  config ",
            "  doctor ",
            "  node ",
            "  database-migrate ",
            "  hash-password ",
            "  version ",
        ];
        let positions: Vec<usize> = commands
            .iter()
            .map(|command| text.find(command).unwrap())
            .collect();
        assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(text.contains("--config-file <PATH>") && text.contains("--data-dir <PATH>"));
        assert!(
            text.contains("Examples:")
                && text.contains("Running without a command starts all server roles.")
        );
    }
}

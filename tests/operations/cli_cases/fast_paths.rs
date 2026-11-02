//! CLI fast paths integration contracts.
use super::*;

#[test]
fn cli_instance_location_works_from_another_directory_and_missing_instance_is_side_effect_free() {
    let root = aster_forge_utils::raii::TempDirGuard::new(
        std::env::temp_dir().join(format!("aster-cli-instance-{}", uuid::Uuid::new_v4())),
        "CLI instance location fixture",
    );
    std::fs::create_dir_all(root.path()).unwrap();
    let cwd = root.path().join("elsewhere");
    let data = root.path().join("instance-data");
    std::fs::create_dir_all(&cwd).unwrap();
    std::fs::create_dir_all(&data).unwrap();
    let output = run_aster_drive_with_env_in_dir(&["config", "list"], &[], &cwd);
    assert!(!output.status.success());
    assert!(!cwd.join("data").exists());

    let missing = run_aster_drive_with_env_in_dir(
        &["--data-dir", data.to_str().unwrap(), "config", "list"],
        &[],
        &cwd,
    );
    assert!(!missing.status.success());
    assert!(!data.join("config.toml").exists());
    assert!(!data.join("asterdrive.db").exists());
}

#[tokio::test]
async fn cli_config_location_resolves_relative_database_and_service_environment_from_other_cwd() {
    let root = aster_forge_utils::raii::TempDirGuard::new(
        std::env::temp_dir().join(format!("aster-cli-paths-{}", uuid::Uuid::new_v4())),
        "CLI relative paths fixture",
    );
    let data = root.path().join("custom-data");
    let cwd = root.path().join("elsewhere");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::create_dir_all(&cwd).unwrap();
    let seeded = setup_database_url().await;
    let seeded_path = seeded
        .strip_prefix("sqlite://")
        .unwrap()
        .split('?')
        .next()
        .unwrap();
    std::fs::copy(seeded_path, data.join("instance.db")).unwrap();
    let config_path = data.join("config.toml");
    let config_content = "[database]\nurl = \"sqlite://instance.db?mode=rwc\"\n";
    std::fs::write(&config_path, config_content).unwrap();
    for args in [
        vec![
            "--config-file",
            config_path.to_str().unwrap(),
            "config",
            "get",
            "--key",
            "auth_cookie_secure",
        ],
        vec![
            "config",
            "--data-dir",
            data.to_str().unwrap(),
            "get",
            "--key",
            "auth_cookie_secure",
        ],
    ] {
        let output = run_aster_drive_with_env_in_dir(&args, &[], &cwd);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!cwd.join("data").exists());
        assert_eq!(
            std::fs::read_to_string(&config_path).unwrap(),
            config_content
        );
    }
    let env = run_aster_drive_with_env_in_dir(
        &["config", "get", "--key", "auth_cookie_secure"],
        &[
            ("ASTER_CLI_CONFIG_FILE", config_path.to_str().unwrap()),
            ("ASTER__DATABASE__URL", &seeded),
        ],
        &cwd,
    );
    assert!(
        env.status.success(),
        "{}",
        String::from_utf8_lossy(&env.stderr)
    );
    let doctor = run_aster_drive_with_env_in_dir(
        &["--data-dir", data.to_str().unwrap(), "doctor"],
        &[],
        &cwd,
    );
    let report: Value = serde_json::from_slice(&doctor.stdout).unwrap();
    assert!(
        report["data"]["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|check| check["name"] == "database_connection" && check["status"] == "ok")
    );
    let missing_database = data.join("missing.db");
    std::fs::write(
        &config_path,
        "[database]\nurl = \"sqlite://missing.db?mode=rwc\"\n",
    )
    .unwrap();
    let output = run_aster_drive_with_env_in_dir(
        &["--data-dir", data.to_str().unwrap(), "config", "list"],
        &[],
        &cwd,
    );
    assert!(!output.status.success());
    assert!(!missing_database.exists());
}

#[tokio::test]
async fn offline_config_and_doctor_ignore_uncompiled_exporters() {
    let database_url = setup_database_url().await;
    let root = aster_forge_utils::raii::TempDirGuard::new(
        std::env::temp_dir().join(format!(
            "aster-cli-exporter-boundary-{}",
            uuid::Uuid::new_v4()
        )),
        "offline exporter validation boundary",
    );
    std::fs::create_dir_all(root.path()).unwrap();
    let path = root.path().join("config.toml");
    for section in ["prometheus", "otlp_metrics"] {
        let content = format!(
            "[database]\nurl = \"{database_url}\"\n[observability.{section}]\nenabled = true\n"
        );
        std::fs::write(&path, &content).unwrap();
        for action in [
            vec!["list"],
            vec!["get", "--key", "auth_cookie_secure"],
            vec!["export"],
            vec!["validate", "--key", "custom.preview", "--value", "preview"],
            vec!["set", "--key", "custom.offline", "--value", "updated"],
        ] {
            let mut args = vec!["config", "--config-file", path.to_str().unwrap()];
            args.extend(action);
            let output = run_aster_drive_with_env_in_dir(&args, &[], root.path());
            assert!(
                output.status.success(),
                "offline {section} config: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(output.stderr.is_empty());
        }
        assert_eq!(std::fs::read_to_string(&path).unwrap(), content);
        assert!(!root.path().join("data").exists());
        let output = run_aster_drive_with_env_in_dir(
            &["doctor", "--config-file", path.to_str().unwrap()],
            &[],
            root.path(),
        );
        let report: Value = serde_json::from_slice(&output.stdout)
            .expect("doctor must produce its diagnostic report");
        assert!(
            report["data"]["checks"]
                .as_array()
                .unwrap()
                .iter()
                .any(|check| check["name"] == "static_config" && check["status"] == "ok")
        );
        assert!(
            report["data"]["checks"]
                .as_array()
                .unwrap()
                .iter()
                .any(|check| check["name"] == "database_connection" && check["status"] == "ok")
        );
    }
}

#[cfg(all(unix, any(not(feature = "metrics"), not(feature = "otlp-metrics"))))]
#[tokio::test]
async fn server_starts_and_keeps_metrics_route_absent_for_uncompiled_exporters() {
    use aster_forge_test::process::{TestProcess, available_loopback_port};
    use std::time::Duration;
    for mode in ["primary", "follower"] {
        let port = available_loopback_port();
        let mut command = Command::new(aster_drive_bin());
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("ASTER_") {
                command.env_remove(key);
            }
        }
        command
            .env("ASTER__SERVER__START_MODE", mode)
            .env("ASTER__SERVER__HOST", "127.0.0.1")
            .env("ASTER__SERVER__PORT", port.to_string())
            .env("ASTER__SERVER__WORKERS", "1")
            .env("ASTER__DATABASE__URL", "sqlite://asterdrive.db?mode=rwc")
            .env("ASTER__DATABASE__POOL_SIZE", "2")
            .env("ASTER__AUTH__BOOTSTRAP_INSECURE_COOKIES", "true");
        if !cfg!(feature = "metrics") {
            command
                .env("ASTER__OBSERVABILITY__PROMETHEUS__ENABLED", "true")
                .env(
                    "ASTER__OBSERVABILITY__PROMETHEUS__BASIC_AUTH__ENABLED",
                    "true",
                )
                .env(
                    "ASTER__OBSERVABILITY__PROMETHEUS__BASIC_AUTH__USERNAME",
                    "invalid:username",
                )
                .env(
                    "ASTER__OBSERVABILITY__PROMETHEUS__BASIC_AUTH__PASSWORD_HASH",
                    "invalid-secret-marker",
                );
        }
        if !cfg!(feature = "otlp-metrics") {
            command
                .env("ASTER__OBSERVABILITY__OTLP_METRICS__ENABLED", "true")
                .env(
                    "ASTER__OBSERVABILITY__OTLP_METRICS__ENDPOINT",
                    "invalid://invalid-secret-marker",
                )
                .env("ASTER__OBSERVABILITY__OTLP_METRICS__INTERVAL_SECONDS", "0");
        }
        let mut process = TestProcess::spawn(&format!("ignored-exporter-{mode}"), &mut command);
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                process.assert_running();
                if client
                    .get(format!("http://127.0.0.1:{port}/health"))
                    .send()
                    .await
                    .is_ok_and(|response| response.status().is_success())
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .unwrap_or_else(|error| panic!("server did not start: {error}\n{}", process.diagnostics()));
        let response = client
            .get(format!("http://127.0.0.1:{port}/health/metrics"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);
        assert!(
            process.terminate_gracefully(Duration::from_secs(20)),
            "{}",
            process.diagnostics()
        );
        assert!(!process.diagnostics().contains("invalid-secret-marker"));
    }
}

#[test]
fn version_output_is_structured_and_does_not_load_instance_configuration() {
    let root = aster_forge_utils::raii::TempDirGuard::new(
        std::env::temp_dir().join(format!("aster-cli-version-{}", uuid::Uuid::new_v4())),
        "CLI version fixture",
    );
    std::fs::create_dir_all(root.path().join("data")).unwrap();
    let path = root.path().join("data/config.toml");
    std::fs::write(&path, "invalid TOML credential-marker").unwrap();
    let json_output = run_aster_drive_with_env_in_dir(&["version", "--json"], &[], root.path());
    assert!(
        json_output.status.success(),
        "{}",
        String::from_utf8_lossy(&json_output.stderr)
    );
    assert!(json_output.stderr.is_empty());
    let identity: Value = serde_json::from_slice(&json_output.stdout).unwrap();
    assert_eq!(identity["product"], "AsterDrive");
    assert_eq!(identity["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(identity["revision"], env!("ASTER_BUILD_REVISION"));
    assert_eq!(identity["target"], env!("ASTER_BUILD_TARGET"));
    assert_eq!(identity["profile"], env!("ASTER_BUILD_PROFILE"));
    assert_eq!(identity["build_time"], env!("ASTER_BUILD_TIME"));
    assert!(identity["dirty"].is_boolean() || identity["dirty"].is_null());
    for (feature, enabled) in [
        ("metrics", cfg!(feature = "metrics")),
        ("otlp-metrics", cfg!(feature = "otlp-metrics")),
        ("operations-cli", cfg!(feature = "operations-cli")),
        ("server", cfg!(feature = "server")),
        ("openapi", cfg!(feature = "openapi")),
    ] {
        assert_eq!(
            identity["features"]
                .as_array()
                .unwrap()
                .iter()
                .any(|value| value == feature),
            enabled
        );
    }
    let human = run_aster_drive_with_env_in_dir(&["version"], &[], root.path());
    assert!(human.status.success());
    let human = String::from_utf8(human.stdout).unwrap();
    for label in ["Revision:", "Target:", "Profile:", "Features:", "Built:"] {
        assert!(human.contains(label));
    }
    for args in [vec!["-V"], vec!["--version"]] {
        let output = run_aster_drive_with_env_in_dir(&args, &[], root.path());
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            concat!("AsterDrive ", env!("CARGO_PKG_VERSION"), "\n")
        );
        assert!(output.stderr.is_empty());
    }
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "invalid TOML credential-marker"
    );
    assert_eq!(
        std::fs::read_dir(root.path().join("data")).unwrap().count(),
        1
    );
}

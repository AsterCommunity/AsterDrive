//! Service commands must also work in builds without operational CLI commands.

use std::process::Command;

fn command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_aster_drive"));
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("ASTER_") {
            command.env_remove(key);
        }
    }
    command
}

#[test]
fn service_help_and_invalid_roles_never_bootstrap_an_instance() {
    let dir = aster_forge_utils::raii::TempDirGuard::new(
        std::env::temp_dir().join(format!("aster-service-help-{}", uuid::Uuid::new_v4())),
        "service command parser fixture",
    );
    std::fs::create_dir_all(dir.path()).unwrap();
    for args in [vec!["--help"], vec!["serve", "--help"]] {
        let output = command()
            .current_dir(dir.path())
            .args(&args)
            .output()
            .unwrap();
        assert!(output.status.success());
        let help = String::from_utf8(output.stdout).unwrap();
        assert!(help.starts_with(concat!("AsterDrive ", env!("CARGO_PKG_VERSION"))));
        if args[0] == "serve" {
            assert!(help.contains("all") && help.contains("api") && help.contains("worker"));
        } else {
            assert_eq!(help.contains("  config "), cfg!(feature = "operations-cli"));
            assert_eq!(help.contains("  doctor "), cfg!(feature = "operations-cli"));
            assert_eq!(help.contains("  node "), cfg!(feature = "operations-cli"));
        }
    }
    for args in [vec!["serve", "follower"], vec!["serve", "api", "worker"]] {
        let output = command()
            .current_dir(dir.path())
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
    }
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[cfg(unix)]
#[tokio::test]
async fn service_roles_start_and_worker_does_not_expose_product_routes() {
    use aster_forge_test::process::{TestProcess, available_loopback_port};
    use std::time::Duration;

    for role in ["all", "api", "worker"] {
        let port = available_loopback_port();
        let mut command = command();
        command
            .args(["--data-dir", "instance", "serve", role])
            .env("ASTER__SERVER__HOST", "127.0.0.1")
            .env("ASTER__SERVER__PORT", port.to_string())
            .env("ASTER__SERVER__WORKERS", "1")
            .env("ASTER__DATABASE__POOL_SIZE", "2")
            .env("ASTER__AUTH__BOOTSTRAP_INSECURE_COOKIES", "true");
        let mut process = TestProcess::spawn(&format!("service-role-{role}"), &mut command);
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
        .unwrap_or_else(|error| panic!("{role} did not start: {error}\n{}", process.diagnostics()));

        let response = client
            .get(format!("http://127.0.0.1:{port}/api/v1/auth/login"))
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status() == reqwest::StatusCode::NOT_FOUND,
            role == "worker"
        );
        assert!(process.runtime_dir().join("instance/config.toml").is_file());
        assert!(
            process
                .runtime_dir()
                .join("instance/asterdrive.db")
                .is_file()
        );
        assert!(!process.runtime_dir().join("data").exists());
        assert!(
            process.terminate_gracefully(Duration::from_secs(20)),
            "{}",
            process.diagnostics()
        );
    }
}

//! Exercise the unmodified executable with Windows' default main-thread stack.
#![cfg(all(windows, feature = "server"))]

use std::{fs::File, process::Stdio, time::Duration};

#[tokio::test]
async fn fresh_install_and_restart_use_default_windows_stack() {
    let directory = aster_forge_utils::raii::TempDirGuard::new(
        std::env::temp_dir().join(format!("aster-windows-startup-{}", uuid::Uuid::new_v4())),
        "Windows startup regression",
    );
    std::fs::create_dir_all(directory.path()).unwrap();
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();

    for attempt in 0..2 {
        let log_path = directory.path().join(format!("startup-{attempt}.log"));
        let log = File::create(&log_path).unwrap();
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_aster_drive"));
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("ASTER_") {
                command.env_remove(key);
            }
        }
        let mut child = command
            .current_dir(directory.path())
            .env_remove("RUST_MIN_STACK")
            .env("ASTER__SERVER__HOST", "127.0.0.1")
            .env("ASTER__SERVER__PORT", port.to_string())
            .env("ASTER__SERVER__WORKERS", "1")
            .env("ASTER__AUTH__BOOTSTRAP_INSECURE_COOKIES", "true")
            .stdout(Stdio::from(log.try_clone().unwrap()))
            .stderr(Stdio::from(log))
            .kill_on_drop(true)
            .spawn()
            .unwrap();

        let ready = tokio::time::timeout(Duration::from_secs(60), async {
            loop {
                if let Some(status) = child.try_wait().unwrap() {
                    panic!(
                        "server exited with {status}: {}",
                        std::fs::read_to_string(&log_path).unwrap()
                    );
                }
                if client
                    .get(format!("http://127.0.0.1:{port}/health/ready"))
                    .send()
                    .await
                    .is_ok_and(|response| response.status().is_success())
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await;
        child.kill().await.unwrap();
        ready.unwrap_or_else(|error| {
            panic!(
                "server readiness timed out: {error}: {}",
                std::fs::read_to_string(&log_path).unwrap()
            )
        });
    }
}

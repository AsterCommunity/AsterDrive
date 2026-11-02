//! CLI completions integration contracts.
use super::*;

#[test]
fn completion_install_generates_shell_scripts_without_instance_bootstrap() {
    let root = aster_forge_utils::raii::TempDirGuard::new(
        std::env::temp_dir().join(format!("aster-cli-completions-{}", uuid::Uuid::new_v4())),
        "CLI shell completion fixture",
    );
    std::fs::create_dir_all(root.path()).unwrap();
    for (shell, relative_path, expected) in [
        (
            "bash",
            ".local/share/bash-completion/completions/aster_drive",
            "complete -F",
        ),
        (
            "zsh",
            ".local/share/zsh/site-functions/_aster_drive",
            "_arguments",
        ),
        (
            "fish",
            ".config/fish/completions/aster_drive.fish",
            "complete -c aster_drive",
        ),
    ] {
        let output = Command::new(aster_drive_bin())
            .args(["completions", "install", shell])
            .env("HOME", root.path())
            .env_remove("XDG_DATA_HOME")
            .env_remove("XDG_CONFIG_HOME")
            .current_dir(root.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let path = root.path().join(relative_path);
        let script = std::fs::read_to_string(path).unwrap();
        assert!(script.contains(expected), "{shell}: {script}");
        assert!(script.contains("database-migrate"));
        assert!(script.contains("completions"));
        if shell == "fish" {
            assert!(script.contains("-a 'all api worker'"));
            assert!(script.contains("-a 'bash zsh fish'"));
        }
        assert!(!root.path().join("data").exists());
        if shell != "fish" {
            match Command::new(shell)
                .arg("-n")
                .arg(root.path().join(relative_path))
                .output()
            {
                Ok(syntax) => assert!(
                    syntax.status.success(),
                    "{}",
                    String::from_utf8_lossy(&syntax.stderr)
                ),
                Err(error) if shell == "zsh" && error.kind() == std::io::ErrorKind::NotFound => {
                    eprintln!("Skipping zsh syntax check: interpreter is not installed");
                }
                Err(error) => panic!("failed to run {shell} syntax check: {error}"),
            }
        }
    }
    let inferred = Command::new(aster_drive_bin())
        .args(["completions", "install"])
        .env("HOME", root.path())
        .env("SHELL", "/usr/bin/fish")
        .env_remove("XDG_DATA_HOME")
        .env_remove("XDG_CONFIG_HOME")
        .current_dir(root.path())
        .output()
        .unwrap();
    assert!(
        inferred.status.success(),
        "{}",
        String::from_utf8_lossy(&inferred.stderr)
    );
    assert!(
        root.path()
            .join(".config/fish/completions/aster_drive.fish")
            .exists()
    );
}

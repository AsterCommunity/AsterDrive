//! Completion generation and user-level shell installation.
use clap::{Subcommand, ValueEnum};
use clap_complete::Shell;
use std::path::{Path, PathBuf};
#[derive(Debug, Clone, Subcommand)]
pub(in crate::cli) enum CompletionCommand {
    /// Install the generated completion script for a shell
    Install {
        /// Shell to install for; defaults to the current SHELL environment variable
        #[arg(value_enum)]
        shell: Option<CompletionShell>,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(in crate::cli) enum CompletionShell {
    Bash,
    Zsh,
    Fish,
}

impl CompletionShell {
    pub(in crate::cli) fn clap_shell(self) -> Shell {
        match self {
            Self::Bash => Shell::Bash,
            Self::Zsh => Shell::Zsh,
            Self::Fish => Shell::Fish,
        }
    }
}

pub(in crate::cli) fn shell_from_environment() -> Option<CompletionShell> {
    let shell = std::env::var_os("SHELL")?;
    match Path::new(&shell).file_name()?.to_str()? {
        "bash" => Some(CompletionShell::Bash),
        "zsh" => Some(CompletionShell::Zsh),
        "fish" => Some(CompletionShell::Fish),
        _ => None,
    }
}

pub(in crate::cli) fn shell_name(shell: Shell) -> &'static str {
    match shell {
        Shell::Bash => "bash",
        Shell::Zsh => "zsh",
        Shell::Fish => "fish",
        _ => "unsupported",
    }
}

pub(in crate::cli) fn install_completions(
    shell: Shell,
    command: &mut clap::Command,
) -> std::io::Result<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "HOME is not set; cannot install completion",
        )
    })?;
    let path = completion_path(
        shell,
        &home,
        std::env::var_os("XDG_DATA_HOME").as_deref(),
        std::env::var_os("XDG_CONFIG_HOME").as_deref(),
    );
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut contents = Vec::new();
    clap_complete::generate(shell, command, "aster_drive", &mut contents);
    if shell == Shell::Fish {
        add_fish_positional_values(command, &[], &mut contents);
    }
    std::fs::write(&path, contents)?;
    Ok(path)
}

pub(in crate::cli) fn add_fish_positional_values(
    command: &clap::Command,
    path: &[&str],
    output: &mut Vec<u8>,
) {
    if let Some((root, nested)) = path.split_first() {
        for argument in command.get_positionals() {
            let values = argument
                .get_possible_values()
                .into_iter()
                .filter(|value| !value.is_hide_set())
                .map(|value| value.get_name().to_owned())
                .collect::<Vec<_>>();
            if values.is_empty() {
                continue;
            }
            let mut condition = format!("__fish_aster_drive_using_subcommand {root}");
            for subcommand in nested {
                condition.push_str(&format!("; and __fish_seen_subcommand_from {subcommand}"));
            }
            let values = values.join(" ");
            condition.push_str(&format!("; and not __fish_seen_subcommand_from {values}"));
            output.extend_from_slice(
                format!("complete -c aster_drive -n '{condition}' -f -a '{values}'\n").as_bytes(),
            );
        }
    }
    for child in command.get_subcommands() {
        let mut child_path = path.to_vec();
        child_path.push(child.get_name());
        add_fish_positional_values(child, &child_path, output);
    }
}

fn completion_path(
    shell: Shell,
    home: &Path,
    xdg_data_home: Option<&std::ffi::OsStr>,
    xdg_config_home: Option<&std::ffi::OsStr>,
) -> PathBuf {
    let data_home = xdg_data_home
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/share"));
    match shell {
        Shell::Bash => data_home.join("bash-completion/completions/aster_drive"),
        Shell::Zsh => data_home.join("zsh/site-functions/_aster_drive"),
        Shell::Fish => xdg_config_home
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"))
            .join("fish/completions/aster_drive.fish"),
        _ => home.join(".local/share/unsupported/aster_drive"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completion_install_paths_follow_shell_and_xdg_directories() {
        let home = Path::new("/home/tester");
        assert_eq!(
            completion_path(Shell::Bash, home, None, None),
            Path::new("/home/tester/.local/share/bash-completion/completions/aster_drive")
        );
        assert_eq!(
            completion_path(Shell::Zsh, home, Some(std::ffi::OsStr::new("/data")), None),
            Path::new("/data/zsh/site-functions/_aster_drive")
        );
        assert_eq!(
            completion_path(
                Shell::Fish,
                home,
                None,
                Some(std::ffi::OsStr::new("/config"))
            ),
            Path::new("/config/fish/completions/aster_drive.fish")
        );
    }
}

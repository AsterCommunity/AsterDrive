//! Root command parsing, help policy and operational command dispatch.

use clap::{CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;
use std::io;
use std::path::{Path, PathBuf};

#[cfg(feature = "operations-cli")]
use super::{CliError, config, context::CliContext, database_migration, doctor, node, shared};

use super::{build_info::render_build_info, help};

#[derive(Debug, Parser)]
#[command(
    name = "aster_drive",
    display_name = "AsterDrive",
    version,
    propagate_version = true,
    about = env!("CARGO_PKG_DESCRIPTION"),
    after_help = if cfg!(feature = "operations-cli") { "Examples:\n  aster_drive serve\n  aster_drive --data-dir /srv/asterdrive/data doctor\n  aster_drive config --help\n\nRunning without a command starts all server roles." } else { "Examples:\n  aster_drive serve\n  aster_drive --data-dir /srv/asterdrive/data serve worker\n\nRunning without a command starts all server roles." },
    styles = help::cli_styles()
)]
struct RootCli {
    /// Instance configuration file
    #[arg(
        long,
        global = true,
        env = "ASTER_CLI_CONFIG_FILE",
        value_name = "PATH",
        conflicts_with = "data_dir"
    )]
    config_file: Option<std::path::PathBuf>,
    /// Instance data directory
    #[arg(
        long,
        global = true,
        env = "ASTER_CLI_DATA_DIR",
        value_name = "PATH",
        conflicts_with = "config_file"
    )]
    data_dir: Option<std::path::PathBuf>,
    #[command(subcommand)]
    command: Option<RootCommand>,
}

#[derive(Debug, Clone, Subcommand)]
enum RootCommand {
    /// Run the server with all, API, or worker responsibilities
    Serve {
        /// Process responsibilities; Follower node mode is still selected by config
        #[arg(value_enum, default_value = "all")]
        role: crate::runtime::role::ServeRole,
    },
    #[cfg(feature = "operations-cli")]
    /// View and manage runtime settings
    Config {
        #[arg(long, env = "ASTER_CLI_DATABASE_URL")]
        database_url: Option<String>,
        #[arg(long, env = "ASTER_CLI_OUTPUT_FORMAT", default_value = "auto")]
        output_format: shared::OutputFormat,
        #[command(subcommand)]
        action: config::ConfigCommand,
    },
    #[cfg(feature = "operations-cli")]
    /// Check instance health and configuration
    Doctor {
        #[arg(long, env = "ASTER_CLI_OUTPUT_FORMAT", default_value = "auto")]
        output_format: shared::OutputFormat,
        #[command(flatten)]
        args: doctor::DoctorArgs,
    },
    #[cfg(feature = "operations-cli")]
    /// Manage follower enrollment
    Node {
        #[arg(long, env = "ASTER_CLI_OUTPUT_FORMAT", default_value = "auto")]
        output_format: shared::OutputFormat,
        #[command(subcommand)]
        action: node::NodeCommand,
    },
    #[cfg(feature = "operations-cli")]
    /// Migrate data between database backends
    DatabaseMigrate {
        #[arg(long, env = "ASTER_CLI_OUTPUT_FORMAT", default_value = "auto")]
        output_format: database_migration::DatabaseMigrateOutputFormat,
        #[command(flatten)]
        args: database_migration::DatabaseMigrateArgs,
    },
    #[cfg(feature = "operations-cli")]
    /// Generate an authentication verifier (masked terminal input, or stdin until EOF)
    HashPassword,
    /// Show build information
    Version {
        /// Output build information as JSON
        #[arg(long)]
        json: bool,
    },
    /// Generate shell completion scripts
    Completions {
        #[command(subcommand)]
        action: CompletionCommand,
    },
}

#[derive(Debug, Clone, Subcommand)]
enum CompletionCommand {
    /// Install the generated completion script for a shell
    Install {
        /// Shell to install for; defaults to the current SHELL environment variable
        #[arg(value_enum)]
        shell: Option<CompletionShell>,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CompletionShell {
    Bash,
    Zsh,
    Fish,
}

impl CompletionShell {
    fn clap_shell(self) -> Shell {
        match self {
            Self::Bash => Shell::Bash,
            Self::Zsh => Shell::Zsh,
            Self::Fish => Shell::Fish,
        }
    }
}

/// The process entry owns server startup and the final exit status.
pub enum CommandOutcome {
    Serve {
        role: crate::runtime::role::ServeRole,
        location: crate::config::ConfigLocation,
    },
    Exit(i32),
}

pub async fn execute() -> io::Result<CommandOutcome> {
    let mut command = help::configure_cli_help(RootCli::command());
    let cli = RootCli::from_arg_matches(&command.clone().get_matches())
        .unwrap_or_else(|error| error.exit());
    if let Some(RootCommand::Completions { action }) = cli.command.as_ref() {
        match action {
            CompletionCommand::Install { shell } => {
                let shell = shell.or_else(shell_from_environment).ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "shell not recognized; pass bash, zsh, or fish",
                    )
                })?;
                let shell = shell.clap_shell();
                let path = install_completions(shell, &mut command)?;
                println!(
                    "Installed {} completion to {}",
                    shell_name(shell),
                    path.display()
                );
            }
        }
        return Ok(CommandOutcome::Exit(0));
    }
    let config_location =
        crate::config::ConfigLocation::resolve(cli.config_file.as_deref(), cli.data_dir.as_deref())
            .map_err(io_other)?;
    let serve_role = match cli.command {
        Some(RootCommand::Version { json }) => {
            let output = render_build_info(json).map_err(io_other)?;
            println!("{output}");
            return Ok(CommandOutcome::Exit(0));
        }
        #[cfg(feature = "operations-cli")]
        Some(RootCommand::HashPassword) => {
            return Ok(match super::hash_password::execute().await {
                Ok(()) => CommandOutcome::Exit(0),
                Err(CliError::Canceled) => {
                    eprintln!(
                        "\n{}",
                        shared::CliTerminalPalette::stderr().warn("Canceled.")
                    );
                    CommandOutcome::Exit(130)
                }
                Err(CliError::Failed(error)) => {
                    eprintln!("{error}");
                    CommandOutcome::Exit(1)
                }
            });
        }
        #[cfg(feature = "operations-cli")]
        Some(RootCommand::Doctor {
            output_format,
            args,
        }) => {
            if args.dry_run && !args.fix {
                eprintln!("--dry-run requires --fix for doctor");
                return Ok(CommandOutcome::Exit(2));
            }
            if args.fix
                && !args.dry_run
                && let Err(error) = shared::confirm_write(
                    "Apply the detected storage counter repairs? Run with --dry-run first to preview.",
                    args.yes,
                )
            {
                return Ok(CommandOutcome::Exit(confirmation_error_exit(error)));
            }
            let context = match CliContext::existing(&config_location, args.database_url.as_deref())
            {
                Ok(context) => context,
                Err(error) => {
                    eprintln!("{}", config::render_error(output_format, &error));
                    return Ok(CommandOutcome::Exit(1));
                }
            };
            let report = doctor::execute_doctor_command(&args, &context).await;
            println!("{}", doctor::render_doctor_success(output_format, &report));
            if report.should_exit_nonzero() {
                return Ok(CommandOutcome::Exit(1));
            }
            return Ok(CommandOutcome::Exit(0));
        }
        #[cfg(feature = "operations-cli")]
        Some(RootCommand::Config {
            database_url,
            output_format,
            action,
        }) => {
            let result = async {
                let context = CliContext::existing(&config_location, database_url.as_deref())?;
                config::execute_config_command(&context, &action).await
            }
            .await;
            match result {
                Ok(data) => {
                    println!("{}", config::render_success(output_format, &data));
                    return Ok(CommandOutcome::Exit(0));
                }
                Err(CliError::Canceled) => {
                    eprintln!(
                        "\n{}",
                        shared::CliTerminalPalette::stderr().warn("Canceled.")
                    );
                    return Ok(CommandOutcome::Exit(130));
                }
                Err(CliError::Failed(error)) => {
                    eprintln!("{}", config::render_error(output_format, &error));
                    return Ok(CommandOutcome::Exit(1));
                }
            }
        }
        #[cfg(feature = "operations-cli")]
        Some(RootCommand::DatabaseMigrate {
            output_format,
            args,
        }) => {
            if !args.dry_run
                && !args.verify_only
                && let Err(error) = shared::confirm_write(
                    "Apply database migrations and copy data to the target? Run with --dry-run first to preview.",
                    args.yes,
                )
            {
                return Ok(CommandOutcome::Exit(confirmation_error_exit(error)));
            }
            match database_migration::execute_database_migration(&args).await {
                Ok(data) => {
                    println!(
                        "{}",
                        database_migration::render_database_migration_success(output_format, &data,)
                    );
                    return Ok(CommandOutcome::Exit(0));
                }
                Err(error) => {
                    eprintln!(
                        "{}",
                        database_migration::render_database_migration_error(output_format, &error,)
                    );
                    return Ok(CommandOutcome::Exit(1));
                }
            }
        }
        #[cfg(feature = "operations-cli")]
        Some(RootCommand::Node {
            output_format,
            action: node::NodeCommand::Enroll(args),
        }) => match node::execute_node_enroll(&args, &config_location).await {
            Ok(data) => {
                println!("{}", node::render_node_success(output_format, &data));
                return Ok(CommandOutcome::Exit(0));
            }
            Err(CliError::Canceled) => {
                eprintln!(
                    "\n{}",
                    shared::CliTerminalPalette::stderr().warn("Canceled.")
                );
                return Ok(CommandOutcome::Exit(130));
            }
            Err(CliError::Failed(error)) => {
                eprintln!("{}", node::render_node_error(output_format, &error));
                return Ok(CommandOutcome::Exit(1));
            }
        },
        Some(RootCommand::Serve { role }) => role,
        Some(RootCommand::Completions { .. }) => return Ok(CommandOutcome::Exit(2)),
        None => crate::runtime::role::ServeRole::All,
    };

    Ok(CommandOutcome::Serve {
        role: serve_role,
        location: config_location,
    })
}

fn io_other(error: impl std::fmt::Display) -> io::Error {
    io::Error::other(error.to_string())
}

#[cfg(feature = "operations-cli")]
fn confirmation_error_exit(error: CliError) -> i32 {
    match error {
        CliError::Canceled => {
            eprintln!(
                "\n{}",
                shared::CliTerminalPalette::stderr().warn("Canceled.")
            );
            130
        }
        CliError::Failed(error) => {
            eprintln!("{error}");
            1
        }
    }
}

fn shell_from_environment() -> Option<CompletionShell> {
    let shell = std::env::var_os("SHELL")?;
    match Path::new(&shell).file_name()?.to_str()? {
        "bash" => Some(CompletionShell::Bash),
        "zsh" => Some(CompletionShell::Zsh),
        "fish" => Some(CompletionShell::Fish),
        _ => None,
    }
}

fn shell_name(shell: Shell) -> &'static str {
    match shell {
        Shell::Bash => "bash",
        Shell::Zsh => "zsh",
        Shell::Fish => "fish",
        _ => "unsupported",
    }
}

fn install_completions(shell: Shell, command: &mut clap::Command) -> io::Result<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
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

// clap_complete's static Fish generator omits positional arguments. Derive
// the missing enum candidates from the same command metadata as its options.
fn add_fish_positional_values(command: &clap::Command, path: &[&str], output: &mut Vec<u8>) {
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
            // Command and enum names here are the CLI's fixed identifiers,
            // never instance settings or user input.
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
mod role_cli_tests {
    use super::*;
    use crate::runtime::role::ServeRole;

    #[test]
    fn default_and_explicit_role_commands() {
        for (args, expected) in [
            (vec!["aster_drive"], ServeRole::All),
            (vec!["aster_drive", "serve"], ServeRole::All),
            (vec!["aster_drive", "serve", "all"], ServeRole::All),
            (vec!["aster_drive", "serve", "api"], ServeRole::Api),
            (vec!["aster_drive", "serve", "worker"], ServeRole::Worker),
        ] {
            let role = match RootCli::try_parse_from(args).unwrap().command {
                None => ServeRole::All,
                Some(RootCommand::Serve { role }) => role,
                other => panic!("unexpected command: {other:?}"),
            };
            assert_eq!(role, expected);
        }
    }

    #[test]
    fn invalid_or_conflicting_roles_are_rejected() {
        for args in [
            vec!["aster_drive", "serve", "follower"],
            vec!["aster_drive", "serve", "API"],
            vec!["aster_drive", "serve", "api", "worker"],
            vec!["aster_drive", "serve", "--role", "worker"],
        ] {
            assert!(RootCli::try_parse_from(args).is_err());
        }
    }

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

    #[test]
    fn generated_completion_includes_cli_commands_and_dynamic_values() {
        for shell in [Shell::Bash, Shell::Zsh, Shell::Fish] {
            let mut command = RootCli::command();
            let mut output = Vec::new();
            clap_complete::generate(shell, &mut command, "aster_drive", &mut output);
            if shell == Shell::Fish {
                add_fish_positional_values(&command, &[], &mut output);
                assert!(String::from_utf8_lossy(&output).contains("-a 'all api worker'"));
                assert!(String::from_utf8_lossy(&output).contains("-a 'bash zsh fish'"));
            }
            let output = String::from_utf8(output).unwrap();
            assert_eq!(
                output.contains("database-migrate"),
                cfg!(feature = "operations-cli")
            );
            assert!(output.contains("completions"));
            assert!(output.contains("serve"));
            assert!(output.contains("worker"));
        }
    }
}

#[cfg(all(test, feature = "operations-cli"))]
mod config_cli_tests {
    use super::*;

    #[test]
    fn config_accepts_positional_keys_values_and_interactive_selection() {
        for args in [
            vec!["config", "get", "branding_title"],
            vec!["config", "delete", "custom.example"],
            vec!["config", "set", "branding_title", "My Drive"],
            vec!["config", "set", "branding_title"],
            vec!["config", "set"],
            vec!["config", "set", "branding_title", "--clear"],
            vec!["config", "validate", "branding_title", "My Drive"],
        ] {
            assert!(RootCli::try_parse_from(std::iter::once("aster_drive").chain(args)).is_ok());
        }
    }

    #[test]
    fn config_rejects_duplicate_keys_and_value_sources_without_exposing_values() {
        for args in [
            vec!["config", "get", "branding_title", "--key", "branding_title"],
            vec![
                "config",
                "set",
                "mail_smtp_password",
                "credential-marker",
                "--value-stdin",
            ],
            vec![
                "config",
                "set",
                "mail_smtp_password",
                "--value",
                "credential-marker",
                "--clear",
            ],
        ] {
            let error =
                RootCli::try_parse_from(std::iter::once("aster_drive").chain(args)).unwrap_err();
            assert!(!error.to_string().contains("credential-marker"));
        }
    }

    #[test]
    fn node_accepts_positional_primary_and_rejects_duplicate_targets() {
        for args in [
            vec!["node", "enroll"],
            vec!["node", "enroll", "https://drive.example.com"],
            vec![
                "node",
                "enroll",
                "--master-url",
                "https://drive.example.com",
            ],
        ] {
            assert!(RootCli::try_parse_from(std::iter::once("aster_drive").chain(args)).is_ok());
        }
        assert!(
            RootCli::try_parse_from([
                "aster_drive",
                "node",
                "enroll",
                "https://one.example.com",
                "--master-url",
                "https://two.example.com"
            ])
            .is_err()
        );
    }
}

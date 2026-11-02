//! Static Clap command tree and command-line parsing.

use super::commands::completions::CompletionCommand;
use clap::{CommandFactory, Parser, Subcommand};
use std::path::PathBuf;

#[cfg(feature = "operations-cli")]
use super::commands::{config, database_migration, doctor, node};
use super::common::help;
#[cfg(feature = "operations-cli")]
use super::common::output;

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
pub(super) struct RootCli {
    /// Instance configuration file
    #[arg(
        long,
        global = true,
        env = "ASTER_CLI_CONFIG_FILE",
        value_name = "PATH",
        conflicts_with = "data_dir"
    )]
    pub(super) config_file: Option<PathBuf>,
    /// Instance data directory
    #[arg(
        long,
        global = true,
        env = "ASTER_CLI_DATA_DIR",
        value_name = "PATH",
        conflicts_with = "config_file"
    )]
    pub(super) data_dir: Option<PathBuf>,
    #[command(subcommand)]
    pub(super) command: Option<RootCommand>,
}

#[derive(Debug, Clone, Subcommand)]
pub(super) enum RootCommand {
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
        output_format: output::OutputFormat,
        #[command(subcommand)]
        action: config::args::ConfigCommand,
    },
    #[cfg(feature = "operations-cli")]
    /// Check instance health and configuration
    Doctor {
        #[arg(long, env = "ASTER_CLI_OUTPUT_FORMAT", default_value = "auto")]
        output_format: output::OutputFormat,
        #[command(flatten)]
        args: doctor::args::DoctorArgs,
    },
    #[cfg(feature = "operations-cli")]
    /// Manage follower enrollment
    Node {
        #[arg(long, env = "ASTER_CLI_OUTPUT_FORMAT", default_value = "auto")]
        output_format: output::OutputFormat,
        #[command(subcommand)]
        action: node::args::NodeCommand,
    },
    #[cfg(feature = "operations-cli")]
    /// Migrate data between database backends
    DatabaseMigrate {
        #[arg(long, env = "ASTER_CLI_OUTPUT_FORMAT", default_value = "auto")]
        output_format: output::OutputFormat,
        #[command(flatten)]
        args: database_migration::args::DatabaseMigrateArgs,
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

pub(super) fn command() -> clap::Command {
    help::configure_cli_help(RootCli::command())
}

#[cfg(test)]
mod role_cli_tests {
    use super::*;
    use crate::runtime::role::ServeRole;
    use clap::{CommandFactory, Parser};

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
    fn generated_completion_includes_cli_commands_and_dynamic_values() {
        for shell in [
            clap_complete::Shell::Bash,
            clap_complete::Shell::Zsh,
            clap_complete::Shell::Fish,
        ] {
            let mut command = RootCli::command();
            let mut output = Vec::new();
            clap_complete::generate(shell, &mut command, "aster_drive", &mut output);
            if shell == clap_complete::Shell::Fish {
                crate::cli::commands::completions::add_fish_positional_values(
                    &command,
                    &[],
                    &mut output,
                );
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
    use clap::Parser;

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

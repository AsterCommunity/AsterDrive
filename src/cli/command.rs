//! Root command parsing, help policy and operational command dispatch.

use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use std::io;

#[cfg(feature = "operations-cli")]
use super::{
    build_info::render_build_info, config, context::CliContext, database_migration, doctor, node,
    shared,
};

use super::help;

#[derive(Debug, Parser)]
#[command(
    name = "aster_drive",
    display_name = "AsterDrive",
    version,
    propagate_version = true,
    about = "Self-hosted file storage.",
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
    /// Generate an authentication verifier
    HashPassword,
    #[cfg(feature = "operations-cli")]
    /// Show build information
    Version {
        /// Output build information as JSON
        #[arg(long)]
        json: bool,
    },
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
    let cli =
        RootCli::from_arg_matches(&help::configure_cli_help(RootCli::command()).get_matches())
            .unwrap_or_else(|error| error.exit());
    let config_location =
        crate::config::ConfigLocation::resolve(cli.config_file.as_deref(), cli.data_dir.as_deref())
            .map_err(io_other)?;
    let serve_role = match cli.command {
        #[cfg(feature = "operations-cli")]
        Some(RootCommand::Version { json }) => {
            let output = render_build_info(json).map_err(io_other)?;
            println!("{output}");
            return Ok(CommandOutcome::Exit(0));
        }
        #[cfg(feature = "operations-cli")]
        Some(RootCommand::HashPassword) => {
            super::hash_password::execute().await?;
            return Ok(CommandOutcome::Exit(0));
        }
        #[cfg(feature = "operations-cli")]
        Some(RootCommand::Doctor {
            output_format,
            args,
        }) => {
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
                Err(error) => {
                    eprintln!("{}", config::render_error(output_format, &error));
                    return Ok(CommandOutcome::Exit(1));
                }
            }
        }
        #[cfg(feature = "operations-cli")]
        Some(RootCommand::DatabaseMigrate {
            output_format,
            args,
        }) => match database_migration::execute_database_migration(&args).await {
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
        },
        #[cfg(feature = "operations-cli")]
        Some(RootCommand::Node {
            output_format,
            action: node::NodeCommand::Enroll(args),
        }) => match node::execute_node_enroll(&args, &config_location).await {
            Ok(data) => {
                println!("{}", node::render_node_success(output_format, &data));
                return Ok(CommandOutcome::Exit(0));
            }
            Err(error) => {
                eprintln!("{}", node::render_node_error(output_format, &error));
                return Ok(CommandOutcome::Exit(1));
            }
        },
        Some(RootCommand::Serve { role }) => role,
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
                #[cfg(feature = "operations-cli")]
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
}

//! Root command parsing, help policy and operational command dispatch.

use super::CommandOutcome;
use super::app::{RootCli, RootCommand};
use super::commands::completions::{self, CompletionCommand};
use std::io;

#[cfg(feature = "operations-cli")]
use super::{
    commands::{config, database_migration, doctor, node},
    common::error::CliError,
    common::{context::CliContext, terminal},
};

use super::commands::build_info::render_build_info;

/// The process entry owns server startup and the final exit status.
pub(super) async fn execute(
    cli: RootCli,
    command: &mut clap::Command,
) -> io::Result<CommandOutcome> {
    if let Some(RootCommand::Completions { action }) = cli.command.as_ref() {
        match action {
            CompletionCommand::Install { shell } => {
                let shell = shell
                    .or_else(completions::shell_from_environment)
                    .ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "shell not recognized; pass bash, zsh, or fish",
                        )
                    })?;
                let shell = shell.clap_shell();
                let path = completions::install_completions(shell, command)?;
                println!(
                    "Installed {} completion to {}",
                    completions::shell_name(shell),
                    path.display()
                );
            }
        }
        return Ok(CommandOutcome::Exit(0));
    }
    if let Some(RootCommand::Version { json }) = cli.command.as_ref() {
        println!("{}", render_build_info(*json).map_err(io_other)?);
        return Ok(CommandOutcome::Exit(0));
    }
    let config_location =
        crate::config::ConfigLocation::resolve(cli.config_file.as_deref(), cli.data_dir.as_deref())
            .map_err(io_other)?;
    let serve_role = match cli.command {
        Some(RootCommand::Version { .. }) => return Ok(CommandOutcome::Exit(0)),
        #[cfg(feature = "operations-cli")]
        Some(RootCommand::HashPassword) => {
            return Ok(match super::commands::hash_password::execute().await {
                Ok(hash) => {
                    println!("{hash}");
                    CommandOutcome::Exit(0)
                }
                Err(CliError::Canceled) => {
                    eprintln!(
                        "\n{}",
                        terminal::CliTerminalPalette::stderr().warn("Canceled.")
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
                && let Err(error) = terminal::confirm_write(
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
                    eprintln!("{}", config::render::render_error(output_format, &error));
                    return Ok(CommandOutcome::Exit(1));
                }
            };
            let report = doctor::execute::execute_doctor_command(&args, &context).await;
            println!(
                "{}",
                doctor::render::render_doctor_success(output_format, &report)
            );
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
                config::execute::execute_config_command(&context, &action).await
            }
            .await;
            match result {
                Ok(data) => {
                    println!("{}", config::render::render_success(output_format, &data));
                    return Ok(CommandOutcome::Exit(0));
                }
                Err(CliError::Canceled) => {
                    eprintln!(
                        "\n{}",
                        terminal::CliTerminalPalette::stderr().warn("Canceled.")
                    );
                    return Ok(CommandOutcome::Exit(130));
                }
                Err(CliError::Failed(error)) => {
                    eprintln!("{}", config::render::render_error(output_format, &error));
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
                && let Err(error) = terminal::confirm_write(
                    "Apply database migrations and copy data to the target? Run with --dry-run first to preview.",
                    args.yes,
                )
            {
                return Ok(CommandOutcome::Exit(confirmation_error_exit(error)));
            }
            match database_migration::execute::execute_database_migration(&args).await {
                Ok(data) => {
                    println!(
                        "{}",
                        database_migration::render::render_database_migration_success(
                            output_format,
                            &data,
                        )
                    );
                    return Ok(CommandOutcome::Exit(0));
                }
                Err(error) => {
                    eprintln!(
                        "{}",
                        database_migration::render::render_database_migration_error(
                            output_format,
                            &error,
                        )
                    );
                    return Ok(CommandOutcome::Exit(1));
                }
            }
        }
        #[cfg(feature = "operations-cli")]
        Some(RootCommand::Node {
            output_format,
            action: node::args::NodeCommand::Enroll(args),
        }) => match node::execute::execute_node_enroll(&args, &config_location).await {
            Ok(data) => {
                println!(
                    "{}",
                    node::render::render_node_success(output_format, &data)
                );
                return Ok(CommandOutcome::Exit(0));
            }
            Err(CliError::Canceled) => {
                eprintln!(
                    "\n{}",
                    terminal::CliTerminalPalette::stderr().warn("Canceled.")
                );
                return Ok(CommandOutcome::Exit(130));
            }
            Err(CliError::Failed(error)) => {
                eprintln!("{}", node::render::render_node_error(output_format, &error));
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
                terminal::CliTerminalPalette::stderr().warn("Canceled.")
            );
            130
        }
        CliError::Failed(error) => {
            eprintln!("{error}");
            1
        }
    }
}

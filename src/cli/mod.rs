//! Command-line process boundary for the single AsterDrive executable.
mod app;
mod commands;
mod common;
mod dispatch;

/// CLI result consumed by the process entry point.
pub enum CommandOutcome {
    Serve {
        role: crate::runtime::role::ServeRole,
        location: crate::config::ConfigLocation,
    },
    Exit(i32),
}

/// Parses arguments and runs an operational command or selects server startup.
pub async fn run() -> std::io::Result<CommandOutcome> {
    use clap::FromArgMatches;
    let mut command = app::command();
    let cli = app::RootCli::from_arg_matches(&command.clone().get_matches())
        .unwrap_or_else(|error| error.exit());
    dispatch::execute(cli, &mut command).await
}

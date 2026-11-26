//! Command-line process boundary for the single AsterDrive executable.
mod app;
mod commands;
mod common;
mod dispatch;

/// CLI result consumed by the process entry point.
pub enum CommandOutcome {
    /// Start serving; `role` selects which responsibilities to run and
    /// `location` records where the active configuration was loaded from.
    Serve {
        /// Node responsibilities selected for this process.
        role: crate::runtime::role::ServeRole,
        /// Where the active configuration file was resolved from.
        location: crate::config::ConfigLocation,
    },
    /// Exit the process immediately with the given exit code (0 for success).
    Exit(i32),
}

/// Parses arguments and runs an operational command or selects server startup.
pub async fn run(
    prepare_panic: &impl Fn(&std::path::Path, bool),
) -> std::io::Result<CommandOutcome> {
    use clap::FromArgMatches;
    let mut command = app::command();
    let cli = app::RootCli::from_arg_matches(&command.clone().get_matches())
        .unwrap_or_else(|error| error.exit());
    dispatch::execute(cli, &mut command, prepare_panic).await
}

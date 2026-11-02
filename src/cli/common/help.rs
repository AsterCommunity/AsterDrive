//! Help layout and styles shared by service and operations builds.

use clap::builder::styling::{AnsiColor, Effects, Styles};

pub(in crate::cli) fn configure_cli_help(command: clap::Command) -> clap::Command {
    command
        .help_template(concat!(
            "AsterDrive ",
            env!("CARGO_PKG_VERSION"),
            "\n{about}\n\n{usage-heading} {usage}\n\n{all-args}{after-help}"
        ))
        .max_term_width(100)
        .next_line_help(false)
        .styles(cli_styles())
        .disable_help_flag(true)
        .arg(
            clap::Arg::new("help")
                .short('h')
                .long("help")
                .action(clap::ArgAction::HelpShort)
                .help("Show help"),
        )
        .mut_args(|arg| arg.hide_env(true))
        .mut_subcommands(configure_cli_help)
}

/// Builds the clap styling palette shared by all CLI subcommands.
pub(in crate::cli) fn cli_styles() -> Styles {
    Styles::styled()
        .header(AnsiColor::Green.on_default() | Effects::BOLD)
        .usage(AnsiColor::Green.on_default() | Effects::BOLD)
        .literal(AnsiColor::Cyan.on_default() | Effects::BOLD)
        .placeholder(AnsiColor::Yellow.on_default())
}

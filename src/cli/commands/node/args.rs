use crate::cli::common::input::SecretArg;
use clap::{Args, Subcommand};
#[derive(Debug, Clone, Subcommand)]
pub enum NodeCommand {
    /// Connect this follower to its primary using an enrollment token
    Enroll(NodeEnrollArgs),
}

#[derive(Debug, Clone, Args)]
pub struct NodeEnrollArgs {
    /// Primary URL (also accepts ASTER_CLI_MASTER_URL; prompts when omitted)
    #[arg(index = 1, value_name = "MASTER_URL", conflicts_with = "master_url")]
    pub positional_master_url: Option<String>,
    /// Primary URL
    #[arg(long)]
    pub master_url: Option<String>,
    /// Explicit token (may be exposed in shell history and process listings)
    #[arg(long, group = "token_input")]
    pub token: Option<SecretArg>,
    /// Read the enrollment token from stdin
    #[arg(long, group = "token_input")]
    pub token_stdin: bool,
    /// Read the enrollment token from a private UTF-8 file
    #[arg(long, group = "token_input")]
    pub token_file: Option<std::path::PathBuf>,
    /// Read the enrollment token with masked feedback (default when omitted)
    #[arg(long, group = "token_input")]
    pub token_prompt: bool,
    #[arg(long, env = "ASTER_CLI_DATABASE_URL")]
    pub database_url: Option<String>,
    /// Confirm follower enrollment without an interactive prompt.
    #[arg(long, env = "ASTER_CLI_YES", default_value_t = false)]
    pub yes: bool,
}

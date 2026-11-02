use clap::Args;
#[derive(Debug, Clone, Args)]
pub struct DatabaseMigrateArgs {
    #[arg(long, env = "ASTER_CLI_SOURCE_DATABASE_URL")]
    pub source_database_url: String,
    #[arg(long, env = "ASTER_CLI_TARGET_DATABASE_URL")]
    pub target_database_url: String,
    #[arg(long, env = "ASTER_CLI_DRY_RUN", default_value_t = false)]
    pub dry_run: bool,
    #[arg(long, env = "ASTER_CLI_VERIFY_ONLY", default_value_t = false)]
    pub verify_only: bool,
    /// Confirm target schema and data writes without an interactive prompt.
    #[arg(long, env = "ASTER_CLI_YES", default_value_t = false)]
    pub yes: bool,
}

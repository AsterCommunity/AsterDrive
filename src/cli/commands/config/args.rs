use crate::cli::common::input::SecretArg;
use crate::errors::{AsterError, Result};
use clap::{Args, Subcommand};
use std::path::PathBuf;
#[derive(Debug, Clone, Subcommand)]
pub enum ConfigCommand {
    /// List saved runtime settings
    List,
    /// Show one runtime setting (sensitive values are hidden)
    Get(KeyArgs),
    /// Update one runtime setting
    Set(KeyValueArgs),
    /// Remove a custom runtime setting
    Delete(DeleteArgs),
    /// Check a value or backup without saving changes
    Validate(ValidateArgs),
    /// Write an importable JSON backup (secrets omitted by default)
    Export(ExportArgs),
    /// Restore settings from a versioned JSON backup
    Import(FileArgs),
}

impl ConfigCommand {
    /// Whether this invocation may write state, excluding read commands and previews.
    pub(super) fn writes_state(&self) -> bool {
        match self {
            Self::Set(args) => !args.dry_run,
            Self::Delete(args) => !args.dry_run,
            Self::Import(args) => !args.dry_run,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Default, Args)]
#[group(skip)]
pub struct KeyArgs {
    /// Runtime setting key
    #[arg(index = 1, value_name = "KEY", conflicts_with = "key")]
    pub positional_key: Option<String>,
    /// Runtime setting key (also accepts ASTER_CLI_CONFIG_KEY)
    #[arg(long, value_name = "KEY")]
    pub key: Option<String>,
}

impl KeyArgs {
    pub(super) fn resolve(&self) -> Result<Option<String>> {
        if self.positional_key.is_some() && self.key.is_some() {
            return Err(AsterError::validation_error(
                "provide KEY or --key, not both",
            ));
        }
        let key = self
            .positional_key
            .clone()
            .or_else(|| self.key.clone())
            .or_else(|| std::env::var("ASTER_CLI_CONFIG_KEY").ok());
        if key.as_ref().is_some_and(|key| key.trim().is_empty()) {
            return Err(AsterError::validation_error(
                "configuration key cannot be empty",
            ));
        }
        Ok(key)
    }

    pub(super) fn required(&self) -> Result<String> {
        self.resolve()?.ok_or_else(|| {
            AsterError::validation_error(
                "configuration key required; use config get KEY or config delete KEY",
            )
        })
    }
}

#[derive(Debug, Clone, Default, Args)]
#[group(id = "config_value_input", multiple = false)]
pub struct ValueInput {
    /// Explicit value (prefer terminal, stdin or private files for secrets)
    #[arg(index = 2, value_name = "VALUE", allow_negative_numbers = true)]
    pub positional_value: Option<SecretArg>,
    /// Explicit value (secrets may be exposed in shell history and process listings)
    #[arg(long)]
    pub value: Option<SecretArg>,
    /// Read a value from stdin; remove one trailing line ending
    #[arg(long)]
    pub value_stdin: bool,
    /// Read a value from a private UTF-8 file; remove one trailing line ending
    #[arg(long)]
    pub value_file: Option<std::path::PathBuf>,
    /// Prompt in the terminal (sensitive input displays asterisks)
    #[arg(long)]
    pub value_prompt: bool,
    /// Store an empty value, subject to the setting's validation rules
    #[arg(long)]
    pub clear: bool,
}

impl ValueInput {
    pub fn supplied(&self) -> bool {
        self.explicitly_supplied() || std::env::var_os("ASTER_CLI_CONFIG_VALUE").is_some()
    }

    pub(super) fn explicitly_supplied(&self) -> bool {
        self.positional_value.is_some()
            || self.value.is_some()
            || self.value_stdin
            || self.value_file.is_some()
            || self.value_prompt
            || self.clear
    }

    pub(super) fn interactive(&self) -> bool {
        self.value_prompt || !self.supplied()
    }
}

#[derive(Debug, Clone, Default, Args)]
pub struct KeyValueArgs {
    #[command(flatten)]
    pub key: KeyArgs,
    #[command(flatten)]
    pub input: ValueInput,
    /// Preview the normalized change without writing or publishing a reload.
    #[arg(long)]
    pub dry_run: bool,
    /// Confirm the write without an interactive prompt.
    #[arg(long, env = "ASTER_CLI_YES", default_value_t = false)]
    pub yes: bool,
}

#[derive(Debug, Clone, Default, Args)]
pub struct DeleteArgs {
    #[command(flatten)]
    pub key: KeyArgs,
    /// Preview the deletion without writing or publishing a reload.
    #[arg(long)]
    pub dry_run: bool,
    /// Confirm the write without an interactive prompt.
    #[arg(long, env = "ASTER_CLI_YES", default_value_t = false)]
    pub yes: bool,
}

#[derive(Debug, Clone, Args)]
pub struct ExportArgs {
    /// Include plaintext secrets in the backup; protect the resulting file
    #[arg(long)]
    pub include_sensitive: bool,
}

#[derive(Debug, Clone, Args)]
pub struct FileArgs {
    #[arg(long, env = "ASTER_CLI_INPUT_FILE")]
    pub input_file: PathBuf,
    /// Preview imported changes without writing or publishing a reload.
    #[arg(long)]
    pub dry_run: bool,
    /// Confirm the write without an interactive prompt.
    #[arg(long, env = "ASTER_CLI_YES", default_value_t = false)]
    pub yes: bool,
}

#[derive(Debug, Clone, Args)]
pub struct ValidateArgs {
    #[command(flatten)]
    pub key: KeyArgs,
    #[command(flatten)]
    pub input: ValueInput,
    #[arg(long)]
    pub input_file: Option<PathBuf>,
}

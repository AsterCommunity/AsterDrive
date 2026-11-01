//! `aster_drive node` 的聚合入口。

use super::CliResult;
use super::input::{SecretArg, read_secret, read_terminal_value};
use crate::config::node_mode::NodeRuntimeMode;
use crate::errors::{AsterError, Result};
use clap::{Args, Subcommand};
use inquire::validator::Validation;
use secrecy::ExposeSecret;
use serde::Serialize;
use std::io::IsTerminal;

use super::shared::{
    CliTerminalPalette, OutputFormat, ResolvedOutputFormat, human_key, prepare_database_config,
    render_error_json, render_success_envelope,
};

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

#[derive(Debug, Serialize)]
pub struct NodeEnrollReport {
    action: &'static str,
    binding_id: i64,
    binding_name: String,
    master_url: String,
    access_key: String,
    config_path: String,
    server_host: String,
    server_port: u16,
    readiness_check_path: String,
    connectivity_hint: String,
}

pub async fn execute_node_enroll(
    args: &NodeEnrollArgs,
    location: &crate::config::ConfigLocation,
) -> CliResult<NodeEnrollReport> {
    let supplied_master = args
        .positional_master_url
        .clone()
        .or_else(|| args.master_url.clone())
        .or_else(|| std::env::var("ASTER_CLI_MASTER_URL").ok());
    let input = match supplied_master {
        Some(url) => url,
        None => {
            if !std::io::stdin().is_terminal() {
                return Err(AsterError::validation_error(
                    "primary URL required; use node enroll MASTER_URL or --master-url URL",
                )
                .into());
            }
            read_terminal_value(false, "Primary URL", |value: &str| {
                Ok(
                    if crate::storage::remote_protocol::normalize_remote_base_url(value)
                        .is_ok_and(|url| !url.is_empty())
                    {
                        Validation::Valid
                    } else {
                        Validation::Invalid("Enter a non-empty http/https primary URL".into())
                    },
                )
            })?
            .expose_secret()
            .to_owned()
        }
    };
    let master_url = match crate::storage::remote_protocol::normalize_remote_base_url(&input) {
        Ok(url) if !url.is_empty() => url,
        _ => {
            return Err(AsterError::validation_error(
                "primary URL must be a non-empty http/https URL",
            )
            .into());
        }
    };
    let environment_token = if args.token.is_none()
        && !args.token_stdin
        && args.token_file.is_none()
        && !args.token_prompt
    {
        std::env::var("ASTER_CLI_ENROLLMENT_TOKEN")
            .ok()
            .map(SecretArg::from)
    } else {
        None
    };
    let explicit_token = args.token.as_ref().or(environment_token.as_ref());
    let prompt = args.token_prompt
        || (explicit_token.is_none() && !args.token_stdin && args.token_file.is_none());
    if prompt && !std::io::stdin().is_terminal() {
        return Err(AsterError::validation_error(
            "enrollment token required; use --token-stdin, --token-file PATH, or an interactive terminal",
        ).into());
    }
    let token = if prompt {
        read_terminal_value(true, "Enrollment token", |value: &str| {
            Ok(if value.is_empty() || value.chars().any(char::is_control) {
                Validation::Invalid(
                    "enrollment token must be non-empty without control characters".into(),
                )
            } else {
                Validation::Valid
            })
        })?
    } else {
        read_secret(explicit_token, args.token_stdin, args.token_file.as_deref())?
    };
    if token.expose_secret().is_empty() || token.expose_secret().chars().any(char::is_control) {
        return Err(AsterError::validation_error(
            "enrollment token must be non-empty and contain no control characters",
        )
        .into());
    }
    super::shared::confirm_write(
        &format!("Enroll this follower with primary {master_url}?"),
        args.yes,
    )?;
    let config_path = ensure_follower_start_mode(location)?;
    let config = crate::config::get_config();
    let database = args.database_url.as_deref().map_or_else(
        || config.database.clone(),
        |database_url| crate::config::DatabaseConfig {
            url: database_url.into(),
            pool_size: 1,
            retry_count: 0,
        },
    );
    let db = prepare_database_config(&database, &config).await?;
    let result = crate::services::remote::node_enrollment::enroll(
        &db,
        crate::services::remote::node_enrollment::NodeEnrollmentInput {
            master_url,
            token: token.expose_secret().to_owned(),
        },
    )
    .await
    .map_err(|error| {
        if error.message().contains(token.expose_secret()) {
            AsterError::validation_error("node enrollment failed (credential redacted)")
        } else {
            error
        }
    })?;
    let binding = result.binding;
    let binding_id = binding.id;

    Ok(NodeEnrollReport {
        action: result.action,
        binding_id,
        binding_name: binding.name.clone(),
        master_url: binding.master_url.clone(),
        access_key: binding.access_key.clone(),
        config_path,
        server_host: config.server.host.clone(),
        server_port: config.server.port,
        readiness_check_path: "/health/ready".to_string(),
        connectivity_hint: build_connectivity_hint(&config.server.host, config.server.port),
    })
}

pub fn render_node_success(format: OutputFormat, report: &NodeEnrollReport) -> String {
    match format.resolve() {
        ResolvedOutputFormat::Json => render_success_envelope(report, false),
        ResolvedOutputFormat::PrettyJson => render_success_envelope(report, true),
        ResolvedOutputFormat::Human => render_node_human(report),
    }
}

pub fn render_node_error(format: OutputFormat, err: &AsterError) -> String {
    match format.resolve() {
        ResolvedOutputFormat::Json => render_error_json(err, false),
        ResolvedOutputFormat::PrettyJson => render_error_json(err, true),
        ResolvedOutputFormat::Human => err.to_string(),
    }
}

fn ensure_follower_start_mode(location: &crate::config::ConfigLocation) -> Result<String> {
    let default = crate::services::remote::node_enrollment::follower_seed_config();
    let config_path = location.ensure_default(&default)?;
    let config_path = config_path.display().to_string();

    crate::config::init_config_at(location)?;
    let config = crate::config::get_config();

    if matches!(
        crate::config::node_mode::start_mode(config.as_ref()),
        NodeRuntimeMode::Follower
    ) {
        return Ok(config_path);
    }

    Err(AsterError::validation_error(format!(
        "before enrolling this node, set [server].start_mode = \"follower\" in {} and rerun the command",
        config_path
    )))
}

fn render_node_human(report: &NodeEnrollReport) -> String {
    let palette = CliTerminalPalette::stdout();
    let title = palette.title("AsterDrive Node Enrollment");
    let status = palette.status_badge("ok");
    let mut lines = vec![
        format!("{title} {status}"),
        palette.dim("--------------------------------------------------"),
        format!("{}{}", human_key("Action", &palette), report.action),
        format!(
            "{}{} (#{} )",
            human_key("Binding", &palette),
            report.binding_name,
            report.binding_id
        )
        .replace("#", "#")
        .replace(" )", ")"),
        format!("{}{}", human_key("Master URL", &palette), report.master_url),
        format!("{}{}", human_key("Access Key", &palette), report.access_key),
        format!("{}{}", human_key("Config", &palette), report.config_path),
        format!(
            "{}{}:{}",
            human_key("Listen", &palette),
            report.server_host,
            report.server_port
        ),
    ];

    lines.push(String::new());
    lines.push(palette.label("Next steps:"));
    lines.push(
        "  1. Restart the AsterDrive process on this node so follower services start.".to_string(),
    );
    lines.push(format!(
        "  2. For direct transport, confirm the master can reach this node on {}:{}.",
        palette.accent(&report.server_host),
        report.server_port
    ));
    lines.push(format!("     {}", report.connectivity_hint));
    lines.push(format!(
        "  3. For reverse tunnel transport, keep outbound access from this node to {} available.",
        palette.accent(&report.master_url)
    ));

    lines.join("\n")
}

fn build_connectivity_hint(server_host: &str, server_port: u16) -> String {
    if aster_forge_utils::net::is_loopback_host(server_host) {
        return format!(
            "Current server.host is {server_host}. Direct transport from another machine needs a reachable bind host or reverse proxy in front of port {server_port}."
        );
    }

    format!(
        "If you publish the follower for direct transport, make sure the public address forwards to port {server_port}."
    )
}

#[cfg(test)]
mod tests {
    use super::{NodeEnrollReport, render_node_human};

    #[test]
    fn render_node_human_focuses_on_connectivity_steps() {
        let report = NodeEnrollReport {
            action: "created",
            binding_id: 7,
            binding_name: "node-a".to_string(),
            master_url: "http://localhost:3000".to_string(),
            access_key: "ak_test".to_string(),
            config_path: "data/config.toml".to_string(),
            server_host: "127.0.0.1".to_string(),
            server_port: 3000,
            readiness_check_path: "/health/ready".to_string(),
            connectivity_hint:
                "Current server.host is 127.0.0.1. If the master runs on another machine, change server.host or put a reverse proxy/tunnel in front of port 3000."
                    .to_string(),
        };

        let rendered = render_node_human(&report);

        assert!(rendered.contains("Next steps:"));
        assert!(rendered.contains("Listen"));
        assert!(rendered.contains("127.0.0.1:3000"));
        assert!(rendered.contains("For direct transport"));
        assert!(rendered.contains("confirm the master can reach this node"));
        assert!(rendered.contains("For reverse tunnel transport"));
        assert!(rendered.contains("keep outbound access"));
    }

    #[test]
    fn follower_seed_config_uses_public_bind_host() {
        let config = crate::services::remote::node_enrollment::follower_seed_config();

        assert_eq!(config.server.start_mode, super::NodeRuntimeMode::Follower);
        assert_eq!(
            config.server.host,
            crate::services::remote::node_enrollment::FOLLOWER_DEFAULT_SERVER_HOST
        );
    }
}

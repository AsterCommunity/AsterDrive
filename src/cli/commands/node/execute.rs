//! Follower enrollment input, bootstrap and service orchestration.
use super::{args::NodeEnrollArgs, render::build_connectivity_hint, report::NodeEnrollReport};
use crate::cli::common::{
    database::prepare_database_config,
    error::CliResult,
    input::{SecretArg, read_secret, read_terminal_value},
};
use crate::config::node_mode::NodeRuntimeMode;
use crate::errors::{AsterError, Result};
use inquire::validator::Validation;
use secrecy::ExposeSecret;
use std::io::IsTerminal;
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
    crate::cli::common::terminal::confirm_write(
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

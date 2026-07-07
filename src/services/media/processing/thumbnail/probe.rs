use crate::config::media_processing as media_processing_config;
use crate::errors::{AsterError, MapAsterErr, Result};

use crate::services::media::processing::shared::run_cli_command_with_timeout;

pub async fn probe_ffmpeg_cli_command(command: &str) -> Result<String> {
    let command = media_processing_config::normalize_ffmpeg_command(command)?;
    if !media_processing_config::command_is_available(&command) {
        return Err(AsterError::validation_error(format!(
            "ffmpeg_cli command '{command}' is not available"
        )));
    }

    tracing::debug!(
        processor = "ffmpeg_cli",
        command = %command,
        "starting ffmpeg CLI probe"
    );

    let probe_command = command.clone();
    let output = tokio::task::spawn_blocking(move || {
        run_cli_command_with_timeout(&probe_command, &["-version"], |message| {
            AsterError::validation_error(format!("ffmpeg_cli probe failed: {message}"))
        })
    })
    .await
    .map_aster_err_ctx(
        "ffmpeg CLI probe task panicked",
        AsterError::validation_error,
    )??;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            format!("exit status {}", output.status)
        };
        return Err(AsterError::validation_error(format!(
            "ffmpeg_cli probe failed for '{command}': {detail}"
        )));
    }

    let detail = first_non_empty_output_line(&output.stdout)
        .or_else(|| first_non_empty_output_line(&output.stderr))
        .unwrap_or_default();

    tracing::debug!(
        processor = "ffmpeg_cli",
        command = %command,
        version = detail.as_str(),
        "ffmpeg CLI probe completed"
    );

    if detail.is_empty() {
        Ok(format!("ffmpeg_cli command '{command}' is available"))
    } else {
        Ok(format!(
            "ffmpeg_cli command '{command}' is available: {detail}"
        ))
    }
}

fn first_non_empty_output_line(output: &[u8]) -> Option<String> {
    String::from_utf8_lossy(output)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

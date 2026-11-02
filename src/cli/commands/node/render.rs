//! Enrollment report rendering.
use super::report::NodeEnrollReport;
use crate::cli::common::{
    output::{OutputFormat, ResolvedOutputFormat, render_error_json, render_success_envelope},
    terminal::{CliTerminalPalette, human_key},
};
use crate::errors::AsterError;
pub(in crate::cli) fn render_node_success(
    format: OutputFormat,
    report: &NodeEnrollReport,
) -> String {
    match format.resolve() {
        ResolvedOutputFormat::Json => render_success_envelope(report, false),
        ResolvedOutputFormat::PrettyJson => render_success_envelope(report, true),
        ResolvedOutputFormat::Human => render_node_human(report),
    }
}

pub(in crate::cli) fn render_node_error(format: OutputFormat, err: &AsterError) -> String {
    match format.resolve() {
        ResolvedOutputFormat::Json => render_error_json(err, false),
        ResolvedOutputFormat::PrettyJson => render_error_json(err, true),
        ResolvedOutputFormat::Human => err.to_string(),
    }
}

pub(super) fn render_node_human(report: &NodeEnrollReport) -> String {
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

pub(super) fn build_connectivity_hint(server_host: &str, server_port: u16) -> String {
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

        assert_eq!(
            config.server.start_mode,
            crate::config::node_mode::NodeRuntimeMode::Follower
        );
        assert_eq!(
            config.server.host,
            crate::services::remote::node_enrollment::FOLLOWER_DEFAULT_SERVER_HOST
        );
    }
}

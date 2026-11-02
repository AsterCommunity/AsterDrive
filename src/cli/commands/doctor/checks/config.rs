//! Effective runtime settings checks.
use super::super::report::{DoctorCheck, DoctorStatus};
pub(in crate::cli::commands::doctor) fn doctor_public_site_url_check(
    runtime_config: &crate::config::RuntimeConfig,
) -> DoctorCheck {
    let Some(raw_value) = runtime_config.get(crate::config::site_url::PUBLIC_SITE_URL_KEY) else {
        return DoctorCheck {
            name: "public_site_url",
            label: "Public site URL",
            status: DoctorStatus::Warn,
            summary: "public_site_url is not configured".to_string(),
            details: vec![
                "share, preview, and callback URLs will not have a stable public origin"
                    .to_string(),
            ],
            suggestion: Some(
                "Set config public_site_url to an externally reachable HTTP(S) origin.".to_string(),
            ),
        };
    };

    if raw_value.trim().is_empty() {
        return DoctorCheck {
            name: "public_site_url",
            label: "Public site URL",
            status: DoctorStatus::Warn,
            summary: "public_site_url is empty".to_string(),
            details: vec![
                "share, preview, and callback URLs will not have a stable public origin"
                    .to_string(),
            ],
            suggestion: Some(
                "Set config public_site_url to an externally reachable HTTP(S) origin.".to_string(),
            ),
        };
    }

    match crate::config::site_url::parse_public_site_url_value(&raw_value) {
        Ok(origins) => {
            let configured = serde_json::to_string(&origins)
                .unwrap_or_else(|_| "<invalid public_site_url origins>".to_string());
            if origins.is_empty() {
                return DoctorCheck {
                    name: "public_site_url",
                    label: "Public site URL",
                    status: DoctorStatus::Warn,
                    summary: "public_site_url is empty".to_string(),
                    details: vec![
                        "share, preview, and callback URLs will not have a stable public origin"
                            .to_string(),
                    ],
                    suggestion: Some(
                        "Set config public_site_url to at least one externally reachable HTTP(S) origin."
                            .to_string(),
                    ),
                };
            }
            if origins.iter().any(|origin| origin.starts_with("http://")) {
                return DoctorCheck {
                    name: "public_site_url",
                    label: "Public site URL",
                    status: DoctorStatus::Warn,
                    summary: "public_site_url uses insecure HTTP".to_string(),
                    details: vec![
                        format!("configured={configured}"),
                        "production deployments should terminate TLS at a reverse proxy"
                            .to_string(),
                    ],
                    suggestion: Some(
                        "Put the site behind an HTTPS reverse proxy and change public_site_url to an https:// origin."
                            .to_string(),
                    ),
                };
            }

            DoctorCheck {
                name: "public_site_url",
                label: "Public site URL",
                status: DoctorStatus::Ok,
                summary: format!("configured as {configured}"),
                details: Vec::new(),
                suggestion: None,
            }
        }
        Err(err) => DoctorCheck {
            name: "public_site_url",
            label: "Public site URL",
            status: DoctorStatus::Fail,
            summary: "public_site_url is invalid".to_string(),
            details: vec![err.message().to_string()],
            suggestion: Some(
                "Use a plain origin such as https://drive.example.com, without a path or non-HTTP(S) scheme."
                    .to_string(),
            ),
        },
    }
}

pub(in crate::cli::commands::doctor) fn doctor_mail_check(
    runtime_config: &crate::config::RuntimeConfig,
) -> DoctorCheck {
    let settings = crate::config::mail::runtime_mail_settings(runtime_config);
    let mut details = vec![
        format!(
            "smtp_host={}",
            non_empty_or_placeholder(&settings.smtp_host)
        ),
        format!("smtp_port={}", settings.smtp_port),
        format!(
            "from_address={}",
            non_empty_or_placeholder(&settings.from_address)
        ),
        format!(
            "auth={}",
            if settings.smtp_username.trim().is_empty() {
                "disabled"
            } else {
                "enabled"
            }
        ),
        format!(
            "transport_security={}",
            if settings.encryption_enabled {
                "enabled"
            } else {
                "disabled"
            }
        ),
    ];

    if settings.smtp_username.trim().is_empty() ^ settings.smtp_password.trim().is_empty() {
        details.push(
            "mail_smtp_username and mail_smtp_password must both be set or both be empty"
                .to_string(),
        );
        return DoctorCheck {
            name: "mail_configuration",
            label: "Mail configuration",
            status: DoctorStatus::Fail,
            summary: "SMTP authentication is only partially configured".to_string(),
            details,
            suggestion: Some(
                "Set both mail_smtp_username and mail_smtp_password together, or leave both empty."
                    .to_string(),
            ),
        };
    }

    if !settings.is_configured() {
        let mut missing = Vec::new();
        if settings.smtp_host.trim().is_empty() {
            missing.push("mail_smtp_host");
        }
        if settings.from_address.trim().is_empty() {
            missing.push("mail_from_address");
        }
        details.push(format!("missing={}", missing.join(", ")));
        return DoctorCheck {
            name: "mail_configuration",
            label: "Mail configuration",
            status: DoctorStatus::Warn,
            summary: "mail delivery is not fully configured".to_string(),
            details,
            suggestion: Some(
                "At minimum, set mail_smtp_host and mail_from_address to make mail delivery usable."
                    .to_string(),
            ),
        };
    }

    DoctorCheck {
        name: "mail_configuration",
        label: "Mail configuration",
        status: DoctorStatus::Ok,
        summary: "mail delivery settings are configured".to_string(),
        details,
        suggestion: None,
    }
}

pub(in crate::cli::commands::doctor) fn doctor_preview_apps_check(
    runtime_config: &crate::config::RuntimeConfig,
) -> DoctorCheck {
    let raw = runtime_config
        .get(crate::services::preview::apps::PREVIEW_APPS_CONFIG_KEY)
        .unwrap_or_else(crate::services::preview::apps::default_public_preview_apps_json);

    let normalized =
        match crate::services::preview::apps::normalize_public_preview_apps_config_value(&raw) {
            Ok(normalized) => normalized,
            Err(err) => {
                return DoctorCheck {
                    name: "preview_apps",
                    label: "Preview app registry",
                    status: DoctorStatus::Fail,
                    summary: "preview app registry is invalid".to_string(),
                    details: vec![err.message().to_string()],
                    suggestion: Some(
                        "Fix frontend_preview_apps_json or restore the default preview app configuration."
                            .to_string(),
                    ),
                };
            }
        };

    let parsed: crate::services::preview::apps::PublicPreviewAppsConfig = match serde_json::from_str(
        &normalized,
    ) {
        Ok(parsed) => parsed,
        Err(err) => {
            return DoctorCheck {
                    name: "preview_apps",
                    label: "Preview app registry",
                    status: DoctorStatus::Fail,
                    summary: "preview app registry could not be parsed".to_string(),
                    details: vec![err.to_string()],
                    suggestion: Some(
                        "Check whether frontend_preview_apps_json was edited into an invalid state; restore the default value if needed."
                            .to_string(),
                    ),
                };
        }
    };

    let total_apps = parsed.apps.len();
    let enabled_apps = parsed.apps.iter().filter(|app| app.enabled).count();
    let wopi_apps = parsed
        .apps
        .iter()
        .filter(|app| {
            app.enabled && app.provider == crate::services::preview::apps::PreviewAppProvider::Wopi
        })
        .count();
    let details = vec![
        format!("apps={total_apps}"),
        format!("enabled={enabled_apps}"),
        format!("wopi_enabled={wopi_apps}"),
    ];

    if wopi_apps > 0 && crate::config::site_url::public_site_urls(runtime_config).is_empty() {
        return DoctorCheck {
            name: "preview_apps",
            label: "Preview app registry",
            status: DoctorStatus::Warn,
            summary: "WOPI apps are configured but public_site_url is empty".to_string(),
            details,
            suggestion: Some(
                "Set public_site_url or disable WOPI apps to avoid generating unusable launch entry points."
                    .to_string(),
            ),
        };
    }

    DoctorCheck {
        name: "preview_apps",
        label: "Preview app registry",
        status: DoctorStatus::Ok,
        summary: "preview app registry is valid".to_string(),
        details,
        suggestion: None,
    }
}

pub(in crate::cli::commands::doctor) fn non_empty_or_placeholder(value: &str) -> &str {
    if value.trim().is_empty() {
        "<empty>"
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::{DoctorStatus, doctor_public_site_url_check};
    use crate::config::RuntimeConfig;
    use crate::config::definitions::CONFIG_CATEGORY_SITE;
    use crate::config::site_url::PUBLIC_SITE_URL_KEY;
    use aster_forge_config::{ConfigSource, ConfigValueType};
    use aster_forge_db::system_config;
    use chrono::Utc;

    fn config_model(key: &str, value: &str) -> system_config::Model {
        system_config::Model {
            id: 1,
            key: key.to_string(),
            value: value.to_string(),
            value_type: ConfigValueType::String,
            requires_restart: false,
            is_sensitive: false,
            source: ConfigSource::System,
            visibility: aster_forge_config::ConfigVisibility::Private,
            namespace: String::new(),
            category: CONFIG_CATEGORY_SITE.to_string(),
            description: "test".to_string(),
            updated_at: Utc::now(),
            updated_by: None,
        }
    }

    #[test]
    fn doctor_public_site_url_warns_for_http_origins() {
        let runtime_config = RuntimeConfig::new();
        runtime_config.apply(config_model(
            PUBLIC_SITE_URL_KEY,
            r#"["http://drive.example.com"]"#,
        ));

        let check = doctor_public_site_url_check(&runtime_config);

        assert_eq!(check.status, DoctorStatus::Warn);
        assert_eq!(check.summary, "public_site_url uses insecure HTTP");
        assert!(
            check
                .details
                .iter()
                .any(|detail| { detail == r#"configured=["http://drive.example.com"]"# })
        );
        assert!(
            check
                .suggestion
                .as_deref()
                .is_some_and(|hint| hint.contains("https://"))
        );
    }

    #[test]
    fn doctor_public_site_url_accepts_https_origins() {
        let runtime_config = RuntimeConfig::new();
        runtime_config.apply(config_model(
            PUBLIC_SITE_URL_KEY,
            r#"["https://drive.example.com"]"#,
        ));

        let check = doctor_public_site_url_check(&runtime_config);

        assert_eq!(check.status, DoctorStatus::Ok);
        assert_eq!(
            check.summary,
            r#"configured as ["https://drive.example.com"]"#
        );
    }
}

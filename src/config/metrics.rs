//! Product metrics configuration and exporter identity.

use crate::config::node_mode::NodeRuntimeMode;
use crate::errors::{AsterError, Result};
use crate::runtime::role::ServeRole;
use aster_forge_metrics::ObservabilityConfig;

fn effective_config(config: ObservabilityConfig) -> ObservabilityConfig {
    ObservabilityConfig {
        #[cfg(not(feature = "metrics"))]
        prometheus: Default::default(),
        #[cfg(not(feature = "otlp-metrics"))]
        otlp_metrics: Default::default(),
        ..config
    }
}

pub(super) fn deserialize_observability<'de, D>(
    deserializer: D,
) -> std::result::Result<ObservabilityConfig, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    ObservabilityConfig::deserialize(deserializer).map(effective_config)
}

pub fn create_provider(
    config: &ObservabilityConfig,
    node_mode: NodeRuntimeMode,
    role: ServeRole,
    instance_id: &str,
) -> Result<aster_forge_metrics::MetricsProvider> {
    effective_config(config.clone())
        .create_provider(aster_forge_metrics::MetricsIdentity {
            service_name: "asterdrive".into(),
            service_version: env!("CARGO_PKG_VERSION").into(),
            profile_namespace: "com.astercosm.drive".into(),
            role: role.as_str().into(),
            node_mode: Some(node_mode.as_str().into()),
            instance_id: instance_id.to_owned(),
        })
        .map_err(|error| AsterError::config_error(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aster_forge_config::ValidatableConfig;
    use aster_forge_metrics::MetricsBasicAuthConfig;

    #[cfg(feature = "metrics")]
    #[tokio::test]
    async fn configured_provider_exports_profile_and_role_identity() {
        use aster_forge_metrics::{MetricDescriptor, ProductCounter};
        use std::time::Duration;

        let mut config = ObservabilityConfig::default();
        config.prometheus.enabled = true;
        config.environment = "test".into();
        for (mode, role) in [
            (NodeRuntimeMode::Primary, ServeRole::All),
            (NodeRuntimeMode::Primary, ServeRole::Api),
            (NodeRuntimeMode::Primary, ServeRole::Worker),
            (NodeRuntimeMode::Follower, ServeRole::All),
        ] {
            let provider = create_provider(&config, mode, role, "profile-test-boot").unwrap();
            assert!(provider.enabled());
            let handle = provider
                .register_batch(&[
                    MetricDescriptor::counter("test", "events", "Test events.", &[])
                        .with_unit("{event}"),
                ])
                .unwrap()
                .remove(0);
            ProductCounter::from_handle(handle).unwrap().inc(&[], 1);
            let output = provider
                .export_prometheus(Duration::from_secs(1))
                .await
                .unwrap();
            let sample = output
                .lines()
                .find(|line| line.starts_with("test_events_total{"))
                .unwrap();
            for attribute in [
                "service_name=\"asterdrive\"".to_owned(),
                format!("service_version=\"{}\"", env!("CARGO_PKG_VERSION")),
                format!("aster_role=\"{}\"", role.as_str()),
                format!("aster_node_mode=\"{}\"", mode.as_str()),
                "deployment_environment_name=\"test\"".into(),
                "service_instance_id=\"profile-test-boot\"".into(),
                "service_namespace=\"com.astercosm.drive\"".into(),
                format!(
                    "aster_metrics_schema_version=\"{}\"",
                    aster_forge_metrics::METRICS_PROFILE_VERSION
                ),
            ] {
                assert!(sample.contains(&attribute), "missing {attribute}: {sample}");
            }
            provider.shutdown(Duration::from_secs(1)).await.unwrap();
        }
    }

    #[cfg(feature = "otlp-metrics")]
    #[tokio::test]
    async fn configured_otlp_payload_contains_profile_resource_attributes() {
        use aster_forge_metrics::{MetricDescriptor, ProductCounter};
        use std::time::Duration;
        use std::{
            io::{Read, Write},
            net::TcpListener,
        };

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/v1/metrics", listener.local_addr().unwrap());
        let receiver = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buffer = [0_u8; 4096];
            loop {
                let read = stream.read(&mut buffer).unwrap();
                assert!(read > 0);
                request.extend_from_slice(&buffer[..read]);
                let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n")
                else {
                    continue;
                };
                let headers = String::from_utf8_lossy(&request[..header_end]).to_ascii_lowercase();
                let length = headers
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length: "))
                    .unwrap()
                    .parse::<usize>()
                    .unwrap();
                if request.len() >= header_end + 4 + length {
                    let body = &request[header_end + 4..header_end + 4 + length];
                    assert!(
                        body.windows("aster.role".len())
                            .any(|value| value == b"aster.role")
                    );
                    assert!(body.windows(3).any(|value| value == b"all"));
                    assert!(
                        body.windows("aster.node.mode".len())
                            .any(|value| value == b"aster.node.mode")
                    );
                    assert!(body.windows(7).any(|value| value == b"primary"));
                    assert!(
                        body.windows("service.namespace".len())
                            .any(|w| w == b"service.namespace")
                    );
                    assert!(
                        body.windows("aster.metrics.schema.version".len())
                            .any(|w| w == b"aster.metrics.schema.version")
                    );
                    assert!(
                        body.windows(aster_forge_metrics::METRICS_PROFILE_VERSION.len())
                            .any(|w| w == aster_forge_metrics::METRICS_PROFILE_VERSION.as_bytes())
                    );
                    stream
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                        )
                        .unwrap();
                    return;
                }
            }
        });

        let mut config = ObservabilityConfig::default();
        config.prometheus.enabled = false;
        config.otlp_metrics.enabled = true;
        config.otlp_metrics.endpoint = endpoint;
        config.otlp_metrics.interval_seconds = 60;
        config.otlp_metrics.timeout_seconds = 2;
        let provider = create_provider(
            &config,
            NodeRuntimeMode::Primary,
            ServeRole::All,
            "otlp-profile-test",
        )
        .unwrap();
        let handle = provider
            .register_batch(&[
                MetricDescriptor::counter("test", "events", "Test events.", &[])
                    .with_unit("{event}"),
            ])
            .unwrap()
            .remove(0);
        ProductCounter::from_handle(handle).unwrap().inc(&[], 1);
        provider.force_flush(Duration::from_secs(3)).await.unwrap();
        receiver.join().unwrap();
        let _ = provider.shutdown(Duration::from_secs(1)).await;
    }

    fn valid_auth() -> MetricsBasicAuthConfig {
        use aster_forge_crypto::{
            PasswordHashPolicy, PasswordHashVerificationLimits, PasswordHashWorkFactor,
        };
        let policy = PasswordHashPolicy::new(
            PasswordHashWorkFactor::new(8, 1, 1, 32).unwrap(),
            PasswordHashVerificationLimits::default(),
        )
        .unwrap();
        MetricsBasicAuthConfig {
            enabled: true,
            username: "prometheus".into(),
            password_hash: aster_forge_crypto::hash_password_with_policy("secret", &policy)
                .unwrap(),
        }
    }

    #[test]
    fn only_compiled_and_enabled_exporters_are_activated() {
        let mut config = ObservabilityConfig::default();
        assert!(!config.prometheus.enabled);
        assert!(ValidatableConfig::validate(&config).is_ok());
        assert!(
            !create_provider(
                &config,
                NodeRuntimeMode::Primary,
                ServeRole::All,
                "disabled-instance",
            )
            .unwrap()
            .enabled()
        );
        config.prometheus.enabled = true;
        assert_eq!(
            ValidatableConfig::validate(&config).is_ok(),
            cfg!(feature = "metrics")
        );
        config.prometheus.enabled = false;
        config.otlp_metrics.enabled = true;
        assert_eq!(
            ValidatableConfig::validate(&config).is_ok(),
            cfg!(feature = "otlp-metrics")
        );
        config.otlp_metrics.enabled = false;
        config.prometheus.basic_auth = Some(valid_auth());
        assert!(ValidatableConfig::validate(&config).is_err());
    }

    #[cfg(any(not(feature = "metrics"), not(feature = "otlp-metrics")))]
    #[test]
    fn unavailable_exporters_are_ignored_without_activating_a_provider() {
        for prometheus in [true, false] {
            if (prometheus && cfg!(feature = "metrics"))
                || (!prometheus && cfg!(feature = "otlp-metrics"))
            {
                continue;
            }
            let mut config = ObservabilityConfig::default();
            config.environment.clear();
            if prometheus {
                config.prometheus.enabled = true;
                config.prometheus.basic_auth = Some(MetricsBasicAuthConfig {
                    enabled: true,
                    username: "invalid:username".into(),
                    password_hash: "invalid-secret-marker".into(),
                });
            } else {
                config.otlp_metrics.enabled = true;
                config.otlp_metrics.endpoint = "invalid://secret-marker".into();
                config.otlp_metrics.interval_seconds = 0;
                config.otlp_metrics.timeout_seconds = 0;
                config
                    .otlp_metrics
                    .headers
                    .insert("invalid header".into(), "secret-marker".into());
            }
            ValidatableConfig::validate(&effective_config(config.clone())).unwrap();
            let loaded: crate::config::Config = toml::from_str(
                &toml::to_string(&crate::config::Config {
                    observability: config.clone(),
                    ..Default::default()
                })
                .unwrap(),
            )
            .unwrap();
            aster_forge_config::ConfigSections::validate_sections(&loaded).unwrap();
            assert!(
                !create_provider(
                    &config,
                    NodeRuntimeMode::Primary,
                    ServeRole::All,
                    "ignored-exporter",
                )
                .unwrap()
                .enabled()
            );
        }
    }

    #[cfg(feature = "metrics")]
    #[test]
    fn compiled_exporters_validate_credentials_and_environment() {
        let mut config = ObservabilityConfig::default();
        config.prometheus.enabled = true;
        config.prometheus.basic_auth = Some(valid_auth());
        ValidatableConfig::validate(&config).unwrap();

        for username in ["", "bad:username", "bad\nusername"] {
            config.prometheus.basic_auth.as_mut().unwrap().username = username.into();
            assert!(ValidatableConfig::validate(&config).is_err());
        }
        config.prometheus.basic_auth = Some(valid_auth());
        config.prometheus.basic_auth.as_mut().unwrap().password_hash =
            "invalid-secret-marker".into();
        let error = ValidatableConfig::validate(&config).unwrap_err();
        assert!(!error.to_string().contains("invalid-secret-marker"));

        config.prometheus.basic_auth = Some(valid_auth());
        config.prometheus.enabled = false;
        assert!(ValidatableConfig::validate(&config).is_err());
        config.prometheus.enabled = true;
        config.environment.clear();
        assert!(ValidatableConfig::validate(&config).is_err());
    }

    #[cfg(feature = "metrics")]
    #[test]
    fn credentials_validate_without_argon_work_and_errors_never_echo_secrets() {
        let valid = valid_auth();
        let mut config = ObservabilityConfig::default();
        config.prometheus.enabled = true;
        config.prometheus.basic_auth = Some(valid.clone());
        ValidatableConfig::validate(&config).unwrap();
        for username in ["", " ", "with:colon", "with\ncontrol", "with\u{7f}control"] {
            let mut invalid = valid.clone();
            invalid.username = username.into();
            config.prometheus.basic_auth = Some(invalid);
            assert!(ValidatableConfig::validate(&config).is_err());
        }
        for hash in [
            "".to_owned(),
            "secret-invalid-phc".to_owned(),
            valid.password_hash.replace("m=8", "m=65537"),
            valid.password_hash.replace("t=1", "t=4"),
            valid.password_hash.replace("p=1", "p=5"),
            valid.password_hash.replace("argon2id", "argon2i"),
            valid.password_hash.replace("v=19", "v=16"),
            "$argon2id$v=19$m=8,t=1,p=1$bad$bad".into(),
        ] {
            let mut invalid = valid.clone();
            invalid.password_hash = hash.clone();
            config.prometheus.basic_auth = Some(invalid);
            let error = ValidatableConfig::validate(&config)
                .unwrap_err()
                .to_string();
            if !hash.is_empty() {
                assert!(!error.contains(&hash));
            }
        }
        config.prometheus.basic_auth = Some(MetricsBasicAuthConfig {
            enabled: false,
            username: valid.username,
            password_hash: "bad".into(),
        });
        ValidatableConfig::validate(&config).unwrap();
    }

    #[cfg(feature = "metrics")]
    #[test]
    fn environment_source_overrides_file_credentials_without_changing_the_file() {
        let input = "[observability.prometheus.basic_auth]\nenabled = false\nusername = 'file-user'\npassword_hash = 'file-hash'\n";
        let environment = std::collections::HashMap::from([
            (
                "ASTER__OBSERVABILITY__PROMETHEUS__BASIC_AUTH__ENABLED".into(),
                "true".into(),
            ),
            (
                "ASTER__OBSERVABILITY__PROMETHEUS__BASIC_AUTH__USERNAME".into(),
                "env-user".into(),
            ),
            (
                "ASTER__OBSERVABILITY__PROMETHEUS__BASIC_AUTH__PASSWORD_HASH".into(),
                "env-hash".into(),
            ),
        ]);
        let config: crate::config::Config = config::Config::builder()
            .add_source(config::File::from_str(input, config::FileFormat::Toml))
            .add_source(
                config::Environment::with_prefix("ASTER")
                    .separator("__")
                    .try_parsing(true)
                    .source(Some(environment)),
            )
            .build()
            .unwrap()
            .try_deserialize()
            .unwrap();
        let auth = &config.observability.prometheus.basic_auth;
        let auth = auth.as_ref().unwrap();
        assert!(auth.enabled);
        assert_eq!(auth.username, "env-user");
        assert_eq!(auth.password_hash, "env-hash");
        let reloaded: crate::config::Config = toml::from_str(input).unwrap();
        assert_eq!(
            reloaded
                .observability
                .prometheus
                .basic_auth
                .unwrap()
                .password_hash,
            "file-hash"
        );
        let debug = format!("{config:?}");
        assert!(!debug.contains("env-user") && !debug.contains("env-hash"));
    }

    #[cfg(any(feature = "metrics", feature = "otlp-metrics"))]
    #[test]
    fn enabled_exporters_require_a_nonblank_bounded_environment() {
        for prometheus in [true, false] {
            if (prometheus && !cfg!(feature = "metrics"))
                || (!prometheus && !cfg!(feature = "otlp-metrics"))
            {
                continue;
            }
            let mut config = ObservabilityConfig::default();
            config.prometheus.enabled = prometheus;
            config.otlp_metrics.enabled = !prometheus;
            for environment in [
                "".to_owned(),
                " ".into(),
                "\t".into(),
                "prod\n".into(),
                "x".repeat(129),
            ] {
                config.environment = environment;
                assert!(ValidatableConfig::validate(&config).is_err());
            }
            config.environment = "x".repeat(128);
            ValidatableConfig::validate(&config).unwrap();

            config.environment.clear();
            let error = create_provider(
                &config,
                NodeRuntimeMode::Primary,
                ServeRole::All,
                "invalid-environment",
            )
            .err()
            .expect("provider creation must validate an enabled exporter");
            assert!(error.message().contains("environment"));
        }
        let mut disabled = ObservabilityConfig::default();
        disabled.environment.clear();
        ValidatableConfig::validate(&disabled).unwrap();
    }
}

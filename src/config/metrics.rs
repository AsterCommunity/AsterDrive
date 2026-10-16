//! Static credentials for the optional Prometheus pull endpoint.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::errors::{AsterError, Result};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ObservabilityConfig {
    pub environment: String,
    pub prometheus: PrometheusConfig,
    pub otlp_metrics: OtlpMetricsConfig,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            environment: "production".into(),
            prometheus: PrometheusConfig::default(),
            otlp_metrics: OtlpMetricsConfig::default(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct PrometheusConfig {
    pub enabled: bool,
    pub basic_auth: MetricsBasicAuthConfig,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct OtlpMetricsConfig {
    pub enabled: bool,
    pub endpoint: String,
    pub headers: std::collections::BTreeMap<String, String>,
    pub interval_seconds: u64,
    pub timeout_seconds: u64,
}
impl Default for OtlpMetricsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: "http://127.0.0.1:4318/v1/metrics".into(),
            headers: Default::default(),
            interval_seconds: 60,
            timeout_seconds: 5,
        }
    }
}
impl fmt::Debug for OtlpMetricsConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OtlpMetricsConfig")
            .field("enabled", &self.enabled)
            .field("endpoint", &"[REDACTED]")
            .field("headers", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}
impl ObservabilityConfig {
    pub fn validate(&self) -> Result<()> {
        if self.prometheus.enabled && !cfg!(feature = "metrics") {
            return Err(AsterError::config_error(
                "observability.prometheus requires the metrics Cargo feature",
            ));
        }
        if self.otlp_metrics.enabled && !cfg!(feature = "otlp-metrics") {
            return Err(AsterError::config_error(
                "observability.otlp_metrics requires the otlp-metrics Cargo feature",
            ));
        }
        if (self.prometheus.enabled || self.otlp_metrics.enabled)
            && (self.environment.trim().is_empty()
                || self.environment.len() > 128
                || self.environment.chars().any(char::is_control))
        {
            return Err(AsterError::config_error(
                "invalid observability deployment identity",
            ));
        }
        self.prometheus.basic_auth.validate()?;
        if self.prometheus.basic_auth.enabled && !self.prometheus.enabled {
            return Err(AsterError::config_error(
                "observability.prometheus.basic_auth requires the Prometheus endpoint",
            ));
        }
        #[cfg(feature = "otlp-metrics")]
        if self.otlp_metrics.enabled {
            self.otlp_metrics
                .forge_config()
                .validate()
                .map_err(metrics_error)?;
        }
        Ok(())
    }

    pub fn create_provider(
        &self,
        role: crate::config::node_mode::NodeRuntimeMode,
        instance_id: &str,
    ) -> Result<aster_forge_metrics::MetricsProvider> {
        self.validate()?;
        if !self.prometheus.enabled && !self.otlp_metrics.enabled {
            return aster_forge_metrics::MetricsProvider::builder()
                .build()
                .map_err(metrics_error);
        }
        let builder = aster_forge_metrics::MetricsProvider::builder()
            .with_resource(aster_forge_metrics::MetricsResource {
                service_name: "asterdrive".into(),
                service_version: crate::build_info::VERSION.into(),
                environment: self.environment.clone(),
                role: role.as_str().into(),
                instance_id: instance_id.to_owned(),
            })
            .map_err(metrics_error)?;
        #[cfg(feature = "metrics")]
        let builder = if self.prometheus.enabled {
            builder.with_prometheus().map_err(metrics_error)?
        } else {
            builder
        };
        #[cfg(feature = "otlp-metrics")]
        let builder = if self.otlp_metrics.enabled {
            builder
                .with_otlp(self.otlp_metrics.forge_config())
                .map_err(metrics_error)?
                .0
        } else {
            builder
        };
        builder.build().map_err(metrics_error)
    }
}
#[cfg(feature = "otlp-metrics")]
impl OtlpMetricsConfig {
    fn forge_config(&self) -> aster_forge_metrics::OtlpMetricsConfig {
        aster_forge_metrics::OtlpMetricsConfig {
            endpoint: self.endpoint.clone(),
            headers: self.headers.clone(),
            timeout: std::time::Duration::from_secs(self.timeout_seconds),
            interval: std::time::Duration::from_secs(self.interval_seconds),
            max_request_bytes: 16 * 1024 * 1024,
            max_response_bytes: 64 * 1024,
        }
    }
}
fn metrics_error(error: aster_forge_metrics::MetricError) -> AsterError {
    AsterError::config_error(error.to_string())
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct MetricsBasicAuthConfig {
    pub enabled: bool,
    pub username: String,
    pub password_hash: String,
}

impl fmt::Debug for MetricsBasicAuthConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MetricsBasicAuthConfig")
            .field("enabled", &self.enabled)
            .field("username", &"[REDACTED]")
            .field("password_hash", &"[REDACTED]")
            .finish()
    }
}

impl MetricsBasicAuthConfig {
    /// Validate once, before either role starts accepting requests. Forge checks
    /// PHC structure and the default 64 MiB / 3 passes / 4 lanes verification cap.
    pub fn validate(&self) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        if !cfg!(feature = "metrics") {
            return Err(AsterError::config_error(
                "observability.prometheus.basic_auth requires the metrics Cargo feature",
            ));
        }
        if self.username.trim().is_empty()
            || self.username.contains(':')
            || self.username.chars().any(char::is_control)
            || self.username.len() > 256
        {
            return Err(AsterError::config_error(
                "observability.prometheus.basic_auth.username must be nonempty, at most 256 bytes, without colon or control characters",
            ));
        }
        if self.password_hash.len() > 1024 {
            return Err(invalid_hash());
        }
        aster_forge_crypto::validate_password_hash(
            &self.password_hash,
            aster_forge_crypto::PasswordHashVerificationLimits::default(),
        )
        .map_err(|_| invalid_hash())?;
        Ok(())
    }
}

fn invalid_hash() -> AsterError {
    AsterError::config_error(
        "observability.prometheus.basic_auth.password_hash must be a supported Argon2id PHC hash within the password verification budget",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn feature_defaults_and_explicit_unsupported_capabilities_fail_closed() {
        let mut config = ObservabilityConfig::default();
        assert!(!config.prometheus.enabled);
        assert!(config.validate().is_ok());
        assert!(
            !config
                .create_provider(
                    crate::config::node_mode::NodeRuntimeMode::Primary,
                    "disabled-instance"
                )
                .unwrap()
                .enabled()
        );
        config.prometheus.enabled = true;
        assert_eq!(config.validate().is_ok(), cfg!(feature = "metrics"));
        config.prometheus.enabled = false;
        config.otlp_metrics.enabled = true;
        assert_eq!(config.validate().is_ok(), cfg!(feature = "otlp-metrics"));
        config.otlp_metrics.enabled = false;
        config.prometheus.basic_auth = valid_auth();
        assert!(config.validate().is_err());
    }

    #[cfg(feature = "metrics")]
    #[test]
    fn credentials_validate_without_argon_work_and_errors_never_echo_secrets() {
        let valid = valid_auth();
        valid.validate().unwrap();
        for username in ["", " ", "with:colon", "with\ncontrol", "with\u{7f}control"] {
            let mut invalid = valid.clone();
            invalid.username = username.into();
            assert!(invalid.validate().is_err());
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
            let error = invalid.validate().unwrap_err().to_string();
            if !hash.is_empty() {
                assert!(!error.contains(&hash));
            }
        }
        let mut disabled = valid;
        disabled.enabled = false;
        disabled.password_hash = "bad".into();
        disabled.validate().unwrap();
    }

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
        assert!(auth.enabled);
        assert_eq!(auth.username, "env-user");
        assert_eq!(auth.password_hash, "env-hash");
        let reloaded: crate::config::Config = toml::from_str(input).unwrap();
        assert_eq!(
            reloaded.observability.prometheus.basic_auth.password_hash,
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
                assert!(config.validate().is_err());
            }
            config.environment = "x".repeat(128);
            config.validate().unwrap();
        }
        let mut disabled = ObservabilityConfig::default();
        disabled.environment.clear();
        disabled.validate().unwrap();
    }
}

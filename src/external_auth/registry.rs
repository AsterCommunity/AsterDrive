//! 外部认证 provider driver 注册表。

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use super::driver::{ExternalAuthProviderDescriptor, ExternalAuthProviderDriver};
use super::providers::oauth2::OAuth2ProviderDriver;
use super::providers::oidc::OidcProviderDriver;
use crate::errors::{AsterError, Result};
use crate::types::ExternalAuthProviderKind;

pub struct ExternalAuthProviderRegistry {
    drivers: HashMap<ExternalAuthProviderKind, Arc<dyn ExternalAuthProviderDriver>>,
}

impl ExternalAuthProviderRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            drivers: HashMap::new(),
        };
        registry.register(OidcProviderDriver::new());
        registry.register(OAuth2ProviderDriver::new());
        registry
    }

    pub fn register<D>(&mut self, driver: D)
    where
        D: ExternalAuthProviderDriver + 'static,
    {
        self.drivers.insert(driver.kind(), Arc::new(driver));
    }

    pub fn register_arc(&mut self, driver: Arc<dyn ExternalAuthProviderDriver>) {
        self.drivers.insert(driver.kind(), driver);
    }

    pub fn supported_kinds(&self) -> impl Iterator<Item = ExternalAuthProviderKind> + '_ {
        self.drivers.keys().copied()
    }

    pub fn descriptors(&self) -> Vec<ExternalAuthProviderDescriptor> {
        let mut descriptors = self
            .drivers
            .values()
            .map(|driver| driver.descriptor())
            .collect::<Vec<_>>();
        descriptors.sort_by_key(|descriptor| descriptor.kind.as_str());
        descriptors
    }

    pub fn contains(&self, kind: ExternalAuthProviderKind) -> bool {
        self.drivers.contains_key(&kind)
    }

    pub fn get_driver(
        &self,
        kind: ExternalAuthProviderKind,
    ) -> Result<Arc<dyn ExternalAuthProviderDriver>> {
        self.drivers.get(&kind).cloned().ok_or_else(|| {
            AsterError::config_error(format!(
                "external auth provider driver '{}' is not registered",
                kind.as_str()
            ))
        })
    }

    pub fn oidc(&self) -> Result<Arc<dyn ExternalAuthProviderDriver>> {
        self.get_driver(ExternalAuthProviderKind::Oidc)
    }
}

impl Default for ExternalAuthProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub fn default_registry() -> &'static ExternalAuthProviderRegistry {
    static REGISTRY: OnceLock<ExternalAuthProviderRegistry> = OnceLock::new();
    REGISTRY.get_or_init(ExternalAuthProviderRegistry::new)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::external_auth::{
        ExternalAuthAuthorizationStart, ExternalAuthCallback, ExternalAuthProfile,
        ExternalAuthProviderConfig, ExternalAuthProviderTestResult,
    };
    use async_trait::async_trait;

    #[derive(Default)]
    struct TestOidcDriver;

    #[async_trait]
    impl ExternalAuthProviderDriver for TestOidcDriver {
        fn kind(&self) -> ExternalAuthProviderKind {
            ExternalAuthProviderKind::Oidc
        }

        fn descriptor(&self) -> ExternalAuthProviderDescriptor {
            ExternalAuthProviderDescriptor {
                kind: ExternalAuthProviderKind::Oidc,
                protocol: crate::types::ExternalAuthProtocol::Oidc,
                display_name: "Test OIDC",
                description: "Test OIDC driver",
                default_scopes: "openid email profile",
                issuer_url_required: true,
                manual_endpoint_configuration_supported: false,
                authorization_url_required: false,
                token_url_required: false,
                userinfo_url_required: false,
                supports_discovery: true,
                supports_pkce: true,
                supports_email_verified_claim: true,
            }
        }

        async fn start_authorization(
            &self,
            _provider: &ExternalAuthProviderConfig,
            _redirect_uri: &str,
        ) -> Result<ExternalAuthAuthorizationStart> {
            unreachable!("registry tests only inspect driver registration")
        }

        async fn exchange_callback(
            &self,
            _provider: &ExternalAuthProviderConfig,
            _callback: ExternalAuthCallback,
        ) -> Result<ExternalAuthProfile> {
            unreachable!("registry tests only inspect driver registration")
        }

        async fn test_provider(
            &self,
            _provider: &ExternalAuthProviderConfig,
        ) -> Result<ExternalAuthProviderTestResult> {
            unreachable!("registry tests only inspect driver registration")
        }
    }

    #[test]
    fn registry_returns_oidc_driver_by_kind() {
        let registry = ExternalAuthProviderRegistry::new();
        let driver = registry
            .get_driver(ExternalAuthProviderKind::Oidc)
            .expect("OIDC driver should be registered");

        assert_eq!(driver.kind(), ExternalAuthProviderKind::Oidc);
    }

    #[test]
    fn registry_allows_driver_replacement_by_kind() {
        let mut registry = ExternalAuthProviderRegistry::new();
        registry.register(TestOidcDriver);

        assert!(registry.contains(ExternalAuthProviderKind::Oidc));
        assert!(registry.contains(ExternalAuthProviderKind::GenericOAuth2));
    }

    #[test]
    fn default_registry_is_singleton() {
        let first = default_registry() as *const ExternalAuthProviderRegistry;
        let second = default_registry() as *const ExternalAuthProviderRegistry;

        assert_eq!(first, second);
    }
}

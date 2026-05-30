//! 外部认证 provider adapter 抽象。

pub mod driver;
pub mod providers;
pub mod registry;
pub(crate) mod url;

pub use crate::types::{ExternalAuthProtocol, ExternalAuthProviderKind};
pub use driver::{
    ExternalAuthAuthorizationStart, ExternalAuthCallback, ExternalAuthProfile,
    ExternalAuthProviderConfig, ExternalAuthProviderDescriptor, ExternalAuthProviderDriver,
    ExternalAuthProviderTestCheck, ExternalAuthProviderTestResult,
};
pub use registry::ExternalAuthProviderRegistry;

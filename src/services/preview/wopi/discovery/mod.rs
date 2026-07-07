//! WOPI 服务子模块：`discovery`。

mod actions;
mod apps;
mod cache;
mod parser;
mod security;
mod types;
mod url;

pub use apps::{allowed_origins, discover_apps};
pub(crate) use apps::{parse_wopi_app_config, resolve_action_url};
pub(crate) use security::{ensure_request_proof_valid, ensure_request_source_allowed};

#[cfg(test)]
pub(crate) use actions::{build_discovered_apps, resolve_discovery_action_url};
#[cfg(test)]
pub(crate) use parser::parse_discovery_xml;
#[cfg(test)]
pub(crate) use url::{append_wopi_src, expand_action_url, trusted_origins_for_app};

//! WebDAV integration, compatibility, and conformance tests.

#[macro_use]
#[path = "../common/mod.rs"]
mod common;

mod accounts;
mod client_e2e;
mod deltav;
mod file;
mod litmus_compliance;
mod lock_system;
mod path_resolver;
mod protocol;
mod xml_depth_probe;

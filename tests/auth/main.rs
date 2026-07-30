//! Authentication, identity, and user-account integration tests.

#[macro_use]
#[path = "../common/mod.rs"]
mod common;
#[path = "../external_auth/mod.rs"]
mod external_auth;

mod auth;
mod gravatar_config;
mod mfa;
mod oauth2;
mod oidc;
mod preferences;
mod user_invitations;
mod user_management;

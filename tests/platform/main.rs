//! Runtime, database, middleware, configuration, and contract integration tests.

#[macro_use]
#[path = "../common/mod.rs"]
mod common;

mod auth_once_state;
mod branding;
mod cache;
mod cors;
mod database_backends;
mod db_indexes;
mod errors;
mod health;
mod middleware;
mod migrations;
mod observability;
mod refactor_contracts;
mod remote_target_audit_migration;
mod schema_drift;
mod security_fixes;

//! Runtime, database, middleware, configuration, and contract integration tests.

#[macro_use]
#[path = "../common/mod.rs"]
mod common;

mod branding;
mod cache;
mod cors;
mod database_backends;
mod db_indexes;
mod errors;
mod health;
mod middleware;
mod migrations;
mod refactor_contracts;
mod schema_drift;
mod security_fixes;

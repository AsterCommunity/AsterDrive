//! CLI 命令、凭据输入和结果展示。
//!
//! `command` 拥有根参数、帮助和分发；进程入口只衔接服务启动与退出状态。

mod build_info;
pub mod command;
#[cfg(feature = "operations-cli")]
mod config;
#[cfg(feature = "operations-cli")]
mod context;
#[cfg(feature = "operations-cli")]
mod database_migration;
#[cfg(feature = "operations-cli")]
mod db_shared;
#[cfg(feature = "operations-cli")]
mod doctor;
#[cfg(feature = "operations-cli")]
mod hash_password;
mod help;
#[cfg(feature = "operations-cli")]
pub mod input;
#[cfg(feature = "operations-cli")]
mod node;
#[cfg(feature = "operations-cli")]
mod shared;

#[cfg(feature = "operations-cli")]
pub use config::{
    ConfigCommand, ConfigCommandReport, DeleteOutput, FileArgs, KeyArgs, KeyValueArgs,
    ValidateArgs, ValueInput, execute_config_command, render_error, render_success,
};
#[cfg(feature = "operations-cli")]
pub use context::CliContext;
#[cfg(feature = "operations-cli")]
pub use database_migration::{
    DatabaseMigrateArgs, DatabaseMigrateOutputFormat, execute_database_migration,
    render_database_migration_error, render_database_migration_success,
};
#[cfg(feature = "operations-cli")]
pub use doctor::{
    DoctorArgs, DoctorCheck, DoctorReport, DoctorStatus, execute_doctor_command,
    render_doctor_success,
};
#[cfg(feature = "operations-cli")]
pub use node::{
    NodeCommand, NodeEnrollArgs, execute_node_enroll, render_node_error, render_node_success,
};
#[cfg(feature = "operations-cli")]
pub use shared::OutputFormat;

//! CLI 聚合入口。
//!
//! 这里统一导出 `doctor`、`config`、`database-migrate` 共享的参数类型、
//! 执行入口和渲染函数，供根命令分发层直接复用。

mod config;
mod database_migration;
mod db_shared;
mod doctor;
mod node;
mod shared;

pub use config::{
    ConfigCommand, ConfigCommandReport, DeleteOutput, FileArgs, KeyArgs, KeyValueArgs,
    ValidateArgs, execute_config_command, render_error, render_success,
};
pub use database_migration::{
    DatabaseMigrateArgs, DatabaseMigrateOutputFormat, execute_database_migration,
    render_database_migration_error, render_database_migration_success,
};
pub use doctor::{
    DoctorArgs, DoctorCheck, DoctorReport, DoctorStatus, execute_doctor_command,
    render_doctor_success,
};
pub use node::{NodeCommand, execute_node_command, render_node_error, render_node_success};
pub use shared::{OutputFormat, cli_styles};

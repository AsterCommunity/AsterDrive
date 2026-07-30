//! 数据库迁移二进制入口。
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::unreachable,
        clippy::expect_used,
        clippy::panic
    )
)]

use sea_orm_migration::prelude::*;

#[tokio::main]
async fn main() {
    cli::run_cli(aster_drive_migration::Migrator).await;
}

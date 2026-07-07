//! AsterDrive 服务端与 CLI 启动入口。
#![deny(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::unreachable,
        clippy::expect_used,
        clippy::panic,
        clippy::unimplemented,
        clippy::todo
    )
)]

use actix_web::{App, HttpServer, web};
use aster_drive::runtime::SharedRuntimeState;
#[cfg(feature = "cli")]
use clap::{Parser, Subcommand};
use std::io;
use tokio_util::sync::CancellationToken;

const HTTP_SHUTDOWN_TIMEOUT_SECS: u64 = 8;

#[cfg(feature = "jemalloc")]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[cfg(all(feature = "jemalloc", not(target_env = "msvc"), target_os = "linux"))]
#[allow(non_upper_case_globals)]
#[unsafe(export_name = "_rjem_malloc_conf")]
pub static malloc_conf: Option<&'static std::ffi::c_char> = Some(unsafe {
    union Conf {
        bytes: &'static u8,
        ptr: &'static std::ffi::c_char,
    }

    // `narenas:1` lowers idle memory for the self-hosted default profile, but
    // can become allocator contention under high concurrency.
    Conf {
        bytes: &b"narenas:1,dirty_decay_ms:1000,muzzy_decay_ms:1000,background_thread:true\0"[0],
    }
    .ptr
});

#[cfg(all(
    feature = "jemalloc",
    not(target_env = "msvc"),
    any(
        target_os = "macos",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    )
))]
#[allow(non_upper_case_globals)]
#[unsafe(export_name = "_rjem_malloc_conf")]
pub static malloc_conf: Option<&'static std::ffi::c_char> = Some(unsafe {
    union Conf {
        bytes: &'static u8,
        ptr: &'static std::ffi::c_char,
    }

    Conf {
        bytes: &b"narenas:1,dirty_decay_ms:1000,muzzy_decay_ms:1000\0"[0],
    }
    .ptr
});

#[cfg(all(debug_assertions, not(feature = "jemalloc")))]
#[global_allocator]
static GLOBAL: aster_drive::alloc::TrackingAlloc = aster_drive::alloc::TrackingAlloc;

#[cfg(feature = "cli")]
#[derive(Debug, Parser)]
#[command(
    name = "aster_drive",
    version,
    about = "AsterDrive server and operations CLI",
    long_about = "AsterDrive server and operations CLI.\n\nRun without a subcommand to start the server, or use 'serve' explicitly. Use 'config' for offline runtime configuration operations.",
    styles = aster_drive::cli::cli_styles()
)]
struct RootCli {
    #[command(subcommand)]
    command: Option<RootCommand>,
}

#[cfg(feature = "cli")]
#[derive(Debug, Clone, Subcommand)]
enum RootCommand {
    /// Start the AsterDrive server
    Serve,
    /// Run offline health checks for database, config, and storage readiness
    Doctor {
        #[arg(long, env = "ASTER_CLI_OUTPUT_FORMAT", default_value = "auto")]
        output_format: aster_drive::cli::OutputFormat,
        #[command(flatten)]
        args: aster_drive::cli::DoctorArgs,
    },
    /// Manage runtime configuration stored in system_config
    Config {
        #[arg(long, env = "ASTER_CLI_DATABASE_URL")]
        database_url: String,
        #[arg(long, env = "ASTER_CLI_OUTPUT_FORMAT", default_value = "auto")]
        output_format: aster_drive::cli::OutputFormat,
        #[command(subcommand)]
        action: aster_drive::cli::ConfigCommand,
    },
    /// Run an offline database backend migration for a maintenance window
    DatabaseMigrate {
        #[arg(long, env = "ASTER_CLI_OUTPUT_FORMAT", default_value = "auto")]
        output_format: aster_drive::cli::DatabaseMigrateOutputFormat,
        #[command(flatten)]
        args: aster_drive::cli::DatabaseMigrateArgs,
    },
    /// Manage remote node enrollment from a shell
    Node {
        #[arg(long, env = "ASTER_CLI_OUTPUT_FORMAT", default_value = "auto")]
        output_format: aster_drive::cli::OutputFormat,
        #[command(subcommand)]
        action: aster_drive::cli::NodeCommand,
    },
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // 0. 安装自定义 panic hook（最先执行）
    aster_drive::runtime::panic::install_panic_hook();

    dotenvy::dotenv().ok();

    #[cfg(feature = "cli")]
    {
        let cli = RootCli::parse();
        match cli.command {
            Some(RootCommand::Doctor {
                output_format,
                args,
            }) => {
                let report = aster_drive::cli::execute_doctor_command(&args).await;
                println!(
                    "{}",
                    aster_drive::cli::render_doctor_success(output_format, &report)
                );
                if report.should_exit_nonzero() {
                    std::process::exit(1);
                }
                return Ok(());
            }
            Some(RootCommand::Config {
                database_url,
                output_format,
                action,
            }) => match aster_drive::cli::execute_config_command(&database_url, &action).await {
                Ok(data) => {
                    println!("{}", aster_drive::cli::render_success(output_format, &data));
                    return Ok(());
                }
                Err(error) => {
                    eprintln!("{}", aster_drive::cli::render_error(output_format, &error));
                    std::process::exit(1);
                }
            },
            Some(RootCommand::DatabaseMigrate {
                output_format,
                args,
            }) => match aster_drive::cli::execute_database_migration(&args).await {
                Ok(data) => {
                    println!(
                        "{}",
                        aster_drive::cli::render_database_migration_success(output_format, &data,)
                    );
                    return Ok(());
                }
                Err(error) => {
                    eprintln!(
                        "{}",
                        aster_drive::cli::render_database_migration_error(output_format, &error,)
                    );
                    std::process::exit(1);
                }
            },
            Some(RootCommand::Node {
                output_format,
                action,
            }) => match aster_drive::cli::execute_node_command(&action).await {
                Ok(data) => {
                    println!(
                        "{}",
                        aster_drive::cli::render_node_success(output_format, &data)
                    );
                    return Ok(());
                }
                Err(error) => {
                    eprintln!(
                        "{}",
                        aster_drive::cli::render_node_error(output_format, &error)
                    );
                    std::process::exit(1);
                }
            },
            Some(RootCommand::Serve) | None => {}
        }
    }

    let bootstrap_config_path =
        aster_drive::services::remote::node_enrollment::prepare_follower_bootstrap_config()
            .map_err(io_other)?;

    // 1. 加载配置（会自动创建 data/config.toml）
    aster_drive::config::init_config().map_err(io_other)?;
    let cfg = aster_drive::config::get_config();
    let runtime_mode = aster_drive::config::node_mode::start_mode(cfg.as_ref());
    if let Some(config_path) = bootstrap_config_path.as_ref()
        && runtime_mode != aster_drive::config::node_mode::NodeRuntimeMode::Follower
    {
        return Err(io_other(format!(
            "before bootstrapping this node from remote enrollment env, set [server].start_mode = \"follower\" in {} and restart",
            config_path.display()
        )));
    }

    // 2. 初始化日志（基于配置）
    let log_result = aster_drive::runtime::logging::init_logging(&cfg.logging);
    let _log_guard = log_result.guard;
    if let Some(warning) = log_result.warning {
        tracing::warn!("{}", warning);
    }

    // 只清理短命 runtime 临时目录：
    // - 不碰 temp_dir 根，避免误删共享目录（例如 /tmp）里的其他内容；
    // - 不碰 temp_dir/tasks，保留后台任务产物给 retention/排障逻辑处理。
    aster_drive::utils::cleanup_runtime_temp_root(&cfg.server.temp_dir).await;

    match runtime_mode {
        aster_drive::config::node_mode::NodeRuntimeMode::Primary => {
            let prepared = aster_drive::runtime::startup::prepare_primary()
                .await
                .map_err(io_other)?;
            run_primary_http_server(prepared).await
        }
        aster_drive::config::node_mode::NodeRuntimeMode::Follower => {
            let prepared = aster_drive::runtime::startup::prepare_follower()
                .await
                .map_err(io_other)?;
            run_follower_http_server(prepared).await
        }
    }
}

fn io_other(error: impl std::fmt::Display) -> io::Error {
    io::Error::other(error.to_string())
}

async fn run_primary_http_server(
    prepared: aster_drive::runtime::startup::PreparedPrimaryRuntime,
) -> std::io::Result<()> {
    let aster_drive::runtime::startup::PreparedPrimaryRuntime {
        state,
        share_download_rollback_worker,
    } = prepared;
    let host = state.config.server.host.clone();
    let port = state.config.server.port;
    let workers = match state.config.server.workers {
        0 => num_cpus::get(),
        n => n,
    };
    tracing::info!(
        mode = "primary",
        host = %host,
        port,
        "starting HTTP service"
    );

    let configure_db = state.writer_db().clone();
    let shutdown_db = state.writer_db().clone();
    let app_shutdown_token = CancellationToken::new();
    let state = web::Data::new(state);
    let task_state = state.clone();
    let server_state = state.clone();
    let app_state = state.clone();
    let app_shutdown_data = web::Data::new(app_shutdown_token.clone());
    let metrics = web::Data::new(state.metrics.clone());
    let server = HttpServer::new(move || {
        let db = configure_db.clone();
        App::new()
            .wrap(actix_web::middleware::Compress::default())
            .wrap(aster_drive::api::middleware::metrics::MetricsMiddleware)
            .wrap(aster_drive::api::middleware::request_id::RequestIdMiddleware)
            .wrap(aster_drive::api::middleware::cors::RuntimeCors)
            .wrap(aster_drive::api::middleware::security_headers::default_headers())
            .app_data(actix_web::web::PayloadConfig::new(
                aster_drive::api::extractors::DEFAULT_PAYLOAD_LIMIT,
            ))
            .app_data(aster_drive::api::extractors::json_config(
                aster_drive::api::extractors::DEFAULT_JSON_LIMIT,
            ))
            .app_data(app_state.clone())
            .app_data(metrics.clone())
            .app_data(app_shutdown_data.clone())
            .configure(move |cfg| aster_drive::api::configure_primary(cfg, &db))
    })
    .keep_alive(std::time::Duration::from_secs(30))
    .client_request_timeout(std::time::Duration::from_millis(5000))
    .client_disconnect_timeout(std::time::Duration::from_millis(1000))
    .shutdown_timeout(HTTP_SHUTDOWN_TIMEOUT_SECS)
    .disable_signals()
    .bind((host.as_str(), port))?
    .workers(workers)
    .run();

    let server_handle = server.handle();
    let background_tasks = aster_drive::runtime::tasks::spawn_primary_background_tasks(
        task_state,
        share_download_rollback_worker,
        app_shutdown_token.clone(),
    );
    aster_drive::runtime::startup::record_server_start(server_state.as_ref()).await;
    tokio::spawn(async move {
        aster_drive::runtime::shutdown::wait_for_signal().await;
        app_shutdown_token.cancel();
        server_handle.stop(true).await;
    });

    let server_result = server.await;
    tracing::info!("server stopped");
    aster_drive::runtime::shutdown::record_server_shutdown(state.as_ref()).await;
    aster_drive::runtime::shutdown::perform_shutdown(background_tasks, shutdown_db).await;
    server_result
}

async fn run_follower_http_server(
    prepared: aster_drive::runtime::startup::PreparedFollowerRuntime,
) -> std::io::Result<()> {
    let state = prepared.state;
    let host = state.config.server.host.clone();
    let port = state.config.server.port;
    let workers = match state.config.server.workers {
        0 => num_cpus::get(),
        n => n,
    };
    tracing::info!(
        mode = "follower",
        host = %host,
        port,
        "starting HTTP service"
    );

    let shutdown_db = state.writer_db().clone();
    let state = web::Data::new(state);
    let app_shutdown_token = CancellationToken::new();
    let metrics = web::Data::new(state.metrics.clone());
    let app_state = state.clone();
    let app_shutdown_data = web::Data::new(app_shutdown_token.clone());
    let server = HttpServer::new(move || {
        App::new()
            .wrap(actix_web::middleware::Compress::default())
            .wrap(aster_drive::api::middleware::metrics::MetricsMiddleware)
            .wrap(aster_drive::api::middleware::request_id::RequestIdMiddleware)
            .wrap(aster_drive::api::middleware::security_headers::default_headers())
            .app_data(actix_web::web::PayloadConfig::new(
                aster_drive::api::extractors::DEFAULT_PAYLOAD_LIMIT,
            ))
            .app_data(aster_drive::api::extractors::json_config(
                aster_drive::api::extractors::DEFAULT_JSON_LIMIT,
            ))
            .app_data(app_state.clone())
            .app_data(metrics.clone())
            .app_data(app_shutdown_data.clone())
            .configure(aster_drive::api::configure_follower)
    })
    .keep_alive(std::time::Duration::from_secs(30))
    .client_request_timeout(std::time::Duration::from_millis(5000))
    .client_disconnect_timeout(std::time::Duration::from_millis(1000))
    .shutdown_timeout(HTTP_SHUTDOWN_TIMEOUT_SECS)
    .disable_signals()
    .bind((host.as_str(), port))?
    .workers(workers)
    .run();

    let server_handle = server.handle();
    let background_tasks = aster_drive::runtime::tasks::spawn_follower_background_tasks(
        state.clone(),
        app_shutdown_token.clone(),
    );
    aster_drive::runtime::startup::record_server_start(state.as_ref()).await;
    tokio::spawn(async move {
        aster_drive::runtime::shutdown::wait_for_signal().await;
        app_shutdown_token.cancel();
        server_handle.stop(true).await;
    });

    let server_result = server.await;
    tracing::info!("server stopped");
    aster_drive::runtime::shutdown::record_server_shutdown(state.as_ref()).await;
    aster_drive::runtime::shutdown::perform_shutdown(background_tasks, shutdown_db).await;
    server_result
}

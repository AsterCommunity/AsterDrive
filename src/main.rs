//! AsterDrive 服务端与 CLI 启动入口。
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

use std::io;

#[cfg(feature = "jemalloc")]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[cfg(all(feature = "jemalloc", not(target_env = "msvc"), target_os = "linux"))]
#[expect(
    non_upper_case_globals,
    reason = "jemalloc discovers this externally named allocator configuration symbol by ABI name"
)]
#[unsafe(export_name = "_rjem_malloc_conf")]
pub static malloc_conf: Option<&'static std::ffi::c_char> = Some(
    // SAFETY: `Conf` is initialized through `bytes` and read through a pointer field with the
    // same representation and alignment; the referenced static byte string is NUL-terminated.
    unsafe {
        union Conf {
            bytes: &'static u8,
            ptr: &'static std::ffi::c_char,
        }

        // `narenas:1` lowers idle memory for the self-hosted default profile, but
        // can become allocator contention under high concurrency.
        Conf {
            bytes: &b"narenas:1,dirty_decay_ms:1000,muzzy_decay_ms:1000,background_thread:true\0"
                [0],
        }
        .ptr
    },
);

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
#[expect(
    non_upper_case_globals,
    reason = "jemalloc discovers this externally named allocator configuration symbol by ABI name"
)]
#[unsafe(export_name = "_rjem_malloc_conf")]
pub static malloc_conf: Option<&'static std::ffi::c_char> = Some(
    // SAFETY: `Conf` is initialized through `bytes` and read through a pointer field with the
    // same representation and alignment; the referenced static byte string is NUL-terminated.
    unsafe {
        union Conf {
            bytes: &'static u8,
            ptr: &'static std::ffi::c_char,
        }

        Conf {
            bytes: &b"narenas:1,dirty_decay_ms:1000,muzzy_decay_ms:1000\0"[0],
        }
        .ptr
    },
);

#[cfg(all(debug_assertions, not(feature = "jemalloc")))]
#[global_allocator]
static GLOBAL: aster_forge_alloc::TrackingAlloc = aster_forge_alloc::TrackingAlloc;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // 0. 安装自定义 panic hook（最先执行）
    aster_forge_panic::install_panic_hook(aster_forge_panic::PanicHookConfig::new(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        env!("CARGO_PKG_REPOSITORY"),
    ));

    dotenvy::dotenv().ok();

    aster_forge_auth::initialize_jwt_crypto_provider().map_err(io_other)?;

    let (serve_role, config_location) = match aster_drive::cli::command::execute().await? {
        aster_drive::cli::command::CommandOutcome::Serve { role, location } => (role, location),
        aster_drive::cli::command::CommandOutcome::Exit(0) => return Ok(()),
        aster_drive::cli::command::CommandOutcome::Exit(code) => std::process::exit(code),
    };
    let bootstrap_config_path =
        aster_drive::services::remote::node_enrollment::prepare_follower_bootstrap_config(
            &config_location,
            &|name| std::env::var(name).ok(),
        )
        .map_err(io_other)?;

    // 1. 加载配置（会自动创建 data/config.toml）
    let config_load_report =
        aster_drive::config::init_config_at(&config_location).map_err(io_other)?;
    for message in config_load_report.messages() {
        if !message.starts_with("[INFO] Configuration loaded from:") {
            eprintln!("{message}");
        }
    }
    let cfg = aster_drive::config::get_config();
    aster_drive::config::deployment::validate_static(cfg.as_ref()).map_err(io_other)?;
    cfg.observability.validate().map_err(io_other)?;
    cfg.worker.validate().map_err(io_other)?;
    let runtime_mode = aster_drive::config::node_mode::start_mode(cfg.as_ref());
    serve_role.validate(runtime_mode).map_err(io_other)?;
    if let Some(config_path) = bootstrap_config_path.as_ref()
        && runtime_mode != aster_drive::config::node_mode::NodeRuntimeMode::Follower
    {
        return Err(io_other(format!(
            "before bootstrapping this node from remote enrollment env, set [server].start_mode = \"follower\" in {} and restart",
            config_path.display()
        )));
    }

    // 2. 初始化日志（基于配置）
    let log_result = aster_forge_logging::init_logging(&cfg.logging);
    let _log_guard = log_result.guard;
    if let Some(warning) = log_result.warning {
        tracing::warn!("{}", warning);
    }
    tracing::info!(path = %config_load_report.config_path().display(), "configuration loaded");
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        revision = env!("ASTER_BUILD_REVISION"),
        build_time = env!("ASTER_BUILD_TIME"),
        profile = env!("ASTER_BUILD_PROFILE"),
        target = env!("ASTER_BUILD_TARGET"),
        features = env!("ASTER_BUILD_FEATURES"),
        mode = runtime_mode.as_str(),
        role = if runtime_mode == aster_drive::config::node_mode::NodeRuntimeMode::Follower {
            "follower"
        } else {
            serve_role.as_str()
        },
        "starting {}",
        "AsterDrive"
    );

    // 只清理短命 runtime 临时目录：
    // - 不碰 temp_dir 根，避免误删共享目录（例如 /tmp）里的其他内容；
    // - 不碰 temp_dir/tasks，保留后台任务产物给 retention/排障逻辑处理。
    aster_forge_utils::fs::cleanup_runtime_temp_root(&cfg.server.temp_dir).await;

    match runtime_mode {
        aster_drive::config::node_mode::NodeRuntimeMode::Primary => {
            let prepared = aster_drive::runtime::startup::prepare_primary(serve_role)
                .await
                .map_err(io_other)?;
            aster_drive::runtime::assembly::run_primary(prepared, serve_role).await
        }
        aster_drive::config::node_mode::NodeRuntimeMode::Follower => {
            let prepared = aster_drive::runtime::startup::prepare_follower()
                .await
                .map_err(io_other)?;
            aster_drive::runtime::assembly::run_follower(prepared).await
        }
    }
}

fn io_other(error: impl std::fmt::Display) -> io::Error {
    io::Error::other(error.to_string())
}

//! Dedicated process fixture for deterministic crash-recovery tests.

#[cfg(unix)]
#[path = "../main.rs"]
#[expect(
    dead_code,
    reason = "the fixture calls run_server instead of the ordinary server main"
)]
mod server;

#[cfg(unix)]
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    aster_drive::reliability::install_fixture_from_env()?;
    server::run_server().await
}

#[cfg(not(unix))]
fn main() {
    eprintln!("reliability fixture requires Unix process signals and local sockets");
    std::process::exit(2);
}

//! Fixture-only crash checkpoints. The production server never installs a controller.

#[cfg(unix)]
use std::sync::OnceLock;

pub const CHECKPOINTS: &[&str] = &[
    "upload.after_staging_sync_before_receipt_transaction",
    "upload.after_blob_persist_before_finalize_transaction",
    "upload.after_finalize_commit_before_cleanup",
    "task.after_claim_before_handler",
    "task.after_side_effect_before_checkpoint",
    "task.after_checkpoint_before_terminal_update",
];

#[cfg(unix)]
struct FixtureController {
    checkpoint: String,
    generation: String,
    socket: std::path::PathBuf,
}

#[cfg(unix)]
static FIXTURE: OnceLock<FixtureController> = OnceLock::new();

/// Called only by the dedicated fixture executable, once per process generation.
#[cfg(unix)]
pub fn install_fixture_from_env() -> std::io::Result<()> {
    // Recovery generations deliberately run with no armed checkpoint.
    if std::env::var_os("ASTER_RELIABILITY_CHECKPOINT").is_none() {
        return Ok(());
    }
    let checkpoint = std::env::var("ASTER_RELIABILITY_CHECKPOINT")
        .map_err(|_| std::io::Error::other("missing fixture checkpoint"))?;
    if !CHECKPOINTS.contains(&checkpoint.as_str()) {
        return Err(std::io::Error::other("unknown fixture checkpoint"));
    }
    let generation = std::env::var("ASTER_RELIABILITY_GENERATION")
        .map_err(|_| std::io::Error::other("missing fixture generation"))?;
    let socket = std::env::var_os("ASTER_RELIABILITY_SOCKET")
        .ok_or_else(|| std::io::Error::other("missing fixture socket"))?;
    FIXTURE
        .set(FixtureController {
            checkpoint,
            generation,
            socket: socket.into(),
        })
        .map_err(|_| std::io::Error::other("fixture controller already installed"))
}

/// Reports the named durable boundary and waits until the controller releases or kills us.
#[cfg(unix)]
pub async fn checkpoint(name: &'static str) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let Some(controller) = FIXTURE
        .get()
        .filter(|controller| controller.checkpoint == name)
    else {
        return;
    };
    let Ok(mut stream) = tokio::net::UnixStream::connect(&controller.socket).await else {
        eprintln!("fixture checkpoint {name}: IPC connection failed");
        std::process::abort();
    };
    let notification = format!("{}|{name}\n", controller.generation);
    if stream.write_all(notification.as_bytes()).await.is_err() {
        eprintln!("fixture checkpoint {name}: notification failed");
        std::process::abort();
    }
    let mut release = [0u8; 1];
    if stream.read_exact(&mut release).await.is_err() {
        eprintln!("fixture checkpoint {name}: controller disconnected");
        std::process::abort();
    }
}

#[cfg(not(unix))]
pub fn install_fixture_from_env() -> std::io::Result<()> {
    Err(std::io::Error::other("reliability fixture requires Unix"))
}

#[cfg(not(unix))]
pub async fn checkpoint(_name: &'static str) {}

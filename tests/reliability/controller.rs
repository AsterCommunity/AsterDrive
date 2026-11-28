//! Local IPC is evidence of a boundary; elapsed time is never evidence of a crash point.

use std::{path::Path, time::Duration};

use aster_drive::reliability::CHECKPOINTS;
use tokio::{io::AsyncReadExt, net::UnixListener};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    pub seed: u64,
    pub checkpoints: Vec<&'static str>,
}

impl Selection {
    pub fn parse(scenario: &str, checkpoint: Option<&str>, seed: &str) -> Result<Self, String> {
        if !matches!(scenario, "all" | "upload" | "archive-task") {
            return Err("unknown scenario; expected all, upload, or archive-task".into());
        }
        let seed = seed
            .parse()
            .map_err(|_| "SEED must be an unsigned integer")?;
        let checkpoints: Vec<_> = CHECKPOINTS
            .iter()
            .copied()
            .filter(|name| {
                (scenario == "all"
                    || name.starts_with(if scenario == "upload" {
                        "upload."
                    } else {
                        "task."
                    }))
                    && checkpoint.is_none_or(|selected| selected == *name)
            })
            .collect();
        if checkpoints.is_empty() {
            return Err("unknown checkpoint or checkpoint does not belong to scenario".into());
        }
        Ok(Self { seed, checkpoints })
    }

    pub fn from_env() -> Self {
        Self::parse(
            &std::env::var("SCENARIO").unwrap_or_else(|_| "all".into()),
            std::env::var("ASTER_RELIABILITY_CHECKPOINT_FILTER")
                .or_else(|_| std::env::var("CHECKPOINT"))
                .ok()
                .filter(|s| !s.is_empty())
                .as_deref(),
            &std::env::var("SEED").unwrap_or_else(|_| "522".into()),
        )
        .expect("invalid reliability selection (no processes started)")
    }

    /// Fixed SplitMix64 sequence; independent of rand versions and process identities.
    pub fn payload(&self) -> Vec<u8> {
        let mut state = self.seed;
        (0..4096)
            .map(|_| {
                state = state.wrapping_add(0x9e3779b97f4a7c15);
                let mut value = state;
                value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
                value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
                (value ^ (value >> 31)).to_le_bytes()[0]
            })
            .collect()
    }
}

pub struct CheckpointController {
    listener: UnixListener,
    generation: String,
    checkpoint: String,
    reached: bool,
    held_stream: Option<tokio::net::UnixStream>,
}

impl CheckpointController {
    pub fn bind(path: &Path, generation: &str, checkpoint: &str) -> Self {
        assert!(
            CHECKPOINTS.contains(&checkpoint),
            "unknown checkpoint before process spawn"
        );
        Self {
            listener: UnixListener::bind(path).expect("bind fixture IPC socket"),
            generation: generation.into(),
            checkpoint: checkpoint.into(),
            reached: false,
            held_stream: None,
        }
    }

    pub async fn wait(&mut self, timeout: Duration) -> Result<(), String> {
        if self.reached {
            return Err("checkpoint reached an unexpected number of times".into());
        }
        let notification = tokio::time::timeout(timeout, async {
            let (mut stream, _) = self
                .listener
                .accept()
                .await
                .map_err(|_| "IPC accept failed")?;
            let mut bytes = Vec::new();
            // Bound an incomplete or malicious notification, including its newline.
            loop {
                let byte = stream
                    .read_u8()
                    .await
                    .map_err(|_| "incomplete checkpoint notification")?;
                if byte == b'\n' {
                    break;
                }
                bytes.push(byte);
                if bytes.len() > 256 {
                    return Err("oversized checkpoint notification");
                }
            }
            let expected = format!("{}|{}", self.generation, self.checkpoint);
            if bytes != expected.as_bytes() {
                return Err("stale generation or unexpected checkpoint notification");
            }
            // Keep the stream open: EOF must not release the checkpoint before SIGKILL.
            Ok(stream)
        })
        .await
        .map_err(|_| "checkpoint timeout: expected IPC notification was not received")?;
        let stream = notification.map_err(str::to_owned)?;
        self.reached = true;
        // Owned by the controller until the process has been killed, not by wait's future.
        self.held_stream = Some(stream);
        Ok(())
    }

    pub async fn assert_no_duplicate(&self) {
        assert!(self.reached, "requested checkpoint was never exercised");
        assert!(
            tokio::time::timeout(Duration::from_millis(25), self.listener.accept())
                .await
                .is_err(),
            "checkpoint reached more than once"
        );
    }

    pub async fn release(&mut self) {
        use tokio::io::AsyncWriteExt;
        self.held_stream
            .as_mut()
            .expect("checkpoint was not reached")
            .write_all(&[1])
            .await
            .expect("release recovered owner");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aster_forge_test::temp::TestTempDir;
    use tokio::{io::AsyncWriteExt, net::UnixStream};

    #[test]
    fn selection_fails_closed_and_seed_replays() {
        assert!(Selection::parse("upload", Some("renamed"), "522").is_err());
        assert!(Selection::parse("upload", Some(CHECKPOINTS[3]), "522").is_err());
        assert!(Selection::parse("unknown", None, "522").is_err());
        assert!(Selection::parse("all", None, "NaN").is_err());
        let first = Selection::parse("upload", None, "12345").unwrap();
        let replay = Selection::parse("upload", None, "12345").unwrap();
        assert_eq!(first, replay);
        assert_eq!(first.payload(), replay.payload());
        assert_ne!(
            first.payload(),
            Selection::parse("upload", None, "12346").unwrap().payload()
        );
        assert_eq!(&first.payload()[..4], &[160, 237, 29, 202]);
    }

    #[tokio::test]
    async fn timeout_is_bounded_and_stale_generation_is_rejected() {
        let dir = TestTempDir::new("reliability-controller");
        let path = dir.join("ipc");
        let mut controller = CheckpointController::bind(&path, "generation-2", CHECKPOINTS[0]);
        assert!(
            controller
                .wait(Duration::from_millis(10))
                .await
                .unwrap_err()
                .contains("timeout")
        );
        let mut stale = UnixStream::connect(&path).await.unwrap();
        stale
            .write_all(format!("generation-1|{}\n", CHECKPOINTS[0]).as_bytes())
            .await
            .unwrap();
        assert!(
            controller
                .wait(Duration::from_secs(1))
                .await
                .unwrap_err()
                .contains("stale generation")
        );
        let mut current = UnixStream::connect(&path).await.unwrap();
        current
            .write_all(format!("generation-2|{}\n", CHECKPOINTS[0]).as_bytes())
            .await
            .unwrap();
        controller.wait(Duration::from_secs(1)).await.unwrap();
        assert!(
            controller
                .wait(Duration::from_secs(1))
                .await
                .unwrap_err()
                .contains("number")
        );
    }

    #[tokio::test]
    async fn missing_checkpoint_has_bounded_redacted_process_diagnostics() {
        use aster_forge_test::process::TestProcess;
        let dir = TestTempDir::new("reliability-timeout");
        let path = dir.join("ipc");
        let mut controller = CheckpointController::bind(&path, "1", CHECKPOINTS[0]);
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_reliability_fixture"));
        command
            .arg("--help")
            .env("ASTER_RELIABILITY_CHECKPOINT", CHECKPOINTS[0])
            .env("ASTER_RELIABILITY_GENERATION", "1")
            .env("ASTER_RELIABILITY_SOCKET", &path);
        let mut process = TestProcess::spawn("missing-checkpoint", &mut command);
        let failure = controller
            .wait(Duration::from_millis(100))
            .await
            .unwrap_err();
        process.terminate();
        let diagnostic = super::super::invariants::redact_logs(&process.diagnostics(), &[]);
        assert!(failure.contains("checkpoint timeout"));
        assert!(
            !diagnostic.is_empty(),
            "timeout must retain process diagnostics"
        );
    }
}

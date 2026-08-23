//! SFTP storage driver integration test using testcontainers.

use std::io::Cursor;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use aster_drive::storage::drivers::sftp::{SftpDriver, SftpDriverConfig, SftpStaticCredentials};
use aster_drive_storage::{
    StorageDriver, StorageErrorKind, StreamUploadAttempt, StreamUploadCleanup, StreamUploadDriver,
};
use testcontainers::{GenericImage, ImageExt, core::IntoContainerPort, runners::AsyncRunner};
use tokio::io::{AsyncRead, AsyncReadExt as _, ReadBuf};

const SFTP_IMAGE: &str = "lscr.io/linuxserver/openssh-server";
const SFTP_TAG: &str = "10.2_p1-r0-ls229";
const SFTP_PORT: u16 = 2222;
const SFTP_USERNAME: &str = "aster";
const SFTP_PASSWORD: &str = "asterpass";
const SFTP_PROBE_TIMEOUT: Duration = Duration::from_secs(15);

struct TruncatedReader {
    prefix: Cursor<Vec<u8>>,
}

impl AsyncRead for TruncatedReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if buffer.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        let mut chunk = [0_u8; 4];
        let read = std::io::Read::read(&mut self.prefix, &mut chunk).expect("read prefix");
        buffer.put_slice(&chunk[..read]);
        Poll::Ready(Ok(()))
    }
}

fn sftp_driver(endpoint: &str, base_path: &str, host_key_fingerprint: Option<&str>) -> SftpDriver {
    SftpDriver::new(
        SftpDriverConfig {
            endpoint: endpoint.to_string(),
            base_path: base_path.to_string(),
            host_key_fingerprint: host_key_fingerprint.map(str::to_string),
        },
        SftpStaticCredentials {
            username: SFTP_USERNAME.to_string(),
            password: SFTP_PASSWORD.to_string(),
        },
    )
    .expect("create SftpDriver")
}

fn docker_sftp_test_enabled() -> bool {
    std::env::var("ASTER_SFTP_TEST_DOCKER")
        .map(|value| {
            !matches!(
                value.as_str(),
                "0" | "false" | "FALSE" | "no" | "NO" | "off" | "OFF"
            )
        })
        .unwrap_or(true)
}

async fn wait_for_sftp_host_key_fingerprint(driver: &SftpDriver) -> String {
    let mut last_error = None;
    let fingerprint = tokio::time::timeout(Duration::from_secs(45), async {
        loop {
            match tokio::time::timeout(SFTP_PROBE_TIMEOUT, driver.exists("readiness/probe.txt"))
                .await
            {
                Ok(Ok(_)) => last_error = Some("untrusted host key was accepted".to_string()),
                Ok(Err(error)) if error.kind() == StorageErrorKind::Precondition => {
                    let rejection = SftpDriver::host_key_rejection(&error)
                        .expect("untrusted host key error should expose rejection details");
                    assert_eq!(rejection.expected, None);
                    break rejection.actual;
                }
                Ok(Err(error)) => last_error = Some(error.to_string()),
                Err(_) => last_error = Some("host key probe attempt timed out".to_string()),
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    })
    .await;

    fingerprint.unwrap_or_else(|_| {
        panic!(
            "timed out waiting for SFTP host key fingerprint: {}",
            last_error.unwrap_or_else(|| "unknown error".to_string())
        )
    })
}

async fn wait_for_sftp(driver: &SftpDriver) {
    let mut last_error = None;
    let ready = tokio::time::timeout(Duration::from_secs(45), async {
        loop {
            match tokio::time::timeout(
                SFTP_PROBE_TIMEOUT,
                driver.put("readiness/probe.txt", b"ready"),
            )
            .await
            {
                Ok(Ok(_)) => {
                    let _ = driver.delete("readiness/probe.txt").await;
                    break;
                }
                Ok(Err(error)) => last_error = Some(error.to_string()),
                Err(_) => last_error = Some("readiness upload attempt timed out".to_string()),
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    })
    .await;

    if ready.is_err() {
        panic!(
            "timed out waiting for SFTP test server: {}",
            last_error.unwrap_or_else(|| "unknown error".to_string())
        );
    }
}

#[tokio::test]
async fn test_sftp_driver_upload_download_round_trip() {
    if !docker_sftp_test_enabled() {
        eprintln!(
            "skipping SFTP docker integration test because ASTER_SFTP_TEST_DOCKER disables it"
        );
        return;
    }

    let container = GenericImage::new(SFTP_IMAGE, SFTP_TAG)
        .with_exposed_port(IntoContainerPort::tcp(SFTP_PORT))
        .with_env_var("PUID", "1000")
        .with_env_var("PGID", "1000")
        .with_env_var("TZ", "UTC")
        .with_env_var("USER_NAME", SFTP_USERNAME)
        .with_env_var("USER_PASSWORD", SFTP_PASSWORD)
        .with_env_var("PASSWORD_ACCESS", "true")
        .with_env_var("SUDO_ACCESS", "false")
        .start()
        .await
        .expect("failed to start sftp container");

    let port = container
        .get_host_port_ipv4(IntoContainerPort::tcp(SFTP_PORT))
        .await
        .expect("resolve mapped sftp port");
    let endpoint = format!("sftp://127.0.0.1:{port}");
    let base_path = format!("asterdrive-itest-{}", uuid::Uuid::new_v4());
    let untrusted_driver = sftp_driver(&endpoint, &base_path, None);
    let host_key_fingerprint = wait_for_sftp_host_key_fingerprint(&untrusted_driver).await;
    SftpDriver::validate_host_key_fingerprint(&host_key_fingerprint)
        .expect("reported host key fingerprint should be valid");

    let driver = sftp_driver(&endpoint, &base_path, Some(&host_key_fingerprint));
    wait_for_sftp(&driver).await;

    let data = b"hello sftp world";
    driver.put("docs/hello.txt", data).await.unwrap();

    #[cfg(debug_assertions)]
    {
        let baseline = driver.debug_connection_pool_snapshot();
        assert_eq!(
            baseline.idle_connections, 1,
            "successful sequential SFTP operation should return one reusable connection"
        );
        assert!(driver.exists("docs/hello.txt").await.unwrap());
        assert_eq!(driver.get("docs/hello.txt").await.unwrap(), data);
        assert_eq!(
            driver.metadata("docs/hello.txt").await.unwrap().size,
            u64::try_from(data.len()).unwrap()
        );
        let after_sequential = driver.debug_connection_pool_snapshot();
        assert_eq!(
            after_sequential.created_connections, baseline.created_connections,
            "sequential SFTP operations should reuse the authenticated connection"
        );
        assert_eq!(after_sequential.idle_connections, 1);
    }

    assert!(driver.exists("docs/hello.txt").await.unwrap());
    assert!(!driver.exists("docs/missing.txt").await.unwrap());
    assert_eq!(driver.get("docs/hello.txt").await.unwrap(), data);

    #[cfg(debug_assertions)]
    let before_missing_metadata = driver.debug_connection_pool_snapshot();
    let missing_meta = driver
        .metadata("docs/missing.txt")
        .await
        .expect_err("missing sftp object metadata should fail");
    assert_eq!(missing_meta.kind(), StorageErrorKind::NotFound);
    #[cfg(debug_assertions)]
    {
        let after_missing_metadata = driver.debug_connection_pool_snapshot();
        assert_eq!(
            after_missing_metadata.created_connections, before_missing_metadata.created_connections,
            "not-found SFTP status should not force the pooled connection to reconnect"
        );
        assert_eq!(after_missing_metadata.idle_connections, 1);
    }

    let meta = driver.metadata("docs/hello.txt").await.unwrap();
    assert_eq!(meta.size, u64::try_from(data.len()).unwrap());

    let unicode_path = "docs/space dir/中文+plus.txt";
    driver.put(unicode_path, b"encoded path").await.unwrap();
    assert_eq!(driver.get(unicode_path).await.unwrap(), b"encoded path");

    #[cfg(debug_assertions)]
    {
        let before_stream = driver.debug_connection_pool_snapshot();
        assert_eq!(before_stream.idle_connections, 1);
        let mut held_stream = driver.get_stream("docs/hello.txt").await.unwrap();
        let after_stream_open = driver.debug_connection_pool_snapshot();
        assert_eq!(
            after_stream_open.created_connections, before_stream.created_connections,
            "opening a stream should lease the existing idle connection"
        );
        assert_eq!(
            after_stream_open.idle_connections, 0,
            "streaming reader must hold its connection until drop"
        );

        assert_eq!(
            driver.metadata("docs/hello.txt").await.unwrap().size,
            u64::try_from(data.len()).unwrap()
        );
        let while_stream_held = driver.debug_connection_pool_snapshot();
        assert_eq!(
            while_stream_held.created_connections,
            before_stream.created_connections + 1,
            "metadata while a stream is open should use another connection instead of sharing the stream lease"
        );

        let mut held_body = Vec::new();
        held_stream.read_to_end(&mut held_body).await.unwrap();
        assert_eq!(held_body, data);
        drop(held_stream);

        let after_stream_drop = driver.debug_connection_pool_snapshot();
        assert_eq!(
            after_stream_drop.idle_connections, 2,
            "dropping the streaming reader should return its connection lease"
        );
    }

    let mut full_stream = driver.get_stream("docs/hello.txt").await.unwrap();
    let mut full_body = Vec::new();
    full_stream.read_to_end(&mut full_body).await.unwrap();
    assert_eq!(full_body, data);

    let mut empty_range = driver
        .get_range("docs/hello.txt", 0, Some(0))
        .await
        .unwrap();
    let mut empty_range_body = Vec::new();
    empty_range
        .read_to_end(&mut empty_range_body)
        .await
        .unwrap();
    assert!(empty_range_body.is_empty());

    let mut range = driver
        .get_range("docs/hello.txt", 6, Some(4))
        .await
        .unwrap();
    let mut range_body = Vec::new();
    range.read_to_end(&mut range_body).await.unwrap();
    assert_eq!(range_body, b"sftp");

    let mut tail = driver.get_range("docs/hello.txt", 11, None).await.unwrap();
    let mut tail_body = Vec::new();
    tail.read_to_end(&mut tail_body).await.unwrap();
    assert_eq!(tail_body, b"world");

    driver
        .copy_object("docs/hello.txt", "docs/copied.txt")
        .await
        .unwrap();
    assert_eq!(driver.get("docs/copied.txt").await.unwrap(), data);

    driver
        .put_reader(
            "stream/reader.bin",
            Box::new(std::io::Cursor::new(b"stream upload".to_vec())),
            13,
        )
        .await
        .unwrap();
    assert_eq!(
        driver.get("stream/reader.bin").await.unwrap(),
        b"stream upload"
    );

    driver
        .put("stream/attempt.bin", b"old object")
        .await
        .unwrap();
    let attempt = StreamUploadAttempt::new("stream/attempt.bin", 11).unwrap();
    driver
        .stage_attempt(&attempt, Box::new(Cursor::new(b"new content".to_vec())))
        .await
        .unwrap();
    assert_eq!(
        driver.get("stream/attempt.bin").await.unwrap(),
        b"old object"
    );
    driver.commit_attempt(&attempt).await.unwrap();
    assert_eq!(
        driver.get("stream/attempt.bin").await.unwrap(),
        b"new content"
    );
    assert!(!driver.exists(&attempt.staging_path).await.unwrap());

    let aborted = StreamUploadAttempt::new("stream/attempt.bin", 6).unwrap();
    driver
        .stage_attempt(&aborted, Box::new(Cursor::new(b"junk!!".to_vec())))
        .await
        .unwrap();
    assert_eq!(
        driver.abort_attempt(&aborted).await.unwrap(),
        StreamUploadCleanup::Cleaned
    );
    assert!(!driver.exists(&aborted.staging_path).await.unwrap());
    assert_eq!(
        driver.get("stream/attempt.bin").await.unwrap(),
        b"new content"
    );

    driver
        .put("stream/truncated.bin", b"previous version")
        .await
        .unwrap();
    let truncated_error = driver
        .put_reader(
            "stream/truncated.bin",
            Box::new(TruncatedReader {
                prefix: Cursor::new(b"partial".to_vec()),
            }),
            64,
        )
        .await
        .expect_err("a clean EOF before declared size must fail");
    assert_eq!(truncated_error.kind(), StorageErrorKind::Precondition);
    assert_eq!(
        driver.get("stream/truncated.bin").await.unwrap(),
        b"previous version"
    );

    let temp_dir = std::env::temp_dir().join(format!("asterdrive-sftp-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).expect("create temp dir");
    let local_upload = temp_dir.join("upload.bin");
    std::fs::write(&local_upload, b"file upload").expect("write local upload");
    driver
        .put_file(
            "stream/from-file.bin",
            local_upload
                .to_str()
                .expect("temp upload path should be valid utf-8"),
        )
        .await
        .unwrap();
    assert_eq!(
        driver.get("stream/from-file.bin").await.unwrap(),
        b"file upload"
    );
    let _ = std::fs::remove_dir_all(&temp_dir);

    driver.delete("docs/hello.txt").await.unwrap();
    assert!(!driver.exists("docs/hello.txt").await.unwrap());
}

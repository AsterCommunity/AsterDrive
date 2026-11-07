use super::*;
use async_trait::async_trait;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::AsyncRead;

struct SparseVideo {
    prefix: Vec<u8>,
    tail: Vec<u8>,
    size: u64,
    requests: AtomicU64,
    bytes: AtomicU64,
}

#[async_trait]
impl StorageDriver for SparseVideo {
    async fn get_range(
        &self,
        key: &str,
        offset: u64,
        length: Option<u64>,
    ) -> aster_drive_storage::Result<Box<dyn AsyncRead + Unpin + Send>> {
        assert_eq!(key, "fixed/video-object");
        let length = length.unwrap().min(self.size - offset);
        assert!(length <= 256 * 1024);
        self.requests.fetch_add(1, Ordering::SeqCst);
        self.bytes.fetch_add(length, Ordering::SeqCst);
        let mut data = vec![0u8; usize::try_from(length).unwrap()];
        for (start, source) in [
            (0u64, &self.prefix),
            (
                self.size - u64::try_from(self.tail.len()).unwrap(),
                &self.tail,
            ),
        ] {
            let end = start + u64::try_from(source.len()).unwrap();
            let from = offset.max(start);
            let to = (offset + length).min(end);
            if from < to {
                let output = usize::try_from(from - offset).unwrap();
                let input = usize::try_from(from - start).unwrap();
                let count = usize::try_from(to - from).unwrap();
                data[output..output + count].copy_from_slice(&source[input..input + count]);
            }
        }
        Ok(Box::new(std::io::Cursor::new(data)))
    }
    fn supports_efficient_range(&self) -> bool {
        true
    }
    async fn get_stream(
        &self,
        _: &str,
    ) -> aster_drive_storage::Result<Box<dyn AsyncRead + Unpin + Send>> {
        panic!("efficient media source must use Range")
    }
    async fn get(&self, _: &str) -> aster_drive_storage::Result<Vec<u8>> {
        panic!("media source must not buffer the object")
    }
    async fn put(&self, _: &str, _: &[u8]) -> aster_drive_storage::Result<String> {
        unreachable!()
    }
    async fn delete(&self, _: &str) -> aster_drive_storage::Result<()> {
        unreachable!()
    }
    async fn exists(&self, _: &str) -> aster_drive_storage::Result<bool> {
        Ok(true)
    }
    async fn metadata(
        &self,
        _: &str,
    ) -> aster_drive_storage::Result<aster_drive_storage::BlobMetadata> {
        Ok(aster_drive_storage::BlobMetadata {
            size: self.size,
            content_type: Some("video/mp4".into()),
        })
    }
}

fn blob(size: u64) -> file_blob::Model {
    file_blob::Model {
        id: 1,
        hash: "video-hash".into(),
        size: i64::try_from(size).unwrap(),
        policy_id: 1,
        storage_path: Some("fixed/video-object".into()),
        backing: aster_drive_model::types::file_blob::FileBlobBacking::Stored,
        thumbnail_path: None,
        thumbnail_processor: None,
        thumbnail_version: None,
        ref_count: 1,
        cleanup_pending_since: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

fn fixture() -> Option<(TempDirGuard, Vec<u8>)> {
    if !crate::config::media_processing::command_is_available("ffmpeg")
        || !crate::config::media_processing::command_is_available("ffprobe")
    {
        eprintln!("FFmpeg/ffprobe not installed; normal CLI compatibility check skipped");
        return None;
    }
    let root = std::env::temp_dir().join(format!("aster-media-range-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let guard = TempDirGuard::new(root, "media Range compatibility");
    let path = guard.path().join("video.mp4");
    let output = std::process::Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=size=16x16:rate=1",
            "-t",
            "1",
            "-c:v",
            "mpeg4",
            "-threads",
            "1",
            "-y",
        ])
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Some((guard, std::fs::read(path).unwrap()))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remote_video_seeks_to_tail_using_ranges_without_disk_staging_or_full_download() {
    // Verify the actual subprocess cannot inherit no_proxy=* and bypass the
    // object reader. Keep process environment mutation isolated to this child.
    const CHILD: &str = "ASTER_MEDIA_RANGE_PROXY_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "services::media::processing::ffmpeg_input::tests::remote_video_seeks_to_tail_using_ranges_without_disk_staging_or_full_download", "--nocapture"])
            .env(CHILD, "1").env("no_proxy", "*").env("NO_PROXY", "*")
            .env("http_proxy", "http://127.0.0.1:1").env("HTTPS_PROXY", "http://127.0.0.1:1")
            .output().unwrap();
        eprintln!("{}", String::from_utf8_lossy(&result.stderr));
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stdout)
        );
        return;
    }
    let Some((guard, bytes)) = fixture() else {
        return;
    };
    let moov = bytes.windows(4).position(|tag| tag == b"moov").unwrap() - 4;
    let mdat = bytes.windows(4).position(|tag| tag == b"mdat").unwrap();
    assert!(moov > mdat);
    let free_bytes = 64 * 1024 * 1024u32;
    let mut prefix = bytes[..moov].to_vec();
    prefix.extend_from_slice(&free_bytes.to_be_bytes());
    prefix.extend_from_slice(b"free");
    let driver = Arc::new(SparseVideo {
        prefix,
        tail: bytes[moov..].to_vec(),
        size: u64::try_from(bytes.len()).unwrap() + u64::from(free_bytes),
        requests: AtomicU64::new(0),
        bytes: AtomicU64::new(0),
    });
    let temp_root = guard.path().join("unused-scratch");
    let temp_root = temp_root.to_str().unwrap();
    for command in ["ffmpeg", "ffprobe"] {
        let before_bytes = driver.bytes.load(Ordering::SeqCst);
        let before_requests = driver.requests.load(Ordering::SeqCst);
        let input = FfmpegInput::prepare(
            driver.clone(),
            &blob(driver.size),
            "video.mp4",
            "video/mp4",
            temp_root,
        )
        .await
        .unwrap();
        assert!(matches!(&input, FfmpegInput::Range(_)));
        let output = tokio::task::spawn_blocking(move || {
            let after = match command {
                "ffmpeg" => vec!["-frames:v", "1", "-f", "null", "-"],
                _ => vec!["-show_streams", "-of", "json"],
            };
            input.run(
                command,
                &["-v", "error"],
                &after,
                AsterError::validation_error,
            )
        })
        .await
        .unwrap()
        .unwrap();
        assert!(
            output.status.success(),
            "{command}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let read_bytes = driver.bytes.load(Ordering::SeqCst) - before_bytes;
        let requests = driver.requests.load(Ordering::SeqCst) - before_requests;
        assert!(
            read_bytes < 8 * 1024 * 1024,
            "{command} read {read_bytes} of {}",
            driver.size
        );
        assert!(requests < 32, "{command} used {requests} Range requests");
        eprintln!(
            "{command}: source={} bytes, Range bytes={read_bytes}, requests={requests}, staged bytes=0",
            driver.size
        );
    }
    assert!(!std::path::Path::new(temp_root).exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn local_tail_metadata_remains_seekable_without_copying_the_source() {
    let Some((guard, _)) = fixture() else {
        return;
    };
    let input = FfmpegInput::File {
        file: open_source(&guard.path().join("video.mp4")).unwrap(),
        _staging: None,
    };
    let output = tokio::task::spawn_blocking(move || {
        input.run(
            "ffprobe",
            &["-v", "error"],
            &["-show_streams", "-of", "json"],
            AsterError::validation_error,
        )
    })
    .await
    .unwrap()
    .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("mpeg4"));
}

#[tokio::test]
async fn range_input_releases_listener_when_its_operation_ends() {
    let driver = Arc::new(SparseVideo {
        prefix: vec![],
        tail: vec![],
        size: 100,
        requests: AtomicU64::new(0),
        bytes: AtomicU64::new(0),
    });
    let input = ObjectRangeInput::start(driver.clone(), "fixed/video-object", 100)
        .await
        .unwrap();
    let address = input.proxy.strip_prefix("http://").unwrap().to_owned();
    assert!(tokio::net::TcpStream::connect(&address).await.is_ok());
    drop(input);
    tokio::task::yield_now().await;
    assert!(tokio::net::TcpStream::connect(&address).await.is_err());
    assert_eq!(driver.requests.load(Ordering::SeqCst), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn proxy_rejects_other_urls_methods_and_malformed_ranges_before_storage_io() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let driver = Arc::new(SparseVideo {
        prefix: b"0123456789".to_vec(),
        tail: vec![],
        size: 10,
        requests: AtomicU64::new(0),
        bytes: AtomicU64::new(0),
    });
    let source = ObjectRangeInput::start(driver.clone(), "fixed/video-object", 10)
        .await
        .unwrap();
    for request in [
        "GET http://127.0.0.1/private HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n".to_string(),
        format!(
            "GET {}/other HTTP/1.1\r\nHost: media.invalid\r\n\r\n",
            source.url
        ),
        "CONNECT 127.0.0.1:80 HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n".to_string(),
        format!(
            "POST {} HTTP/1.1\r\nHost: media.invalid\r\n\r\n",
            source.url
        ),
        format!(
            "GET {} HTTP/1.1\r\nRange: bytes=0-1,3-4\r\n\r\n",
            source.url
        ),
        format!("GET {} HTTP/1.1\r\nRange: bytes=99-\r\n\r\n", source.url),
        format!(
            "GET {} HTTP/1.1\r\nRange: bytes=0-1\r\nRange: bytes=1-2\r\n\r\n",
            source.url
        ),
    ] {
        let mut socket =
            tokio::net::TcpStream::connect(source.proxy.strip_prefix("http://").unwrap())
                .await
                .unwrap();
        socket.write_all(request.as_bytes()).await.unwrap();
        let mut response = Vec::new();
        socket.read_to_end(&mut response).await.unwrap();
        assert!(
            response.starts_with(b"HTTP/1.1 403"),
            "{}",
            String::from_utf8_lossy(&response)
        );
    }
    assert_eq!(driver.requests.load(Ordering::SeqCst), 0);
    let mut socket = tokio::net::TcpStream::connect(source.proxy.strip_prefix("http://").unwrap())
        .await
        .unwrap();
    socket
        .write_all(
            format!(
                "HEAD {} HTTP/1.1\r\nHost: media.invalid\r\n\r\n",
                source.url
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    let mut response = Vec::new();
    socket.read_to_end(&mut response).await.unwrap();
    assert!(response.starts_with(b"HTTP/1.1 200"));
    assert_eq!(driver.requests.load(Ordering::SeqCst), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remote_reference_manifests_cannot_open_network_or_local_secondary_sources() {
    let Some((guard, video)) = fixture() else {
        return;
    };
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let canary = guard.path().join("private.mp4");
    std::fs::write(&canary, video).unwrap();
    let url = format!("http://{}/canary", listener.local_addr().unwrap());
    for payload in [
        format!("#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXTINF:1,\n{url}\n#EXT-X-ENDLIST\n"),
        format!("ffconcat version 1.0\nfile '{url}'\n"),
        format!("ffconcat version 1.0\nfile '{}'\n", canary.display()),
        format!(
            "<MPD xmlns=\"urn:mpeg:dash:schema:mpd:2011\"><Period><AdaptationSet><Representation><BaseURL>{url}</BaseURL></Representation></AdaptationSet></Period></MPD>"
        ),
    ] {
        let driver = Arc::new(SparseVideo {
            prefix: payload.into_bytes(),
            tail: vec![],
            size: 1024,
            requests: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
        });
        for command in ["ffmpeg", "ffprobe"] {
            let input = FfmpegInput::prepare(
                driver.clone(),
                &blob(driver.size),
                "fake.mp4",
                "video/mp4",
                guard.path().to_str().unwrap(),
            )
            .await
            .unwrap();
            let output = tokio::task::spawn_blocking(move || {
                let after = match command {
                    "ffmpeg" => vec!["-frames:v", "1", "-f", "null", "-"],
                    _ => vec!["-show_streams", "-of", "json"],
                };
                input.run(
                    command,
                    &["-v", "error"],
                    &after,
                    AsterError::validation_error,
                )
            })
            .await
            .unwrap()
            .unwrap();
            assert!(
                !output.status.success(),
                "manifest unexpectedly accepted by {command}"
            );
            assert_eq!(
                listener.accept().unwrap_err().kind(),
                std::io::ErrorKind::WouldBlock
            );
        }
    }
}

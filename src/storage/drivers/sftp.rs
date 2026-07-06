//! Storage driver implementation for `sftp`.

use async_trait::async_trait;
use russh::client::{self, Handler};
use russh::keys::{HashAlg, PublicKey};
use russh_sftp::client::{Config as SftpClientConfig, SftpSession, error::Error as SftpError};
use russh_sftp::protocol::StatusCode;
use std::io::SeekFrom;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeekExt, AsyncWriteExt, ReadBuf};

use crate::entities::storage_policy;
use crate::errors::{AsterError, Result};
use crate::storage::error::{
    StorageErrorContext, StorageErrorKind, storage_driver_error, storage_driver_error_with_context,
};
use crate::storage::{BlobMetadata, StorageDriver, StreamUploadDriver};
use crate::types::parse_storage_policy_options;

const DEFAULT_SFTP_PORT: u16 = 22;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const IO_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
struct SftpEndpoint {
    host: String,
    port: u16,
}

#[derive(Debug, Clone)]
pub struct SftpDriver {
    endpoint: SftpEndpoint,
    username: String,
    password: String,
    base_path: String,
    host_key_fingerprint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SftpHostKeyRejection {
    pub expected: Option<String>,
    pub actual: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum HostKeyRejection {
    MissingPin { actual: String },
    Mismatch { expected: String, actual: String },
}

impl HostKeyRejection {
    fn expected(&self) -> Option<&str> {
        match self {
            Self::MissingPin { .. } => None,
            Self::Mismatch { expected, .. } => Some(expected),
        }
    }

    fn actual(&self) -> &str {
        match self {
            Self::MissingPin { actual } | Self::Mismatch { actual, .. } => actual,
        }
    }
}

#[derive(Clone)]
struct TrustServerKeyClient {
    expected_fingerprint: Option<String>,
    rejection: Arc<Mutex<Option<HostKeyRejection>>>,
}

impl Handler for TrustServerKeyClient {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKey,
    ) -> std::result::Result<bool, Self::Error> {
        let actual = host_key_fingerprint(server_public_key);
        let Some(expected) = self.expected_fingerprint.as_deref() else {
            record_host_key_rejection(&self.rejection, HostKeyRejection::MissingPin { actual });
            return Ok(false);
        };

        if host_key_fingerprint_matches(expected, &actual) {
            return Ok(true);
        }

        record_host_key_rejection(
            &self.rejection,
            HostKeyRejection::Mismatch {
                expected: normalize_host_key_fingerprint(expected),
                actual,
            },
        );
        Ok(false)
    }
}

struct SftpConnection {
    _ssh: client::Handle<TrustServerKeyClient>,
    sftp: SftpSession,
}

struct SftpFileReader {
    _connection: SftpConnection,
    file: russh_sftp::client::fs::File,
}

impl AsyncRead for SftpFileReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.file).poll_read(cx, buf)
    }
}

impl SftpDriver {
    pub fn validate_policy(policy: &storage_policy::Model) -> Result<()> {
        Self::validate_connection_parts(
            &policy.endpoint,
            &policy.access_key,
            &policy.secret_key,
            &policy.base_path,
        )
    }

    pub(crate) fn validate_connection_parts(
        endpoint: &str,
        username: &str,
        password: &str,
        base_path: &str,
    ) -> Result<()> {
        parse_sftp_endpoint(endpoint)?;
        validate_connection_secret(username, "username")?;
        validate_connection_secret(password, "password")?;
        normalize_remote_base_path(base_path)?;
        Ok(())
    }

    pub(crate) fn normalize_endpoint(endpoint: &str) -> Result<String> {
        let endpoint = endpoint.trim();
        parse_sftp_endpoint(endpoint)?;
        Ok(endpoint.to_string())
    }

    pub fn new(policy: &storage_policy::Model) -> Result<Self> {
        Self::validate_policy(policy)?;
        Ok(Self {
            endpoint: parse_sftp_endpoint(&policy.endpoint)?,
            username: policy.access_key.clone(),
            password: policy.secret_key.clone(),
            base_path: normalize_remote_base_path(&policy.base_path)?,
            host_key_fingerprint: parse_storage_policy_options(policy.options.as_ref())
                .sftp_host_key_fingerprint,
        })
    }

    pub fn validate_host_key_fingerprint(fingerprint: &str) -> Result<()> {
        let normalized = normalize_host_key_fingerprint(fingerprint);
        if !is_valid_host_key_fingerprint(&normalized) {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "SFTP host key fingerprint must use the SHA256:<base64> format",
            ));
        }
        Ok(())
    }

    pub fn host_key_rejection(error: &AsterError) -> Option<SftpHostKeyRejection> {
        let StorageErrorContext::SftpHostKeyRejected { expected, actual } =
            error.storage_error_context()?;
        Some(SftpHostKeyRejection {
            expected: expected.clone(),
            actual: actual.clone(),
        })
    }

    async fn connect(&self) -> Result<SftpConnection> {
        let mut config = russh::client::Config::default();
        config.inactivity_timeout = Some(IO_TIMEOUT);
        config.keepalive_interval = Some(Duration::from_secs(10));
        config.nodelay = true;

        let address = (self.endpoint.host.clone(), self.endpoint.port);
        let host_key_rejection = Arc::new(Mutex::new(None));
        let client = TrustServerKeyClient {
            expected_fingerprint: self.host_key_fingerprint.clone(),
            rejection: Arc::clone(&host_key_rejection),
        };
        let mut ssh = timeout_io(
            "connect SFTP endpoint",
            CONNECT_TIMEOUT,
            russh::client::connect(Arc::new(config), address, client),
        )
        .await?
        .map_err(|error| {
            host_key_rejection_error(&self.endpoint, &host_key_rejection)
                .unwrap_or_else(|| map_ssh_error("connect SFTP endpoint failed", error))
        })?;

        let auth = timeout_io(
            "SFTP authentication",
            IO_TIMEOUT,
            ssh.authenticate_password(self.username.clone(), self.password.clone()),
        )
        .await?
        .map_err(|error| map_ssh_error("SFTP authentication failed", error))?;
        if !auth.success() {
            return Err(storage_driver_error(
                StorageErrorKind::Auth,
                "SFTP authentication failed",
            ));
        }

        let channel = timeout_io(
            "open SSH session channel",
            IO_TIMEOUT,
            ssh.channel_open_session(),
        )
        .await?
        .map_err(|error| map_ssh_error("open SSH session channel failed", error))?;
        timeout_io(
            "open SFTP subsystem",
            IO_TIMEOUT,
            channel.request_subsystem(true, "sftp"),
        )
        .await?
        .map_err(|error| map_ssh_error("open SFTP subsystem failed", error))?;

        let sftp_config = SftpClientConfig {
            request_timeout_secs: IO_TIMEOUT.as_secs(),
            ..Default::default()
        };
        let sftp = timeout_io(
            "initialize SFTP session",
            IO_TIMEOUT,
            SftpSession::new_with_config(channel.into_stream(), sftp_config),
        )
        .await?
        .map_err(|error| map_sftp_error("initialize SFTP session failed", error))?;
        sftp.set_timeout(IO_TIMEOUT.as_secs());

        Ok(SftpConnection { _ssh: ssh, sftp })
    }

    fn full_path(&self, path: &str) -> Result<String> {
        let relative = sanitize_relative_storage_path(path)?;
        join_remote_path(&self.base_path, &relative)
    }

    async fn open_reader(&self, path: &str, offset: u64) -> Result<SftpFileReader> {
        let remote_path = self.full_path(path)?;
        let connection = self.connect().await?;
        let mut file = connection
            .sftp
            .open(remote_path)
            .await
            .map_err(|error| map_sftp_error("SFTP open failed", error))?;
        if offset > 0 {
            file.seek(SeekFrom::Start(offset))
                .await
                .map_err(|error| map_io_error("SFTP seek failed", error))?;
        }
        Ok(SftpFileReader {
            _connection: connection,
            file,
        })
    }
}

#[async_trait]
impl StorageDriver for SftpDriver {
    async fn put(&self, path: &str, data: &[u8]) -> Result<String> {
        let remote_path = self.full_path(path)?;
        let connection = self.connect().await?;
        ensure_remote_parent_dir(&connection.sftp, &remote_path).await?;
        let mut file = connection
            .sftp
            .create(remote_path)
            .await
            .map_err(|error| map_sftp_error("SFTP create failed", error))?;
        file.write_all(data)
            .await
            .map_err(|error| map_io_error("SFTP write failed", error))?;
        file.flush()
            .await
            .map_err(|error| map_io_error("SFTP flush failed", error))?;
        file.shutdown()
            .await
            .map_err(|error| map_io_error("SFTP close failed", error))?;
        Ok(path.to_string())
    }

    async fn get(&self, path: &str) -> Result<Vec<u8>> {
        let remote_path = self.full_path(path)?;
        let connection = self.connect().await?;
        connection
            .sftp
            .read(remote_path)
            .await
            .map_err(|error| map_sftp_error("SFTP read failed", error))
    }

    async fn get_stream(&self, path: &str) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        Ok(Box::new(self.open_reader(path, 0).await?))
    }

    async fn get_range(
        &self,
        path: &str,
        offset: u64,
        length: Option<u64>,
    ) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        if length == Some(0) {
            return Ok(Box::new(tokio::io::empty()));
        }

        let reader = self.open_reader(path, offset).await?;
        Ok(match length {
            Some(len) => Box::new(reader.take(len)),
            None => Box::new(reader),
        })
    }

    fn supports_efficient_range(&self) -> bool {
        true
    }

    async fn delete(&self, path: &str) -> Result<()> {
        let remote_path = self.full_path(path)?;
        let connection = self.connect().await?;
        connection
            .sftp
            .remove_file(remote_path)
            .await
            .map_err(|error| map_sftp_error("SFTP delete failed", error))
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        let remote_path = self.full_path(path)?;
        let connection = self.connect().await?;
        match connection.sftp.metadata(remote_path).await {
            Ok(_) => Ok(true),
            Err(error) if is_sftp_not_found(&error) => Ok(false),
            Err(error) => Err(map_sftp_error("SFTP stat failed", error)),
        }
    }

    async fn metadata(&self, path: &str) -> Result<BlobMetadata> {
        let remote_path = self.full_path(path)?;
        let connection = self.connect().await?;
        let stat = connection
            .sftp
            .metadata(remote_path)
            .await
            .map_err(|error| map_sftp_error("SFTP stat failed", error))?;
        Ok(BlobMetadata {
            size: stat.size.unwrap_or(0),
            content_type: None,
        })
    }

    async fn copy_object(&self, src_path: &str, dest_path: &str) -> Result<String> {
        let src_remote_path = self.full_path(src_path)?;
        let dest_remote_path = self.full_path(dest_path)?;
        let connection = self.connect().await?;
        ensure_remote_parent_dir(&connection.sftp, &dest_remote_path).await?;
        let mut src = connection
            .sftp
            .open(src_remote_path)
            .await
            .map_err(|error| map_sftp_error("SFTP source open failed", error))?;
        let mut dest = connection
            .sftp
            .create(dest_remote_path)
            .await
            .map_err(|error| map_sftp_error("SFTP destination create failed", error))?;
        tokio::io::copy(&mut src, &mut dest)
            .await
            .map_err(|error| map_io_error("SFTP copy failed", error))?;
        dest.flush()
            .await
            .map_err(|error| map_io_error("SFTP copy flush failed", error))?;
        dest.shutdown()
            .await
            .map_err(|error| map_io_error("SFTP copy close failed", error))?;
        Ok(dest_path.to_string())
    }

    fn as_stream_upload(&self) -> Option<&dyn StreamUploadDriver> {
        Some(self)
    }
}

#[async_trait]
impl StreamUploadDriver for SftpDriver {
    async fn put_reader(
        &self,
        storage_path: &str,
        mut reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        _size: i64,
    ) -> Result<String> {
        let remote_path = self.full_path(storage_path)?;
        let connection = self.connect().await?;
        ensure_remote_parent_dir(&connection.sftp, &remote_path).await?;
        let mut remote_file = connection
            .sftp
            .create(remote_path)
            .await
            .map_err(|error| map_sftp_error("SFTP create failed", error))?;
        tokio::io::copy(&mut reader, &mut remote_file)
            .await
            .map_err(|error| map_io_error("SFTP stream upload failed", error))?;
        remote_file
            .flush()
            .await
            .map_err(|error| map_io_error("SFTP stream flush failed", error))?;
        remote_file
            .shutdown()
            .await
            .map_err(|error| map_io_error("SFTP stream close failed", error))?;
        Ok(storage_path.to_string())
    }

    async fn put_file(&self, storage_path: &str, local_path: &str) -> Result<String> {
        let local_file = tokio::fs::File::open(local_path)
            .await
            .map_err(|error| map_io_error("open local upload file failed", error))?;
        self.put_reader(storage_path, Box::new(local_file), -1)
            .await
    }
}

fn host_key_fingerprint(public_key: &PublicKey) -> String {
    public_key.fingerprint(HashAlg::Sha256).to_string()
}

fn normalize_host_key_fingerprint(fingerprint: &str) -> String {
    let trimmed = fingerprint.trim();
    trimmed
        .strip_prefix("sha256:")
        .map(|value| format!("SHA256:{value}"))
        .unwrap_or_else(|| trimmed.to_string())
}

fn is_valid_host_key_fingerprint(fingerprint: &str) -> bool {
    fingerprint
        .strip_prefix("SHA256:")
        .is_some_and(|value| !value.is_empty() && !value.chars().any(char::is_whitespace))
}

fn host_key_fingerprint_matches(expected: &str, actual: &str) -> bool {
    normalize_host_key_fingerprint(expected) == normalize_host_key_fingerprint(actual)
}

fn record_host_key_rejection(
    rejection: &Arc<Mutex<Option<HostKeyRejection>>>,
    value: HostKeyRejection,
) {
    match rejection.lock() {
        Ok(mut guard) => {
            *guard = Some(value);
        }
        Err(error) => {
            tracing::warn!("failed to record SFTP host key rejection: {error}");
        }
    }
}

fn host_key_rejection_error(
    endpoint: &SftpEndpoint,
    rejection: &Arc<Mutex<Option<HostKeyRejection>>>,
) -> Option<AsterError> {
    let rejection = rejection.lock().ok()?.clone()?;
    let context = StorageErrorContext::SftpHostKeyRejected {
        expected: rejection.expected().map(ToOwned::to_owned),
        actual: rejection.actual().to_string(),
    };
    Some(match rejection {
        HostKeyRejection::MissingPin { actual } => storage_driver_error_with_context(
            StorageErrorKind::Precondition,
            format!(
                "SFTP host key is not trusted for {}:{}. Confirm fingerprint {actual} and save it as sftp_host_key_fingerprint before testing again",
                endpoint.host, endpoint.port
            ),
            context,
        ),
        HostKeyRejection::Mismatch { expected, actual } => storage_driver_error_with_context(
            StorageErrorKind::Precondition,
            format!(
                "SFTP host key mismatch for {}:{}. Expected {expected}, got {actual}. Verify the server identity before updating sftp_host_key_fingerprint",
                endpoint.host, endpoint.port
            ),
            context,
        ),
    })
}

async fn timeout_io<T, F>(context: &'static str, duration: Duration, future: F) -> Result<T>
where
    F: std::future::Future<Output = T>,
{
    tokio::time::timeout(duration, future).await.map_err(|_| {
        storage_driver_error(
            StorageErrorKind::Transient,
            format!("{context}: timed out after {}s", duration.as_secs()),
        )
    })
}

async fn ensure_remote_parent_dir(sftp: &SftpSession, remote_path: &str) -> Result<()> {
    let Some(parent) = remote_parent_dir(remote_path) else {
        return Ok(());
    };
    ensure_remote_dir(sftp, &parent).await
}

fn remote_parent_dir(remote_path: &str) -> Option<String> {
    let trimmed = remote_path.trim_end_matches('/');
    let index = trimmed.rfind('/')?;
    if index == 0 {
        Some("/".to_string())
    } else {
        Some(trimmed[..index].to_string())
    }
}

async fn ensure_remote_dir(sftp: &SftpSession, dir: &str) -> Result<()> {
    if dir.is_empty() || dir == "." || dir == "/" {
        return Ok(());
    }

    let is_absolute = dir.starts_with('/');
    let (_, segments) = sanitize_remote_path_segments(dir, true)?;
    let mut current = String::new();
    for segment in segments {
        if current.is_empty() {
            current = if is_absolute {
                format!("/{segment}")
            } else {
                segment
            };
        } else if current == "/" {
            current = format!("/{segment}");
        } else {
            current.push('/');
            current.push_str(&segment);
        }

        match sftp.create_dir(current.clone()).await {
            Ok(()) => {}
            Err(error) => match sftp.metadata(current.clone()).await {
                Ok(metadata) if metadata.file_type().is_dir() => {}
                Ok(_) => {
                    return Err(storage_driver_error(
                        StorageErrorKind::Misconfigured,
                        format!("SFTP mkdir failed: {current} exists and is not a directory"),
                    ));
                }
                Err(_) => return Err(map_sftp_error("SFTP mkdir failed", error)),
            },
        }
    }
    Ok(())
}

fn parse_sftp_endpoint(endpoint: &str) -> Result<SftpEndpoint> {
    let endpoint = endpoint.trim();
    if endpoint.is_empty() {
        return Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            "SFTP endpoint is required",
        ));
    }

    let url_text = if endpoint.contains("://") {
        endpoint.to_string()
    } else {
        format!("sftp://{endpoint}")
    };
    let url = url::Url::parse(&url_text).map_err(|error| {
        storage_driver_error(
            StorageErrorKind::Misconfigured,
            format!("invalid SFTP endpoint: {error}"),
        )
    })?;

    if url.scheme() != "sftp" {
        return Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            "SFTP endpoint must use the sftp scheme",
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            "SFTP endpoint must not contain credentials; use access_key and secret_key",
        ));
    }
    if url.path() != "/" && !url.path().is_empty() {
        return Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            "SFTP endpoint path is not supported; use base_path for the remote root",
        ));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            "SFTP endpoint must not contain query or fragment",
        ));
    }

    let host = url.host_str().ok_or_else(|| {
        storage_driver_error(
            StorageErrorKind::Misconfigured,
            "SFTP endpoint host is required",
        )
    })?;
    let port = url.port().unwrap_or(DEFAULT_SFTP_PORT);

    Ok(SftpEndpoint {
        host: host
            .strip_prefix('[')
            .and_then(|host| host.strip_suffix(']'))
            .unwrap_or(host)
            .to_string(),
        port,
    })
}

fn validate_connection_secret(value: &str, field: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(storage_driver_error(
            StorageErrorKind::Auth,
            format!("{field} is required for SFTP storage policies"),
        ));
    }
    Ok(())
}

fn sanitize_remote_path_segments(path: &str, allow_absolute: bool) -> Result<(bool, Vec<String>)> {
    if path.contains('\\') || path.contains('\0') {
        return Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            format!("invalid SFTP path: {path}"),
        ));
    }

    let is_absolute = path.starts_with('/');
    if is_absolute && !allow_absolute {
        return Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            format!("SFTP object path must be relative: {path}"),
        ));
    }

    let mut segments = Vec::new();
    for segment in path.split('/') {
        let segment = segment.trim();
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                format!("SFTP path escapes base path: {path}"),
            ));
        }
        segments.push(segment.to_string());
    }

    Ok((is_absolute, segments))
}

fn sanitize_relative_storage_path(path: &str) -> Result<String> {
    let (_, segments) = sanitize_remote_path_segments(path.trim_start_matches('/'), false)?;
    Ok(segments.join("/"))
}

fn normalize_remote_base_path(path: &str) -> Result<String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    let (is_absolute, segments) = sanitize_remote_path_segments(trimmed, true)?;
    if segments.is_empty() {
        return Ok(if is_absolute {
            "/".to_string()
        } else {
            String::new()
        });
    }
    let normalized = segments.join("/");
    Ok(if is_absolute {
        format!("/{normalized}")
    } else {
        normalized
    })
}

fn join_remote_path(base_path: &str, relative_path: &str) -> Result<String> {
    if relative_path.is_empty() {
        if base_path.is_empty() {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "SFTP storage path cannot be empty",
            ));
        }
        return Ok(base_path.to_string());
    }

    Ok(if base_path.is_empty() {
        relative_path.to_string()
    } else if base_path == "/" {
        format!("/{relative_path}")
    } else {
        format!("{base_path}/{relative_path}")
    })
}

fn map_ssh_error(context: &'static str, error: russh::Error) -> AsterError {
    storage_driver_error(
        classify_error_message(&error.to_string()),
        format!("{context}: {error}"),
    )
}

fn map_sftp_error(context: &'static str, error: SftpError) -> AsterError {
    storage_driver_error(classify_sftp_error(&error), format!("{context}: {error}"))
}

fn map_io_error(context: &'static str, error: std::io::Error) -> AsterError {
    storage_driver_error(classify_io_error(&error), format!("{context}: {error}"))
}

fn classify_sftp_error(error: &SftpError) -> StorageErrorKind {
    match error {
        SftpError::Status(status) => match status.status_code {
            StatusCode::NoSuchFile => StorageErrorKind::NotFound,
            StatusCode::PermissionDenied => StorageErrorKind::Permission,
            StatusCode::NoConnection | StatusCode::ConnectionLost => StorageErrorKind::Transient,
            _ => classify_error_message(&status.error_message),
        },
        SftpError::Timeout => StorageErrorKind::Transient,
        SftpError::IO(message)
        | SftpError::Limited(message)
        | SftpError::UnexpectedBehavior(message) => classify_error_message(message),
        SftpError::UnexpectedPacket => StorageErrorKind::Unknown,
    }
}

fn classify_io_error(error: &std::io::Error) -> StorageErrorKind {
    match error.kind() {
        std::io::ErrorKind::NotFound => StorageErrorKind::NotFound,
        std::io::ErrorKind::PermissionDenied => StorageErrorKind::Permission,
        std::io::ErrorKind::TimedOut
        | std::io::ErrorKind::ConnectionRefused
        | std::io::ErrorKind::ConnectionReset
        | std::io::ErrorKind::ConnectionAborted
        | std::io::ErrorKind::BrokenPipe
        | std::io::ErrorKind::UnexpectedEof
        | std::io::ErrorKind::WouldBlock => StorageErrorKind::Transient,
        _ => classify_error_message(&error.to_string()),
    }
}

fn classify_error_message(message: &str) -> StorageErrorKind {
    let message = message.to_ascii_lowercase();
    if message.contains("no such file") || message.contains("not found") {
        StorageErrorKind::NotFound
    } else if message.contains("auth")
        || message.contains("password")
        || message.contains("permission denied (publickey,password")
    {
        StorageErrorKind::Auth
    } else if message.contains("permission denied") || message.contains("access denied") {
        StorageErrorKind::Permission
    } else if message.contains("connection")
        || message.contains("timed out")
        || message.contains("timeout")
        || message.contains("eof")
        || message.contains("closed")
        || message.contains("reset")
    {
        StorageErrorKind::Transient
    } else {
        StorageErrorKind::Unknown
    }
}

fn is_sftp_not_found(error: &SftpError) -> bool {
    matches!(
        error,
        SftpError::Status(status) if status.status_code == StatusCode::NoSuchFile
    ) || error
        .to_string()
        .to_ascii_lowercase()
        .contains("no such file")
}

#[cfg(test)]
mod tests {
    use super::{
        classify_sftp_error, host_key_fingerprint_matches, is_valid_host_key_fingerprint,
        join_remote_path, normalize_host_key_fingerprint, normalize_remote_base_path,
        parse_sftp_endpoint, sanitize_relative_storage_path,
    };
    use crate::storage::error::StorageErrorKind;
    use crate::storage::{StorageDriver, StreamUploadDriver};
    use crate::types::{DriverType, StoredStoragePolicyAllowedTypes};
    use chrono::Utc;
    use russh_sftp::client::error::Error as SftpError;
    use russh_sftp::protocol::{Status, StatusCode};
    use tokio::io::AsyncReadExt;

    #[test]
    fn parses_plain_sftp_endpoint_with_default_port() {
        let endpoint = parse_sftp_endpoint("example.com").unwrap();
        assert_eq!(endpoint.host, "example.com");
        assert_eq!(endpoint.port, 22);
    }

    #[test]
    fn parses_sftp_endpoint_with_explicit_port() {
        let endpoint = parse_sftp_endpoint("sftp://example.com:2222").unwrap();
        assert_eq!(endpoint.host, "example.com");
        assert_eq!(endpoint.port, 2222);
    }

    #[test]
    fn parses_ipv6_sftp_endpoint() {
        let endpoint = parse_sftp_endpoint("sftp://[::1]:2222").unwrap();
        assert_eq!(endpoint.host, "::1");
        assert_eq!(endpoint.port, 2222);
    }

    #[test]
    fn rejects_endpoint_credentials_paths_query_and_fragment() {
        assert!(parse_sftp_endpoint("").is_err());
        assert!(parse_sftp_endpoint("ftp://example.com").is_err());
        assert!(parse_sftp_endpoint("sftp://user@example.com").is_err());
        assert!(parse_sftp_endpoint("sftp://example.com/uploads").is_err());
        assert!(parse_sftp_endpoint("sftp://example.com?x=1").is_err());
        assert!(parse_sftp_endpoint("sftp://example.com#frag").is_err());
    }

    #[test]
    fn normalizes_remote_base_path() {
        assert_eq!(normalize_remote_base_path("").unwrap(), "");
        assert_eq!(normalize_remote_base_path("/").unwrap(), "/");
        assert_eq!(
            normalize_remote_base_path("/data//uploads/").unwrap(),
            "/data/uploads"
        );
        assert_eq!(
            normalize_remote_base_path("data/./uploads").unwrap(),
            "data/uploads"
        );
        assert!(normalize_remote_base_path("../data").is_err());
        assert!(normalize_remote_base_path("data\\uploads").is_err());
        assert!(normalize_remote_base_path("data\0uploads").is_err());
    }

    #[test]
    fn sanitizes_storage_path_as_relative_path() {
        assert_eq!(
            sanitize_relative_storage_path("/files/./blob.bin").unwrap(),
            "files/blob.bin"
        );
        assert!(sanitize_relative_storage_path("../blob.bin").is_err());
        assert!(sanitize_relative_storage_path("folder\\blob.bin").is_err());
        assert!(sanitize_relative_storage_path("folder\0blob.bin").is_err());
    }

    #[test]
    fn joins_base_and_relative_paths() {
        assert_eq!(join_remote_path("", "files/a.bin").unwrap(), "files/a.bin");
        assert_eq!(
            join_remote_path("/data", "files/a.bin").unwrap(),
            "/data/files/a.bin"
        );
        assert_eq!(
            join_remote_path("/", "files/a.bin").unwrap(),
            "/files/a.bin"
        );
        assert!(join_remote_path("", "").is_err());
        assert_eq!(join_remote_path("/data", "").unwrap(), "/data");
    }

    #[test]
    fn classifies_sftp_status_errors() {
        let status = |status_code, error_message: &str| {
            SftpError::Status(Status {
                id: 1,
                status_code,
                error_message: error_message.to_string(),
                language_tag: String::new(),
            })
        };

        assert_eq!(
            classify_sftp_error(&status(StatusCode::NoSuchFile, "missing")),
            StorageErrorKind::NotFound
        );
        assert_eq!(
            classify_sftp_error(&status(StatusCode::PermissionDenied, "denied")),
            StorageErrorKind::Permission
        );
        assert_eq!(
            classify_sftp_error(&status(StatusCode::ConnectionLost, "lost")),
            StorageErrorKind::Transient
        );
    }

    #[test]
    fn validates_sftp_host_key_fingerprint_format() {
        assert!(is_valid_host_key_fingerprint(
            &normalize_host_key_fingerprint("SHA256:abc123+/=")
        ));
        assert!(host_key_fingerprint_matches(
            "sha256:abc123+/=",
            "SHA256:abc123+/="
        ));
        assert!(!is_valid_host_key_fingerprint("MD5:aa:bb"));
        assert!(!is_valid_host_key_fingerprint("SHA256:"));
        assert!(!is_valid_host_key_fingerprint("SHA256:abc def"));
    }

    fn env_policy() -> Option<crate::entities::storage_policy::Model> {
        let endpoint = std::env::var("ASTER_SFTP_TEST_ENDPOINT").ok()?;
        let username = std::env::var("ASTER_SFTP_TEST_USERNAME").ok()?;
        let password = std::env::var("ASTER_SFTP_TEST_PASSWORD").ok()?;
        let base_path = std::env::var("ASTER_SFTP_TEST_BASE_PATH").ok()?;
        let host_key_fingerprint = std::env::var("ASTER_SFTP_TEST_HOST_KEY_FINGERPRINT").ok()?;
        Some(crate::entities::storage_policy::Model {
            id: 1,
            name: "sftp acceptance".to_string(),
            driver_type: DriverType::Sftp,
            endpoint,
            bucket: String::new(),
            access_key: username,
            secret_key: password,
            base_path,
            remote_node_id: None,
            remote_storage_target_key: None,
            max_file_size: 0,
            allowed_types: StoredStoragePolicyAllowedTypes::empty(),
            options: crate::types::serialize_storage_policy_options(
                &crate::types::StoragePolicyOptions {
                    sftp_host_key_fingerprint: Some(host_key_fingerprint),
                    ..Default::default()
                },
            )
            .expect("serialize SFTP host key options"),
            is_default: false,
            chunk_size: 1024,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
    }

    #[tokio::test]
    #[ignore = "requires ASTER_SFTP_TEST_* environment variables and a reachable SFTP server"]
    async fn real_sftp_driver_round_trip_uses_streaming_upload() {
        let Some(policy) = env_policy() else {
            eprintln!("skipping real SFTP test because ASTER_SFTP_TEST_* is not set");
            return;
        };
        let driver = super::SftpDriver::new(&policy).unwrap();
        let test_root = format!("codex-acceptance/{}", uuid::Uuid::new_v4());
        let object_path = format!("{test_root}/streamed.bin");
        let copy_path = format!("{test_root}/copied.bin");
        let payload = b"hello from asterdrive sftp streaming";

        driver
            .put_reader(
                &object_path,
                Box::new(std::io::Cursor::new(payload.to_vec())),
                payload.len() as i64,
            )
            .await
            .unwrap();

        assert!(driver.exists(&object_path).await.unwrap());
        assert_eq!(
            driver.metadata(&object_path).await.unwrap().size,
            payload.len() as u64
        );
        assert_eq!(driver.get(&object_path).await.unwrap(), payload);

        let mut range = driver.get_range(&object_path, 6, Some(4)).await.unwrap();
        let mut range_bytes = Vec::new();
        range.read_to_end(&mut range_bytes).await.unwrap();
        assert_eq!(range_bytes, b"from");

        driver.copy_object(&object_path, &copy_path).await.unwrap();
        assert_eq!(driver.get(&copy_path).await.unwrap(), payload);

        driver.delete(&object_path).await.unwrap();
        driver.delete(&copy_path).await.unwrap();
        assert!(!driver.exists(&object_path).await.unwrap());
    }
}

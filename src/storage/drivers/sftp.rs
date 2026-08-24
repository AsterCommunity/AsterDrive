//! Storage driver implementation for `sftp`.

use async_trait::async_trait;
use russh::client::{self, Handler};
use russh::keys::{HashAlg, PublicKey};
use russh_sftp::client::{
    Config as SftpClientConfig, RawSftpSession, SftpSession, error::Error as SftpError,
};
use russh_sftp::protocol::{Packet, StatusCode};
use std::io::SeekFrom;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeekExt, AsyncWriteExt, ReadBuf};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use aster_drive_storage::error::{
    Result, StorageError, StorageErrorContext, StorageErrorKind, storage_driver_error,
    storage_driver_error_with_context,
};
use aster_drive_storage::{
    BlobMetadata, StorageDriver, StreamUploadAttempt, StreamUploadCleanup, StreamUploadDriver,
};

const DEFAULT_SFTP_PORT: u16 = 22;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const IO_TIMEOUT: Duration = Duration::from_secs(30);
const SSH_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(10);
const POOL_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(30);
const POOLED_CONNECTION_IDLE_TTL: Duration = Duration::from_secs(60);
const DEFAULT_POOL_SIZE: usize = 4;

#[derive(Debug, Clone)]
struct SftpEndpoint {
    host: String,
    port: u16,
}

#[derive(Clone)]
pub struct SftpDriver {
    endpoint: SftpEndpoint,
    username: String,
    password: String,
    base_path: String,
    host_key_fingerprint: Option<String>,
    pool: Arc<SftpConnectionPool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SftpDriverConfig {
    pub endpoint: String,
    pub base_path: String,
    pub host_key_fingerprint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SftpStaticCredentials {
    pub username: String,
    pub password: String,
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
    ssh: client::Handle<TrustServerKeyClient>,
    sftp: SftpSession,
}

struct IdleSftpConnection {
    connection: SftpConnection,
    returned_at: Instant,
}

struct SftpConnectionPool {
    semaphore: Arc<Semaphore>,
    idle: Mutex<Vec<IdleSftpConnection>>,
    max_idle: usize,
    created_connections: AtomicUsize,
}

struct SftpConnectionLease {
    connection: Option<SftpConnection>,
    pool: Arc<SftpConnectionPool>,
    _permit: OwnedSemaphorePermit,
    reusable: bool,
}

struct SftpFileReader {
    file: russh_sftp::client::fs::File,
    connection: SftpConnectionLease,
}

#[cfg(debug_assertions)]
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SftpConnectionPoolSnapshot {
    pub idle_connections: usize,
    pub created_connections: usize,
}

impl AsyncRead for SftpFileReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let result = Pin::new(&mut self.file).poll_read(cx, buf);
        if matches!(result, Poll::Ready(Err(_))) {
            self.connection.discard();
        }
        result
    }
}

impl SftpConnectionPool {
    fn new(max_size: usize) -> Self {
        let max_size = max_size.max(1);
        Self {
            semaphore: Arc::new(Semaphore::new(max_size)),
            idle: Mutex::new(Vec::with_capacity(max_size)),
            max_idle: max_size,
            created_connections: AtomicUsize::new(0),
        }
    }

    async fn acquire(self: &Arc<Self>, driver: &SftpDriver) -> Result<SftpConnectionLease> {
        let permit = timeout_io(
            "acquire SFTP connection lease",
            POOL_ACQUIRE_TIMEOUT,
            self.semaphore.clone().acquire_owned(),
        )
        .await?
        .map_err(|error| {
            storage_driver_error(
                StorageErrorKind::Transient,
                format!("acquire SFTP connection lease failed: {error}"),
            )
        })?;

        let connection = if let Some(connection) = self.take_idle_connection() {
            connection
        } else {
            let connection = driver.connect_new_connection().await?;
            self.created_connections.fetch_add(1, Ordering::Relaxed);
            connection
        };

        Ok(SftpConnectionLease {
            connection: Some(connection),
            pool: Arc::clone(self),
            _permit: permit,
            reusable: false,
        })
    }

    fn take_idle_connection(&self) -> Option<SftpConnection> {
        let mut idle = match self.idle.lock() {
            Ok(guard) => guard,
            Err(error) => {
                tracing::warn!("failed to lock SFTP connection pool: {error}");
                return None;
            }
        };

        while let Some(connection) = idle.pop() {
            if connection.returned_at.elapsed() <= POOLED_CONNECTION_IDLE_TTL {
                return Some(connection.connection);
            }
        }
        None
    }

    fn return_connection(&self, connection: SftpConnection) {
        let mut idle = match self.idle.lock() {
            Ok(guard) => guard,
            Err(error) => {
                tracing::warn!("failed to return SFTP connection to pool: {error}");
                return;
            }
        };

        if idle.len() < self.max_idle {
            idle.push(IdleSftpConnection {
                connection,
                returned_at: Instant::now(),
            });
        }
    }

    #[cfg(debug_assertions)]
    fn snapshot(&self) -> SftpConnectionPoolSnapshot {
        let idle_connections = self.idle.lock().map(|idle| idle.len()).unwrap_or(0);
        SftpConnectionPoolSnapshot {
            idle_connections,
            created_connections: self.created_connections.load(Ordering::Relaxed),
        }
    }
}

impl SftpConnectionLease {
    fn sftp(&self) -> Result<&SftpSession> {
        self.connection
            .as_ref()
            .map(|connection| &connection.sftp)
            .ok_or_else(|| {
                storage_driver_error(StorageErrorKind::Unknown, "SFTP connection lease is empty")
            })
    }

    fn mark_reusable(&mut self) {
        self.reusable = true;
    }

    fn discard(&mut self) {
        self.reusable = false;
    }

    fn map_sftp_error(&mut self, context: &'static str, error: SftpError) -> StorageError {
        if is_sftp_connection_reusable_after_error(&error) {
            self.mark_reusable();
        }
        map_sftp_error(context, error)
    }
}

impl Drop for SftpConnectionLease {
    fn drop(&mut self) {
        if self.reusable
            && let Some(connection) = self.connection.take()
        {
            self.pool.return_connection(connection);
        }
    }
}

impl SftpDriver {
    pub fn validate_config(
        config: &SftpDriverConfig,
        credentials: &SftpStaticCredentials,
    ) -> Result<()> {
        parse_sftp_endpoint(&config.endpoint)?;
        validate_connection_secret(&credentials.username, "username")?;
        validate_connection_secret(&credentials.password, "password")?;
        normalize_remote_base_path(&config.base_path)?;
        if let Some(fingerprint) = config.host_key_fingerprint.as_deref() {
            Self::validate_host_key_fingerprint(fingerprint)?;
        }
        Ok(())
    }

    pub(crate) fn normalize_endpoint(endpoint: &str) -> Result<String> {
        let endpoint = endpoint.trim();
        parse_sftp_endpoint(endpoint)?;
        Ok(endpoint.to_string())
    }

    pub fn new(config: SftpDriverConfig, credentials: SftpStaticCredentials) -> Result<Self> {
        Self::validate_config(&config, &credentials)?;
        Ok(Self {
            endpoint: parse_sftp_endpoint(&config.endpoint)?,
            username: credentials.username,
            password: credentials.password,
            base_path: normalize_remote_base_path(&config.base_path)?,
            host_key_fingerprint: config
                .host_key_fingerprint
                .map(|value| normalize_host_key_fingerprint(&value)),
            pool: Arc::new(SftpConnectionPool::new(DEFAULT_POOL_SIZE)),
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

    pub fn host_key_rejection(error: &StorageError) -> Option<SftpHostKeyRejection> {
        let StorageErrorContext::SftpHostKeyRejected { expected, actual } = error.context()?;
        Some(SftpHostKeyRejection {
            expected: expected.clone(),
            actual: actual.clone(),
        })
    }

    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn debug_connection_pool_snapshot(&self) -> SftpConnectionPoolSnapshot {
        self.pool.snapshot()
    }

    async fn acquire_connection(&self) -> Result<SftpConnectionLease> {
        self.pool.acquire(self).await
    }

    async fn connect_new_connection(&self) -> Result<SftpConnection> {
        let config = russh::client::Config {
            inactivity_timeout: Some(IO_TIMEOUT),
            keepalive_interval: Some(SSH_KEEPALIVE_INTERVAL),
            nodelay: true,
            ..Default::default()
        };

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

        Ok(SftpConnection { ssh, sftp })
    }

    fn full_path(&self, path: &str) -> Result<String> {
        let relative = sanitize_relative_storage_path(path)?;
        join_remote_path(&self.base_path, &relative)
    }

    async fn open_reader(&self, path: &str, offset: u64) -> Result<SftpFileReader> {
        let remote_path = self.full_path(path)?;
        let mut connection = self.acquire_connection().await?;
        let mut file = connection
            .sftp()?
            .open(remote_path)
            .await
            .map_err(|error| connection.map_sftp_error("SFTP open failed", error))?;
        if offset > 0 {
            file.seek(SeekFrom::Start(offset))
                .await
                .map_err(|error| map_io_error("SFTP seek failed", error))?;
        }
        connection.mark_reusable();
        Ok(SftpFileReader { file, connection })
    }
}

#[async_trait]
impl StorageDriver for SftpDriver {
    async fn put(&self, path: &str, data: &[u8]) -> aster_drive_storage::Result<String> {
        let remote_path = self.full_path(path)?;
        let mut connection = self.acquire_connection().await?;
        ensure_remote_parent_dir(connection.sftp()?, &remote_path).await?;
        let mut file = connection
            .sftp()?
            .create(remote_path)
            .await
            .map_err(|error| connection.map_sftp_error("SFTP create failed", error))?;
        file.write_all(data)
            .await
            .map_err(|error| map_io_error("SFTP write failed", error))?;
        file.flush()
            .await
            .map_err(|error| map_io_error("SFTP flush failed", error))?;
        file.shutdown()
            .await
            .map_err(|error| map_io_error("SFTP close failed", error))?;
        connection.mark_reusable();
        Ok(path.to_string())
    }

    async fn get(&self, path: &str) -> aster_drive_storage::Result<Vec<u8>> {
        let remote_path = self.full_path(path)?;
        let mut connection = self.acquire_connection().await?;
        let data = connection
            .sftp()?
            .read(remote_path)
            .await
            .map_err(|error| connection.map_sftp_error("SFTP read failed", error))?;
        connection.mark_reusable();
        Ok(data)
    }

    async fn get_stream(
        &self,
        path: &str,
    ) -> aster_drive_storage::Result<Box<dyn AsyncRead + Unpin + Send>> {
        Ok(Box::new(self.open_reader(path, 0).await?))
    }

    async fn get_range(
        &self,
        path: &str,
        offset: u64,
        length: Option<u64>,
    ) -> aster_drive_storage::Result<Box<dyn AsyncRead + Unpin + Send>> {
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

    async fn delete(&self, path: &str) -> aster_drive_storage::Result<()> {
        let remote_path = self.full_path(path)?;
        let mut connection = self.acquire_connection().await?;
        connection
            .sftp()?
            .remove_file(remote_path)
            .await
            .map_err(|error| connection.map_sftp_error("SFTP delete failed", error))?;
        connection.mark_reusable();
        Ok(())
    }

    async fn exists(&self, path: &str) -> aster_drive_storage::Result<bool> {
        let remote_path = self.full_path(path)?;
        let mut connection = self.acquire_connection().await?;
        match connection.sftp()?.metadata(remote_path).await {
            Ok(_) => {
                connection.mark_reusable();
                Ok(true)
            }
            Err(error) if is_sftp_not_found(&error) => {
                connection.mark_reusable();
                Ok(false)
            }
            Err(error) => Err(connection.map_sftp_error("SFTP stat failed", error)),
        }
    }

    async fn metadata(&self, path: &str) -> aster_drive_storage::Result<BlobMetadata> {
        let remote_path = self.full_path(path)?;
        let mut connection = self.acquire_connection().await?;
        let stat = connection
            .sftp()?
            .metadata(remote_path)
            .await
            .map_err(|error| connection.map_sftp_error("SFTP stat failed", error))?;
        connection.mark_reusable();
        Ok(BlobMetadata {
            size: stat.size.unwrap_or(0),
            content_type: None,
        })
    }

    async fn copy_object(
        &self,
        src_path: &str,
        dest_path: &str,
    ) -> aster_drive_storage::Result<String> {
        let src_remote_path = self.full_path(src_path)?;
        let dest_remote_path = self.full_path(dest_path)?;
        let mut connection = self.acquire_connection().await?;
        ensure_remote_parent_dir(connection.sftp()?, &dest_remote_path).await?;
        let mut src = connection
            .sftp()?
            .open(src_remote_path)
            .await
            .map_err(|error| connection.map_sftp_error("SFTP source open failed", error))?;
        let mut dest = connection
            .sftp()?
            .create(dest_remote_path)
            .await
            .map_err(|error| connection.map_sftp_error("SFTP destination create failed", error))?;
        tokio::io::copy(&mut src, &mut dest)
            .await
            .map_err(|error| map_io_error("SFTP copy failed", error))?;
        dest.flush()
            .await
            .map_err(|error| map_io_error("SFTP copy flush failed", error))?;
        dest.shutdown()
            .await
            .map_err(|error| map_io_error("SFTP copy close failed", error))?;
        connection.mark_reusable();
        Ok(dest_path.to_string())
    }

    fn extensions(&self) -> aster_drive_storage::traits::StorageDriverExtensions<'_> {
        aster_drive_storage::traits::StorageDriverExtensions {
            stream_upload: Some(self),
            ..Default::default()
        }
    }
}

#[async_trait]
impl StreamUploadDriver for SftpDriver {
    async fn put_reader(
        &self,
        storage_path: &str,
        mut reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        size: i64,
    ) -> aster_drive_storage::Result<String> {
        let remote_path = self.full_path(storage_path)?;
        let expected_size = u64::try_from(size).map_err(|_| {
            storage_driver_error(
                StorageErrorKind::Precondition,
                "SFTP stream upload size must be non-negative",
            )
        })?;
        let temporary_path = format!(
            "{remote_path}.aster-upload-{:016x}.tmp",
            rand::random::<u64>()
        );
        let mut connection = self.acquire_connection().await?;
        ensure_remote_parent_dir(connection.sftp()?, &remote_path).await?;
        let mut remote_file = connection
            .sftp()?
            .create(temporary_path.clone())
            .await
            .map_err(|error| connection.map_sftp_error("SFTP create failed", error))?;
        let written = match tokio::io::copy(&mut reader, &mut remote_file).await {
            Ok(written) => written,
            Err(error) => {
                drop(remote_file);
                cleanup_sftp_temporary_file(&mut connection, &temporary_path, "reader error").await;
                return Err(map_io_error("SFTP stream upload failed", error));
            }
        };
        if written != expected_size {
            drop(remote_file);
            cleanup_sftp_temporary_file(&mut connection, &temporary_path, "size mismatch").await;
            return Err(storage_driver_error(
                StorageErrorKind::Precondition,
                format!("SFTP stream upload size mismatch: declared {size}, actual {written}"),
            ));
        }
        if let Err(error) = remote_file.flush().await {
            drop(remote_file);
            cleanup_sftp_temporary_file(&mut connection, &temporary_path, "flush error").await;
            return Err(map_io_error("SFTP stream flush failed", error));
        }
        if let Err(error) = remote_file.shutdown().await {
            drop(remote_file);
            cleanup_sftp_temporary_file(&mut connection, &temporary_path, "close error").await;
            return Err(map_io_error("SFTP stream close failed", error));
        }
        if let Err(error) =
            replace_sftp_object(&mut connection, &temporary_path, &remote_path, storage_path).await
        {
            cleanup_sftp_temporary_file(&mut connection, &temporary_path, "publish error").await;
            return Err(error);
        }
        connection.mark_reusable();
        Ok(storage_path.to_string())
    }

    async fn stage_attempt(
        &self,
        attempt: &StreamUploadAttempt,
        reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
    ) -> aster_drive_storage::Result<()> {
        self.put_reader(&attempt.staging_path, reader, attempt.expected_size)
            .await
            .map(|_| ())
    }

    async fn commit_attempt(
        &self,
        attempt: &StreamUploadAttempt,
    ) -> aster_drive_storage::Result<String> {
        let staging_path = self.full_path(&attempt.staging_path)?;
        let destination_path = self.full_path(&attempt.storage_path)?;
        let mut connection = self.acquire_connection().await?;
        replace_sftp_object(
            &mut connection,
            &staging_path,
            &destination_path,
            &attempt.storage_path,
        )
        .await?;
        connection.mark_reusable();
        Ok(attempt.storage_path.clone())
    }

    async fn abort_attempt(
        &self,
        attempt: &StreamUploadAttempt,
    ) -> aster_drive_storage::Result<StreamUploadCleanup> {
        match self.delete(&attempt.staging_path).await {
            Ok(()) => Ok(StreamUploadCleanup::Cleaned),
            Err(error) if error.kind() == StorageErrorKind::NotFound => {
                Ok(StreamUploadCleanup::Cleaned)
            }
            Err(error) => {
                tracing::warn!(
                    staging_path = %attempt.staging_path,
                    "SFTP attempt cleanup deferred: {error}"
                );
                Ok(StreamUploadCleanup::Deferred)
            }
        }
    }

    async fn put_file(
        &self,
        storage_path: &str,
        local_path: &str,
    ) -> aster_drive_storage::Result<String> {
        let local_file = tokio::fs::File::open(local_path)
            .await
            .map_err(|error| map_io_error("open local upload file failed", error))?;
        let size = local_file
            .metadata()
            .await
            .map_err(|error| map_io_error("stat local upload file failed", error))?
            .len();
        let size = i64::try_from(size).map_err(|error| {
            storage_driver_error(
                StorageErrorKind::Misconfigured,
                format!("local upload file size conversion failed: {error}"),
            )
        })?;
        self.put_reader(storage_path, Box::new(local_file), size)
            .await
    }
}

async fn cleanup_sftp_temporary_file(
    connection: &mut SftpConnectionLease,
    temporary_path: &str,
    reason: &str,
) {
    match connection.sftp() {
        Ok(sftp) => match sftp.remove_file(temporary_path.to_string()).await {
            Ok(()) => {}
            Err(error) if is_sftp_not_found(&error) => {}
            Err(error) => tracing::warn!(
                temporary_path,
                reason,
                "failed to cleanup SFTP temporary object: {error}"
            ),
        },
        Err(error) => tracing::warn!(temporary_path, reason, "SFTP cleanup unavailable: {error}"),
    }
}

async fn replace_sftp_object(
    connection: &mut SftpConnectionLease,
    temporary_path: &str,
    destination_path: &str,
    storage_path: &str,
) -> aster_drive_storage::Result<()> {
    let destination_exists = match connection
        .sftp()?
        .metadata(destination_path.to_string())
        .await
    {
        Ok(_) => true,
        Err(error) if is_sftp_not_found(&error) => false,
        Err(error) => return Err(connection.map_sftp_error("SFTP destination stat failed", error)),
    };
    if !destination_exists {
        return connection
            .sftp()?
            .rename(temporary_path.to_string(), destination_path.to_string())
            .await
            .map_err(|error| connection.map_sftp_error("SFTP atomic rename failed", error));
    }

    if try_sftp_posix_rename(connection, temporary_path, destination_path).await? {
        Ok(())
    } else {
        Err(storage_driver_error(
            StorageErrorKind::Unsupported,
            format!(
                "SFTP server does not support atomic replacement for existing object {storage_path}; requires posix-rename@openssh.com"
            ),
        ))
    }
}

async fn try_sftp_posix_rename(
    connection: &mut SftpConnectionLease,
    source_path: &str,
    destination_path: &str,
) -> aster_drive_storage::Result<bool> {
    const POSIX_RENAME_EXTENSION: &str = "posix-rename@openssh.com";

    let ssh = connection
        .connection
        .as_mut()
        .map(|connection| &mut connection.ssh)
        .ok_or_else(|| {
            storage_driver_error(StorageErrorKind::Unknown, "SFTP connection lease is empty")
        })?;
    let channel = timeout_io(
        "open SFTP atomic rename channel",
        IO_TIMEOUT,
        ssh.channel_open_session(),
    )
    .await?
    .map_err(|error| map_ssh_error("open SFTP atomic rename channel failed", error))?;
    timeout_io(
        "open SFTP atomic rename subsystem",
        IO_TIMEOUT,
        channel.request_subsystem(true, "sftp"),
    )
    .await?
    .map_err(|error| map_ssh_error("open SFTP atomic rename subsystem failed", error))?;

    let config = SftpClientConfig {
        request_timeout_secs: IO_TIMEOUT.as_secs(),
        ..Default::default()
    };
    let raw = RawSftpSession::new_with_config(channel.into_stream(), config);
    let version = raw
        .init()
        .await
        .map_err(|error| map_sftp_error("initialize SFTP atomic rename session failed", error))?;
    if !version.extensions.contains_key(POSIX_RENAME_EXTENSION) {
        if let Err(error) = raw.close_session() {
            tracing::warn!("failed to close SFTP atomic rename session without extension: {error}");
        }
        return Ok(false);
    }

    let mut data = Vec::with_capacity(source_path.len() + destination_path.len() + 8);
    encode_sftp_extension_string(&mut data, source_path)?;
    encode_sftp_extension_string(&mut data, destination_path)?;
    let response = raw
        .extended(POSIX_RENAME_EXTENSION, data)
        .await
        .map_err(|error| map_sftp_error("SFTP POSIX rename failed", error))?;
    if let Err(error) = raw.close_session() {
        tracing::warn!("failed to close SFTP atomic rename session: {error}");
    }
    match response {
        Packet::Status(status) if status.status_code == StatusCode::Ok => Ok(true),
        Packet::Status(status) if status.status_code == StatusCode::OpUnsupported => Ok(false),
        Packet::Status(status) => Err(map_sftp_error(
            "SFTP POSIX rename failed",
            SftpError::Status(status),
        )),
        _ => Err(storage_driver_error(
            StorageErrorKind::Unknown,
            "SFTP POSIX rename returned an unexpected packet",
        )),
    }
}

fn encode_sftp_extension_string(output: &mut Vec<u8>, value: &str) -> Result<()> {
    let length = u32::try_from(value.len()).map_err(|error| {
        storage_driver_error(
            StorageErrorKind::Misconfigured,
            format!("SFTP extension path is too long: {error}"),
        )
    })?;
    output.extend_from_slice(&length.to_be_bytes());
    output.extend_from_slice(value.as_bytes());
    Ok(())
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
) -> Option<StorageError> {
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

fn map_ssh_error(context: &'static str, error: russh::Error) -> StorageError {
    storage_driver_error(
        classify_error_message(&error.to_string()),
        format!("{context}: {error}"),
    )
}

fn map_sftp_error(context: &'static str, error: SftpError) -> StorageError {
    storage_driver_error(classify_sftp_error(&error), format!("{context}: {error}"))
}

fn map_io_error(context: &'static str, error: std::io::Error) -> StorageError {
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

fn is_sftp_connection_reusable_after_error(error: &SftpError) -> bool {
    matches!(
        error,
        SftpError::Status(status)
            if matches!(
                status.status_code,
                StatusCode::NoSuchFile | StatusCode::PermissionDenied
            )
    )
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
    use super::encode_sftp_extension_string;
    use super::{
        CONNECT_TIMEOUT, DEFAULT_POOL_SIZE, IO_TIMEOUT, POOL_ACQUIRE_TIMEOUT,
        POOLED_CONNECTION_IDLE_TTL, SSH_KEEPALIVE_INTERVAL, SftpConnectionLease,
        SftpConnectionPool, SftpDriverConfig, SftpStaticCredentials, classify_sftp_error,
        cleanup_sftp_temporary_file, host_key_fingerprint_matches,
        is_sftp_connection_reusable_after_error, is_valid_host_key_fingerprint, join_remote_path,
        normalize_host_key_fingerprint, normalize_remote_base_path, parse_sftp_endpoint,
        sanitize_relative_storage_path, try_sftp_posix_rename,
    };
    use aster_drive_storage::error::StorageErrorKind;
    use aster_drive_storage::{StorageDriver, StreamUploadDriver};
    use russh_sftp::client::error::Error as SftpError;
    use russh_sftp::protocol::{Status, StatusCode};
    use std::sync::Arc;
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

    #[test]
    fn sftp_pool_defaults_match_storage_timeout_boundaries() {
        assert_eq!(DEFAULT_POOL_SIZE, 4);
        assert_eq!(CONNECT_TIMEOUT, std::time::Duration::from_secs(10));
        assert_eq!(IO_TIMEOUT, std::time::Duration::from_secs(30));
        assert_eq!(SSH_KEEPALIVE_INTERVAL, std::time::Duration::from_secs(10));
        assert_eq!(POOL_ACQUIRE_TIMEOUT, std::time::Duration::from_secs(30));
        assert_eq!(
            POOLED_CONNECTION_IDLE_TTL,
            std::time::Duration::from_secs(60)
        );
    }

    #[test]
    fn sftp_pool_size_has_lower_bound() {
        let pool = SftpConnectionPool::new(0);
        assert_eq!(pool.max_idle, 1);
        assert_eq!(pool.semaphore.available_permits(), 1);
    }

    #[test]
    fn encodes_posix_rename_extension_paths_as_ssh_strings() {
        let mut encoded = Vec::new();
        encode_sftp_extension_string(&mut encoded, "a/b").unwrap();
        encode_sftp_extension_string(&mut encoded, "target").unwrap();

        assert_eq!(
            encoded,
            [
                0, 0, 0, 3, b'a', b'/', b'b', 0, 0, 0, 6, b't', b'a', b'r', b'g', b'e', b't'
            ]
        );
    }

    #[tokio::test]
    async fn empty_connection_lease_reports_cleanup_and_atomic_rename_unavailable() {
        let pool = Arc::new(SftpConnectionPool::new(1));
        let permit = pool
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("test pool permit should be available");
        let mut lease = SftpConnectionLease {
            connection: None,
            pool,
            _permit: permit,
            reusable: false,
        };

        let error = try_sftp_posix_rename(&mut lease, "source", "destination")
            .await
            .expect_err("atomic rename requires a live SFTP connection");
        assert_eq!(error.kind(), StorageErrorKind::Unknown);
        assert!(error.message().contains("connection lease is empty"));

        cleanup_sftp_temporary_file(&mut lease, "temporary", "test cleanup").await;
    }

    #[test]
    fn sftp_status_errors_reuse_connection_only_for_known_safe_statuses() {
        let status = |status_code, error_message: &str| {
            SftpError::Status(Status {
                id: 1,
                status_code,
                error_message: error_message.to_string(),
                language_tag: String::new(),
            })
        };

        assert!(is_sftp_connection_reusable_after_error(&status(
            StatusCode::NoSuchFile,
            "missing"
        )));
        assert!(is_sftp_connection_reusable_after_error(&status(
            StatusCode::PermissionDenied,
            "denied"
        )));
        assert!(!is_sftp_connection_reusable_after_error(&status(
            StatusCode::BadMessage,
            "bad packet"
        )));
        assert!(!is_sftp_connection_reusable_after_error(&status(
            StatusCode::Failure,
            "generic failure"
        )));
        assert!(!is_sftp_connection_reusable_after_error(&status(
            StatusCode::OpUnsupported,
            "unsupported operation"
        )));
        assert!(!is_sftp_connection_reusable_after_error(&status(
            StatusCode::NoConnection,
            "no connection"
        )));
        assert!(!is_sftp_connection_reusable_after_error(&status(
            StatusCode::ConnectionLost,
            "lost"
        )));
        assert!(!is_sftp_connection_reusable_after_error(
            &SftpError::Timeout
        ));
    }

    fn env_config() -> Option<(SftpDriverConfig, SftpStaticCredentials)> {
        let endpoint = std::env::var("ASTER_SFTP_TEST_ENDPOINT").ok()?;
        let username = std::env::var("ASTER_SFTP_TEST_USERNAME").ok()?;
        let password = std::env::var("ASTER_SFTP_TEST_PASSWORD").ok()?;
        let base_path = std::env::var("ASTER_SFTP_TEST_BASE_PATH").ok()?;
        let host_key_fingerprint = std::env::var("ASTER_SFTP_TEST_HOST_KEY_FINGERPRINT").ok()?;
        Some((
            SftpDriverConfig {
                endpoint,
                base_path,
                host_key_fingerprint: Some(host_key_fingerprint),
            },
            SftpStaticCredentials { username, password },
        ))
    }

    #[tokio::test]
    #[ignore = "requires ASTER_SFTP_TEST_* environment variables and a reachable SFTP server"]
    async fn real_sftp_driver_round_trip_uses_streaming_upload() {
        let Some((config, credentials)) = env_config() else {
            eprintln!("skipping real SFTP test because ASTER_SFTP_TEST_* is not set");
            return;
        };
        let driver = super::SftpDriver::new(config, credentials).unwrap();
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

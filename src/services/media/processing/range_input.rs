//! Per-operation HTTP reader for exactly one immutable storage object.
//!
//! This endpoint never forwards a URL or selects an object from request data.
//! FFmpeg uses it as a mandatory proxy for its synthetic input URL; only the
//! fixed source supports HEAD/GET/Range. Provider credentials stay in the driver.

use std::io::{self, Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::errors::{AsterError, MapAsterErr, Result};
use crate::services::files::archive::core::range_reader::StorageRangeReader;
use aster_drive_storage::StorageDriver;

const CHUNK_BYTES: usize = 256 * 1024;
const MAX_REQUESTS: usize = 128;
const MAX_CONNECTIONS: usize = 4;
const MAX_HEADER_BYTES: usize = 8192;
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) struct ObjectRangeInput {
    pub(super) url: String,
    pub(super) proxy: String,
    stop: CancellationToken,
    task: tokio::task::JoinHandle<()>,
}

impl ObjectRangeInput {
    pub(super) async fn start(
        driver: Arc<dyn StorageDriver>,
        key: &str,
        size: u64,
    ) -> Result<Self> {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .map_aster_err_ctx("bind object range input", AsterError::storage_driver_error)?;
        let address = listener.local_addr().map_aster_err_ctx(
            "read object range address",
            AsterError::storage_driver_error,
        )?;
        // FFmpeg's forward-proxy request target explicitly includes the default port.
        let url = format!("http://media.invalid:80/{}/source", uuid::Uuid::new_v4());
        let source = Arc::new(Source {
            url: url.clone(),
            size,
            reader: Mutex::new(StorageRangeReader::new(
                driver,
                key,
                size,
                tokio::runtime::Handle::current(),
            )),
        });
        let stop = CancellationToken::new();
        let task = tokio::spawn(serve(listener, source, stop.clone()));
        Ok(Self {
            url,
            proxy: format!("http://{address}"),
            stop,
            task,
        })
    }
}

impl Drop for ObjectRangeInput {
    fn drop(&mut self) {
        self.stop.cancel();
        self.task.abort();
    }
}

struct Source {
    url: String,
    size: u64,
    reader: Mutex<StorageRangeReader>,
}

async fn serve(listener: TcpListener, source: Arc<Source>, stop: CancellationToken) {
    let mut connections = JoinSet::new();
    let mut requests = 0;
    loop {
        if requests >= MAX_REQUESTS && connections.is_empty() {
            break;
        }
        tokio::select! {
            biased;
            () = stop.cancelled() => break,
            Some(_) = connections.join_next(), if !connections.is_empty() => {},
            incoming = listener.accept(), if requests < MAX_REQUESTS && connections.len() < MAX_CONNECTIONS => {
                let Ok((socket, _)) = incoming else { break; };
                requests += 1;
                let source = source.clone();
                let stop = stop.clone();
                connections.spawn(async move {
                    tokio::select! {
                        () = stop.cancelled() => {},
                        _ = serve_connection(socket, source) => {},
                    }
                });
            }
            else => break,
        }
    }
}

struct Request {
    head: bool,
    start: u64,
    length: u64,
    partial: bool,
}

fn parse_request(bytes: &[u8], source: &Source) -> io::Result<Option<Request>> {
    let invalid = || {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "unsupported object range request",
        )
    };
    let mut headers = [httparse::EMPTY_HEADER; 32];
    let mut request = httparse::Request::new(&mut headers);
    let status = request.parse(bytes).map_err(|_| invalid())?;
    let httparse::Status::Complete(length) = status else {
        return Ok(None);
    };
    if length != bytes.len() || request.path != Some(source.url.as_str()) {
        return Err(invalid());
    }
    let head = match request.method {
        Some("GET") => false,
        Some("HEAD") => true,
        _ => return Err(invalid()),
    };
    let mut range = None;
    for header in request.headers {
        match header.name.to_ascii_lowercase().as_str() {
            "range" => {
                if range.is_some() {
                    return Err(invalid());
                }
                let value = std::str::from_utf8(header.value).map_err(|_| invalid())?;
                range = Some(
                    aster_forge_utils::http_range::parse_single_byte_range(value, source.size)
                        .map_err(|_| invalid())?,
                );
            }
            "content-length" | "transfer-encoding" => return Err(invalid()),
            _ => {}
        }
    }
    Ok(Some(match range {
        Some(range) => Request {
            head,
            start: range.start(),
            length: range.length(),
            partial: true,
        },
        None => Request {
            head,
            start: 0,
            length: source.size,
            partial: false,
        },
    }))
}

async fn serve_connection(mut socket: TcpStream, source: Arc<Source>) -> io::Result<()> {
    let mut header = Vec::with_capacity(1024);
    let request = loop {
        if header.len() >= MAX_HEADER_BYTES {
            return Err(io::Error::other("object range header limit"));
        }
        let mut buffer = [0u8; 1024];
        let remaining = (MAX_HEADER_BYTES - header.len()).min(buffer.len());
        let read = tokio::time::timeout(CONNECTION_TIMEOUT, socket.read(&mut buffer[..remaining]))
            .await??;
        if read == 0 {
            return Ok(());
        }
        header.extend_from_slice(&buffer[..read]);
        match parse_request(&header, &source) {
            Ok(Some(request)) => break request,
            Ok(None) => continue,
            Err(error) => {
                let _ = socket
                    .write_all(
                        b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                    .await;
                return Err(error);
            }
        }
    };
    let status = match request.partial {
        true => "206 Partial Content",
        false => "200 OK",
    };
    let content_range = match request.partial {
        true => format!(
            "Content-Range: bytes {}-{}/{}\r\n",
            request.start,
            request.start + request.length - 1,
            source.size
        ),
        false => String::new(),
    };
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\n{content_range}Accept-Ranges: bytes\r\nContent-Type: application/octet-stream\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        request.length
    );
    tokio::time::timeout(CONNECTION_TIMEOUT, socket.write_all(response.as_bytes())).await??;
    if request.head {
        return Ok(());
    }
    let end = request.start + request.length;
    let mut position = request.start;
    while position < end {
        // Do not read ahead after FFmpeg has closed a previous seek connection.
        socket.writable().await?;
        let length =
            usize::try_from((end - position).min(CHUNK_BYTES as u64)).map_err(io::Error::other)?;
        let source = source.clone();
        let chunk = tokio::task::spawn_blocking(move || {
            let mut reader = source
                .reader
                .lock()
                .map_err(|_| io::Error::other("object reader lock poisoned"))?;
            reader.seek(SeekFrom::Start(position))?;
            let mut bytes = vec![0; length];
            reader.read_exact(&mut bytes)?;
            Ok::<_, io::Error>(bytes)
        })
        .await
        .map_err(io::Error::other)??;
        tokio::time::timeout(CONNECTION_TIMEOUT, socket.write_all(&chunk)).await??;
        position += u64::try_from(chunk.len()).map_err(io::Error::other)?;
    }
    Ok(())
}

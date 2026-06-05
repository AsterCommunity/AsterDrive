//! WebDAV 子模块：`dav`。

use std::future::Future;
use std::io::SeekFrom;
use std::pin::Pin;
use std::time::{Duration, SystemTime};

use bytes::{Buf, Bytes};
use futures::Stream;
use http::StatusCode;
use xmltree::Element;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DavPath {
    raw: String,
    decoded: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DavPathError {
    InvalidEncoding,
}

impl std::fmt::Display for DavPathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidEncoding => f.write_str("invalid WebDAV path encoding"),
        }
    }
}

impl std::error::Error for DavPathError {}

impl DavPath {
    pub fn new(path: &str) -> Result<Self, DavPathError> {
        let raw = normalize_path(path);
        let decoded = urlencoding::decode(&raw)
            .map_err(|_| DavPathError::InvalidEncoding)?
            .into_owned()
            .into_bytes();
        Ok(Self { raw, decoded })
    }

    pub fn root() -> Self {
        Self {
            raw: "/".to_string(),
            decoded: b"/".to_vec(),
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.decoded
    }

    pub fn as_str(&self) -> &str {
        &self.raw
    }

    pub fn is_collection(&self) -> bool {
        self.raw == "/" || self.raw.ends_with('/')
    }
}

fn normalize_path(path: &str) -> String {
    if path.is_empty() || path == "/" {
        return "/".to_string();
    }

    let mut normalized = path.to_string();
    if !normalized.starts_with('/') {
        normalized.insert(0, '/');
    }
    normalized
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsError {
    NotFound,
    Forbidden,
    GeneralFailure,
    Exists,
    InsufficientStorage,
    TooLarge,
    BadRequest,
}

impl std::fmt::Display for FsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => f.write_str("not found"),
            Self::Forbidden => f.write_str("forbidden"),
            Self::GeneralFailure => f.write_str("general failure"),
            Self::Exists => f.write_str("already exists"),
            Self::InsufficientStorage => f.write_str("insufficient storage"),
            Self::TooLarge => f.write_str("too large"),
            Self::BadRequest => f.write_str("bad request"),
        }
    }
}

impl std::error::Error for FsError {}

pub type FsResult<T> = Result<T, FsError>;
pub type FsFuture<'a, T> = Pin<Box<dyn Future<Output = FsResult<T>> + Send + 'a>>;
pub type FsStream<T> = Pin<Box<dyn Stream<Item = FsResult<T>> + Send>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadDirMeta {
    Data,
}

#[derive(Debug, Clone, Default)]
pub struct OpenOptions {
    pub read: bool,
    pub write: bool,
    pub append: bool,
    pub truncate: bool,
    pub create: bool,
    pub create_new: bool,
    pub size: Option<u64>,
    pub checksum: Option<String>,
}

impl OpenOptions {
    pub fn read() -> Self {
        Self {
            read: true,
            ..Self::default()
        }
    }

    pub fn write() -> Self {
        Self {
            write: true,
            ..Self::default()
        }
    }
}

pub trait DavMetaData: Send + Sync {
    fn len(&self) -> u64;
    fn modified(&self) -> FsResult<SystemTime>;
    fn is_dir(&self) -> bool;
    fn etag(&self) -> Option<String>;
    fn created(&self) -> FsResult<SystemTime>;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn is_file(&self) -> bool {
        !self.is_dir()
    }
}

pub trait DavDirEntry: Send {
    fn name(&self) -> Vec<u8>;
    fn metadata<'a>(&'a self) -> FsFuture<'a, Box<dyn DavMetaData>>;
}

pub trait DavFile: Send {
    fn metadata<'a>(&'a mut self) -> FsFuture<'a, Box<dyn DavMetaData>>;
    fn read_bytes(&mut self, count: usize) -> FsFuture<'_, Bytes>;
    fn write_bytes(&mut self, buf: Bytes) -> FsFuture<'_, ()>;
    fn write_buf(&mut self, buf: Box<dyn Buf + Send>) -> FsFuture<'_, ()>;
    fn seek(&mut self, pos: SeekFrom) -> FsFuture<'_, u64>;
    fn flush(&mut self) -> FsFuture<'_, ()>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DavProp {
    pub name: String,
    pub prefix: Option<String>,
    pub namespace: Option<String>,
    pub xml: Option<Vec<u8>>,
}

pub trait DavFileSystem: Send + Sync {
    fn open<'a>(
        &'a self,
        path: &'a DavPath,
        options: OpenOptions,
    ) -> FsFuture<'a, Box<dyn DavFile>>;
    fn read_dir<'a>(
        &'a self,
        path: &'a DavPath,
        meta: ReadDirMeta,
    ) -> FsFuture<'a, FsStream<Box<dyn DavDirEntry>>>;
    fn metadata<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, Box<dyn DavMetaData>>;
    fn create_dir<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, ()>;
    fn remove_dir<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, ()>;
    fn remove_file<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, ()>;
    fn rename<'a>(&'a self, from: &'a DavPath, to: &'a DavPath) -> FsFuture<'a, ()>;
    fn copy<'a>(&'a self, from: &'a DavPath, to: &'a DavPath) -> FsFuture<'a, ()>;

    fn get_quota(&self) -> FsFuture<'_, (u64, Option<u64>)> {
        Box::pin(async { Ok((0, None)) })
    }

    fn have_props<'a>(
        &'a self,
        _path: &'a DavPath,
    ) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async { false })
    }

    fn get_props<'a>(
        &'a self,
        _path: &'a DavPath,
        _do_content: bool,
    ) -> FsFuture<'a, Vec<DavProp>> {
        Box::pin(async { Ok(Vec::new()) })
    }

    fn patch_props<'a>(
        &'a self,
        _path: &'a DavPath,
        _patches: Vec<(bool, DavProp)>,
    ) -> FsFuture<'a, Vec<(StatusCode, DavProp)>> {
        Box::pin(async { Ok(Vec::new()) })
    }
}

#[derive(Debug, Clone)]
pub struct DavLock {
    pub token: String,
    pub path: Box<DavPath>,
    pub principal: Option<String>,
    pub owner: Option<Box<Element>>,
    pub timeout_at: Option<SystemTime>,
    pub timeout: Option<Duration>,
    pub shared: bool,
    pub deep: bool,
}

pub type LsFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait DavLockSystem: Send + Sync {
    fn lock(
        &self,
        path: &DavPath,
        principal: Option<&str>,
        owner: Option<&Element>,
        timeout: Option<Duration>,
        shared: bool,
        deep: bool,
    ) -> LsFuture<'_, Result<DavLock, DavLock>>;

    fn unlock(&self, path: &DavPath, token: &str) -> LsFuture<'_, Result<(), ()>>;

    fn refresh(
        &self,
        path: &DavPath,
        token: &str,
        timeout: Option<Duration>,
    ) -> LsFuture<'_, Result<DavLock, ()>>;

    fn check(
        &self,
        path: &DavPath,
        principal: Option<&str>,
        ignore_principal: bool,
        deep: bool,
        submitted_tokens: &[String],
    ) -> LsFuture<'_, Result<(), DavLock>>;

    fn discover(&self, path: &DavPath) -> LsFuture<'_, Vec<DavLock>>;

    fn conflicting_locks(&self, path: &DavPath, deep: bool) -> LsFuture<'_, Vec<DavLock>>;

    fn delete(&self, path: &DavPath) -> LsFuture<'_, Result<(), ()>>;
}

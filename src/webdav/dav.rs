//! WebDAV 子模块：`dav`。

use std::collections::HashMap;
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
    PathEscape,
}

impl std::fmt::Display for DavPathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidEncoding => f.write_str("invalid WebDAV path encoding"),
            Self::PathEscape => f.write_str("WebDAV path escapes the mount root"),
        }
    }
}

impl std::error::Error for DavPathError {}

impl DavPath {
    pub fn new(path: &str) -> Result<Self, DavPathError> {
        let raw = ensure_leading_slash(path);
        let decoded = urlencoding::decode(&raw)
            .map_err(|_| DavPathError::InvalidEncoding)?
            .into_owned();
        let raw = clean_decoded_path(&decoded)?;
        let decoded = raw.as_bytes().to_vec();
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

fn ensure_leading_slash(path: &str) -> String {
    if path.is_empty() || path == "/" {
        return "/".to_string();
    }

    let mut normalized = path.to_string();
    if !normalized.starts_with('/') {
        normalized.insert(0, '/');
    }
    normalized
}

fn clean_decoded_path(path: &str) -> Result<String, DavPathError> {
    let mut segments = Vec::new();
    let mut is_collection = false;

    for (index, segment) in path.split('/').enumerate() {
        match segment {
            "" => {
                if index > 0 {
                    is_collection = true;
                }
            }
            "." => {
                is_collection = true;
            }
            ".." => {
                if segments.pop().is_none() {
                    return Err(DavPathError::PathEscape);
                }
                is_collection = true;
            }
            segment => {
                segments.push(segment);
                is_collection = false;
            }
        }
    }

    if segments.is_empty() {
        return Ok("/".to_string());
    }

    let mut cleaned = format!("/{}", segments.join("/"));
    if is_collection && !cleaned.ends_with('/') {
        cleaned.push('/');
    }
    Ok(cleaned)
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
    fn content_type(&self) -> Option<&str> {
        None
    }
    fn created(&self) -> FsResult<SystemTime>;
    fn property_entity(&self) -> Option<(crate::types::EntityType, i64)> {
        None
    }

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

    fn get_props_many<'a>(
        &'a self,
        paths: &'a [DavPath],
        do_content: bool,
    ) -> FsFuture<'a, HashMap<DavPath, Vec<DavProp>>> {
        Box::pin(async move {
            let mut result = HashMap::with_capacity(paths.len());
            for path in paths {
                result.insert(path.clone(), self.get_props(path, do_content).await?);
            }
            Ok(result)
        })
    }

    fn get_props_many_for_entities<'a>(
        &'a self,
        targets: &'a [(DavPath, crate::types::EntityType, i64)],
        do_content: bool,
    ) -> FsFuture<'a, HashMap<DavPath, Vec<DavProp>>> {
        Box::pin(async move {
            let paths = targets
                .iter()
                .map(|(path, _, _)| path.clone())
                .collect::<Vec<_>>();
            self.get_props_many(&paths, do_content).await
        })
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DavLockPreflightError {
    LimitExceeded,
    GeneralFailure,
}

#[derive(Debug, Clone)]
pub enum DavLockError {
    Conflict(DavLock),
    LimitExceeded,
    Backend,
}

pub trait DavLockSystem: Send + Sync {
    fn prepare_lock(&self, _path: &DavPath) -> LsFuture<'_, Result<(), DavLockPreflightError>> {
        Box::pin(async { Ok(()) })
    }

    fn lock(
        &self,
        path: &DavPath,
        principal: Option<&str>,
        owner: Option<&Element>,
        timeout: Option<Duration>,
        shared: bool,
        deep: bool,
    ) -> LsFuture<'_, Result<DavLock, DavLockError>>;

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

    fn discover_many<'a>(
        &'a self,
        paths: &'a [DavPath],
    ) -> LsFuture<'a, HashMap<DavPath, Vec<DavLock>>> {
        Box::pin(async move {
            let mut result = HashMap::with_capacity(paths.len());
            for path in paths {
                result.insert(path.clone(), self.discover(path).await);
            }
            result
        })
    }

    fn conflicting_locks(&self, path: &DavPath, deep: bool) -> LsFuture<'_, Vec<DavLock>>;

    fn delete(&self, path: &DavPath) -> LsFuture<'_, Result<(), ()>>;
}

#[cfg(test)]
mod tests {
    use super::{DavPath, DavPathError};

    #[test]
    fn dav_path_collapses_dot_segments_before_cache_and_resolution() {
        let path = DavPath::new("/projects/./docs/reports/../q1.txt").unwrap();
        assert_eq!(path.as_str(), "/projects/docs/q1.txt");
        assert_eq!(path.as_bytes(), b"/projects/docs/q1.txt");

        let collection = DavPath::new("/projects/./docs/").unwrap();
        assert_eq!(collection.as_str(), "/projects/docs/");
        assert!(collection.is_collection());

        let relative = DavPath::new("projects/./docs").unwrap();
        assert_eq!(relative.as_str(), "/projects/docs");
    }

    #[test]
    fn dav_path_preserves_collection_alias_after_dot_segments() {
        let dot_alias = DavPath::new("/projects/docs/.").unwrap();
        assert_eq!(dot_alias.as_str(), "/projects/docs/");
        assert!(dot_alias.is_collection());

        let parent_alias = DavPath::new("/projects/docs/reports/..").unwrap();
        assert_eq!(parent_alias.as_str(), "/projects/docs/");
        assert!(parent_alias.is_collection());

        let encoded_parent_alias = DavPath::new("/projects/docs/reports/%2e%2e").unwrap();
        assert_eq!(encoded_parent_alias.as_str(), "/projects/docs/");
        assert!(encoded_parent_alias.is_collection());
    }

    #[test]
    fn dav_path_rejects_dot_dot_escape_after_percent_decoding() {
        assert!(matches!(
            DavPath::new("/../secret.txt"),
            Err(DavPathError::PathEscape)
        ));
        assert!(matches!(
            DavPath::new("/projects/../../secret.txt"),
            Err(DavPathError::PathEscape)
        ));
        assert!(matches!(
            DavPath::new("/%2e%2e/secret.txt"),
            Err(DavPathError::PathEscape)
        ));
    }

    #[test]
    fn dav_path_allows_internal_dot_dot_without_cache_aliases() {
        let path = DavPath::new("/projects/docs/../manuals/file.txt").unwrap();
        assert_eq!(path.as_str(), "/projects/manuals/file.txt");
        assert_eq!(path.as_bytes(), b"/projects/manuals/file.txt");

        let encoded = DavPath::new("/projects/docs/%2e%2e/manuals/file.txt").unwrap();
        assert_eq!(encoded.as_str(), "/projects/manuals/file.txt");
        assert_eq!(encoded.as_bytes(), b"/projects/manuals/file.txt");
    }
}

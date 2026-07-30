//! WebDAV PROPFIND / PROPPATCH handlers.

use std::collections::HashMap;
use std::time::{Instant, SystemTime};

use actix_web::HttpResponse;
use actix_web::http::StatusCode;
use aster_forge_webdav::{
    DavBackendError, DavBackendErrorKind, DavCapabilitySnapshot, DavDirectoryEntry,
    DavDirectoryPageLimits, DavDirectoryPageState, DavDirectoryReadError, DavFileSystem,
    DavLivePropertyMetadata, DavLivePropertyRequirements, DavLivePropertyValueSnapshot, DavLock,
    DavLockSystem, DavLockXml, DavMetaData, DavMultiStatusItem, DavMultiStatusLimits,
    DavMultiStatusSourceError, DavNeverCancelled, DavPath, DavProp, DavPropfindRequest,
    DavQuotaSnapshot, DavRequestHead, DavResourceState, DavXmlElement, Depth, FsError,
    build_live_propfind_item, build_proppatch_item, dav_dead_property_element,
    live_property_requirements, multistatus_stream_response, property_multistatus_response,
    propfind_finite_depth_response, propfind_request_label, propfind_xml_error_response,
    proppatch_xml_error_response, read_next_directory_page,
};
use futures::Stream;

use crate::webdav::backend::AsterDavFs;
use crate::webdav::capability::DriveDavCapabilityProvider;
use crate::webdav::responses;
use crate::webdav::{
    child_relative_path, display_name, fs_error_response, href_for_dav_path, href_for_relative,
};

const PROPFIND_PAGE_ENTRIES: usize = 256;
const PROPFIND_MAXIMUM_PAGES: usize = 40;
const PROPFIND_DIRECTORY_LIMITS: DavDirectoryPageLimits = DavDirectoryPageLimits {
    maximum_entries: PROPFIND_PAGE_ENTRIES,
    maximum_pages: PROPFIND_MAXIMUM_PAGES,
};
const PROPFIND_MAXIMUM_RESOURCES: usize = 10_000;
const PROPFIND_MAXIMUM_OUTPUT_BYTES: usize = 16 * 1024 * 1024;
const PROPFIND_MAXIMUM_PROPERTIES_PER_RESOURCE: usize = 512;
const PROPFIND_CHUNK_BYTES: usize = 16 * 1024;

#[derive(Default)]
struct PropfindPreload {
    dead_properties: HashMap<DavPath, Vec<DavProp>>,
    locks: HashMap<DavPath, Vec<DavLockXml>>,
}

struct PropfindValues {
    metadata: PropfindMetadata,
    active_locks: Vec<DavLockXml>,
    dead_properties: Vec<DavProp>,
    quota: Option<DavQuotaSnapshot>,
}

struct PropfindMetadata {
    creation_date: Option<SystemTime>,
    display_name: String,
    content_length: Option<u64>,
    content_type: Option<String>,
    etag: Option<String>,
    last_modified: Option<SystemTime>,
}

struct PropfindPageResource {
    path: DavPath,
    relative: String,
    metadata: PropfindMetadata,
    capabilities: DavCapabilitySnapshot,
}

impl DavLivePropertyValueSnapshot for PropfindValues {
    fn metadata(&self) -> DavLivePropertyMetadata<'_> {
        DavLivePropertyMetadata {
            creation_date: self.metadata.creation_date,
            display_name: Some(&self.metadata.display_name),
            content_language: None,
            content_length: self.metadata.content_length,
            content_type: self.metadata.content_type.as_deref(),
            etag: self.metadata.etag.as_deref(),
            last_modified: self.metadata.last_modified,
        }
    }

    fn active_locks(&self) -> &[DavLockXml] {
        &self.active_locks
    }

    fn dead_properties(&self) -> &[DavProp] {
        &self.dead_properties
    }

    fn quota(&self) -> Option<DavQuotaSnapshot> {
        self.quota
    }
}

pub(crate) async fn handle_propfind<L>(
    request_head: &DavRequestHead,
    dav_fs: &AsterDavFs,
    lock_system: &L,
    prefix: &str,
    body: &[u8],
    capability_snapshot: &DavCapabilitySnapshot,
) -> HttpResponse
where
    L: DavLockSystem + Clone + Send + Sync + 'static,
{
    let path = request_head.target.clone();
    if let Err(response) = aster_forge_webdav::actix::enforce_if_header_with_backends(
        request_head.if_header.as_ref(),
        dav_fs,
        lock_system,
        &path,
        prefix,
        &request_head.origin.scheme,
        &request_head.origin.host,
    )
    .await
    {
        return response;
    }
    let Some(depth) = request_head.depth else {
        return responses::empty(StatusCode::INTERNAL_SERVER_ERROR);
    };
    let request_kind = match parse_propfind_request(body) {
        Ok(kind) => kind,
        Err(response) => return response,
    };

    let request_started_at = Instant::now();
    let metadata_started_at = Instant::now();
    let root_meta = match dav_fs.metadata(&path).await {
        Ok(meta) => meta,
        Err(error) => return fs_error_response(error),
    };
    let metadata_elapsed_ms = metadata_started_at.elapsed().as_millis();
    if depth == Depth::Infinity && root_meta.is_dir() {
        return forge_response(propfind_finite_depth_response());
    }

    let relative = path.as_str().to_owned();
    let requirements = live_property_requirements(capability_snapshot, &request_kind);
    let quota = match load_quota(dav_fs, requirements).await {
        Ok(quota) => quota,
        Err(error) => return fs_error_response(error),
    };
    let preload_started_at = Instant::now();
    let mut root_preload = match preload_property_values(
        dav_fs,
        lock_system,
        prefix,
        std::slice::from_ref(&path),
        requirements,
        !matches!(request_kind, DavPropfindRequest::PropName),
    )
    .await
    {
        Ok(preload) => preload,
        Err(error) => return fs_error_response(error),
    };
    let root_is_dir = root_meta.is_dir();
    let root_metadata = match propfind_metadata(&relative, root_meta.as_ref()) {
        Ok(metadata) => metadata,
        Err(error) => return fs_error_response(error),
    };
    let root_values = values_for(path.clone(), root_metadata, &mut root_preload, quota);
    let root_item = match build_live_propfind_item(
        href_for_relative(prefix, &relative),
        capability_snapshot,
        &request_kind,
        &root_values,
    ) {
        Ok(item) => item,
        Err(error) => {
            tracing::warn!(error = %error, "failed to render WebDAV root live properties");
            return responses::empty(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    tracing::debug!(
        depth = ?depth,
        kind = propfind_request_label(&request_kind),
        resource_count = 1,
        metadata_elapsed_ms,
        preload_elapsed_ms = preload_started_at.elapsed().as_millis(),
        total_elapsed_ms = request_started_at.elapsed().as_millis(),
        "WebDAV PROPFIND response stream prepared"
    );

    let source = propfind_item_stream(
        root_item,
        depth,
        root_is_dir,
        path,
        relative,
        dav_fs.clone(),
        lock_system.clone(),
        prefix.to_owned(),
        request_kind,
        requirements,
        quota,
    );
    forge_response(multistatus_stream_response(
        source,
        DavMultiStatusLimits::new(
            PROPFIND_MAXIMUM_OUTPUT_BYTES,
            PROPFIND_MAXIMUM_RESOURCES,
            PROPFIND_MAXIMUM_PROPERTIES_PER_RESOURCE,
            PROPFIND_CHUNK_BYTES,
        ),
    ))
}

pub(crate) async fn handle_proppatch(
    request_head: &DavRequestHead,
    dav_fs: &dyn DavFileSystem,
    lock_system: &dyn DavLockSystem,
    prefix: &str,
    body: &[u8],
) -> HttpResponse {
    let path = request_head.target.clone();
    if path.as_str() == "/" {
        return responses::unsupported_root_proppatch();
    }
    if let Err(response) = aster_forge_webdav::actix::enforce_if_header_with_backends(
        request_head.if_header.as_ref(),
        dav_fs,
        lock_system,
        &path,
        prefix,
        &request_head.origin.scheme,
        &request_head.origin.host,
    )
    .await
    {
        return response;
    }
    if let Err(response) = aster_forge_webdav::actix::enforce_unlocked(
        lock_system,
        &path,
        false,
        prefix,
        request_head.if_header.as_ref(),
        &request_head.origin.scheme,
        &request_head.origin.host,
    )
    .await
    {
        return response;
    }

    let patches = match parse_proppatch_request(body) {
        Ok(patches) => patches,
        Err(response) => return response,
    };
    let results = match dav_fs.patch_props(&path, patches).await {
        Ok(results) => results,
        Err(error) => return fs_error_response(error),
    };
    let response = build_proppatch_item(
        href_for_dav_path(prefix, &path),
        results
            .into_iter()
            .map(|(status, prop)| (status.as_u16(), prop_element(&prop))),
    );
    forge_response(property_multistatus_response(vec![response]))
}

#[expect(
    clippy::too_many_arguments,
    reason = "The stream owns the authenticated product adapters and immutable per-request PROPFIND contract."
)]
fn propfind_item_stream<L>(
    root_item: DavMultiStatusItem,
    depth: Depth,
    root_is_dir: bool,
    root_path: DavPath,
    root_relative: String,
    dav_fs: AsterDavFs,
    lock_system: L,
    prefix: String,
    request_kind: DavPropfindRequest,
    requirements: DavLivePropertyRequirements,
    quota: Option<DavQuotaSnapshot>,
) -> impl Stream<Item = Result<DavMultiStatusItem, DavMultiStatusSourceError>> + Send + 'static
where
    L: DavLockSystem + Clone + Send + Sync + 'static,
{
    async_stream::stream! {
        yield Ok(root_item);
        if depth != Depth::One || !root_is_dir {
            return;
        }

        let mut state = DavDirectoryPageState::new();
        let cancellation = DavNeverCancelled;
        let mut resource_count = 1usize;

        loop {
            let page = match read_next_directory_page(
                &dav_fs,
                &root_path,
                &mut state,
                PROPFIND_PAGE_ENTRIES,
                PROPFIND_DIRECTORY_LIMITS,
                &cancellation,
            )
            .await
            {
                Ok(Some(page)) => page,
                Ok(None) => break,
                Err(DavDirectoryReadError::Cancelled) => {
                    yield Err(DavMultiStatusSourceError::Cancelled);
                    break;
                }
                Err(DavDirectoryReadError::Backend(error)) => {
                    yield Err(DavMultiStatusSourceError::Backend(error));
                    break;
                }
                Err(error) => {
                    tracing::warn!(error = %error, "bounded WebDAV directory enumeration failed");
                    yield Err(DavMultiStatusSourceError::Backend(DavBackendError::new(
                        DavBackendErrorKind::Internal,
                    )));
                    break;
                }
            };

            let mut resources = Vec::with_capacity(page.entries.len());
            for entry in page.entries {
                let is_dir = entry.metadata().is_dir();
                let child_relative = match child_relative_path(
                    &root_relative,
                    entry.name(),
                    is_dir,
                ) {
                    Ok(relative) => relative,
                    Err(error) => {
                        tracing::warn!(error = %error, "invalid WebDAV directory entry path");
                        yield Err(DavMultiStatusSourceError::Backend(DavBackendError::new(
                            DavBackendErrorKind::Internal,
                        )));
                        return;
                    }
                };
                let child_path = match DavPath::new(&child_relative) {
                    Ok(path) => path,
                    Err(error) => {
                        tracing::warn!(error = %error, "invalid WebDAV child path");
                        yield Err(DavMultiStatusSourceError::Backend(DavBackendError::new(
                            DavBackendErrorKind::Internal,
                        )));
                        return;
                    }
                };
                let metadata = match propfind_metadata(&child_relative, entry.metadata()) {
                    Ok(metadata) => metadata,
                    Err(error) => {
                        yield Err(DavMultiStatusSourceError::Backend(DavBackendError::from(error)));
                        return;
                    }
                };
                let resource = if is_dir {
                    DavResourceState::Collection
                } else {
                    DavResourceState::File
                };
                let capabilities = match DriveDavCapabilityProvider::snapshot_for(resource) {
                    Ok(snapshot) => snapshot,
                    Err(error) => {
                        tracing::warn!(error = %error, "failed to plan WebDAV child capabilities");
                        yield Err(DavMultiStatusSourceError::Backend(DavBackendError::new(
                            DavBackendErrorKind::Internal,
                        )));
                        return;
                    }
                };
                resources.push(PropfindPageResource {
                    path: child_path,
                    relative: child_relative,
                    metadata,
                    capabilities,
                });
            }

            let paths = resources
                .iter()
                .map(|resource| resource.path.clone())
                .collect::<Vec<_>>();
            let mut preload = match preload_property_values(
                &dav_fs,
                &lock_system,
                &prefix,
                &paths,
                requirements,
                !matches!(request_kind, DavPropfindRequest::PropName),
            )
            .await
            {
                Ok(preload) => preload,
                Err(error) => {
                    yield Err(DavMultiStatusSourceError::Backend(DavBackendError::from(error)));
                    break;
                }
            };

            for resource in resources {
                resource_count = match resource_count.checked_add(1) {
                    Some(count) if count <= PROPFIND_MAXIMUM_RESOURCES => count,
                    _ => {
                        yield Err(DavMultiStatusSourceError::Backend(DavBackendError::new(
                            DavBackendErrorKind::PayloadTooLarge,
                        )));
                        return;
                    }
                };
                let values = values_for(resource.path, resource.metadata, &mut preload, quota);
                match build_live_propfind_item(
                    href_for_relative(&prefix, &resource.relative),
                    &resource.capabilities,
                    &request_kind,
                    &values,
                ) {
                    Ok(item) => yield Ok(item),
                    Err(error) => {
                        tracing::warn!(error = %error, "failed to render WebDAV child live properties");
                        yield Err(DavMultiStatusSourceError::Backend(DavBackendError::new(
                            DavBackendErrorKind::Internal,
                        )));
                        return;
                    }
                }
            }

            if !page.has_more {
                break;
            }
        }
    }
}

async fn preload_property_values<L: DavLockSystem>(
    dav_fs: &AsterDavFs,
    lock_system: &L,
    prefix: &str,
    paths: &[DavPath],
    requirements: DavLivePropertyRequirements,
    include_property_content: bool,
) -> Result<PropfindPreload, FsError> {
    let dead_properties = if requirements.dead_properties {
        dav_fs
            .get_props_many(paths, include_property_content)
            .await?
    } else {
        HashMap::new()
    };
    let locks = if requirements.locks && include_property_content {
        lock_system
            .discover_many(paths)
            .await
            .into_iter()
            .map(|(path, locks)| {
                let locks = locks.iter().map(|lock| lock_xml(lock, prefix)).collect();
                (path, locks)
            })
            .collect()
    } else {
        HashMap::new()
    };
    Ok(PropfindPreload {
        dead_properties,
        locks,
    })
}

async fn load_quota(
    dav_fs: &AsterDavFs,
    requirements: DavLivePropertyRequirements,
) -> Result<Option<DavQuotaSnapshot>, FsError> {
    if !requirements.quota {
        return Ok(None);
    }
    let (used_bytes, total_bytes) = dav_fs.get_quota().await?;
    Ok(Some(DavQuotaSnapshot {
        used_bytes,
        available_bytes: total_bytes.map(|total| total.saturating_sub(used_bytes)),
    }))
}

fn propfind_metadata(
    relative: &str,
    metadata: &dyn DavMetaData,
) -> Result<PropfindMetadata, FsError> {
    let is_dir = metadata.is_dir();
    Ok(PropfindMetadata {
        creation_date: Some(metadata.created()?),
        display_name: display_name(relative).to_owned(),
        content_length: (!is_dir).then(|| metadata.len()),
        content_type: (!is_dir)
            .then(|| {
                metadata
                    .content_type()
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned)
            })
            .flatten(),
        etag: metadata.etag().map(|etag| format!("\"{etag}\"")),
        last_modified: Some(metadata.modified()?),
    })
}

fn values_for(
    path: DavPath,
    metadata: PropfindMetadata,
    preload: &mut PropfindPreload,
    quota: Option<DavQuotaSnapshot>,
) -> PropfindValues {
    PropfindValues {
        metadata,
        active_locks: preload.locks.remove(&path).unwrap_or_default(),
        dead_properties: preload.dead_properties.remove(&path).unwrap_or_default(),
        quota,
    }
}

fn lock_xml(lock: &DavLock, prefix: &str) -> DavLockXml {
    DavLockXml {
        token: lock.token.clone(),
        owner: lock.owner.as_deref().cloned(),
        timeout: lock.timeout,
        shared: lock.shared,
        deep: lock.deep,
        root_href: href_for_dav_path(prefix, &lock.path),
    }
}

fn parse_propfind_request(body: &[u8]) -> Result<DavPropfindRequest, HttpResponse> {
    aster_forge_webdav::parse_propfind_request(body)
        .map_err(|error| forge_response(propfind_xml_error_response(error)))
}

fn parse_proppatch_request(body: &[u8]) -> Result<Vec<(bool, DavProp)>, HttpResponse> {
    aster_forge_webdav::parse_proppatch_request(body)
        .map_err(|error| forge_response(proppatch_xml_error_response(error)))?
        .into_iter()
        .map(|patch| {
            let xml = patch
                .property
                .element
                .to_bytes()
                .map_err(|_| responses::empty(StatusCode::INTERNAL_SERVER_ERROR))?;
            Ok((
                patch.set,
                DavProp {
                    name: patch.property.name,
                    prefix: patch.property.prefix,
                    namespace: patch.property.namespace,
                    xml: Some(xml),
                },
            ))
        })
        .collect()
}

fn prop_element(prop: &DavProp) -> DavXmlElement {
    let property = aster_forge_webdav::DavRequestedProperty {
        name: prop.name.clone(),
        namespace: prop.namespace.clone(),
        prefix: prop.prefix.clone(),
    };
    dav_dead_property_element(&property, None, prop.xml.as_deref())
}

fn forge_response<E>(response: Result<aster_forge_webdav::DavResponse, E>) -> HttpResponse {
    match response {
        Ok(response) => aster_forge_webdav::actix::into_response(response),
        Err(_) => responses::empty(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

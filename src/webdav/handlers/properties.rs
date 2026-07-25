//! WebDAV PROPFIND / PROPPATCH handlers.

use std::collections::HashMap;
use std::time::Instant;

use actix_web::HttpResponse;
use actix_web::http::StatusCode;
use aster_forge_utils::http_validators::format_http_date;
use aster_forge_webdav::{
    DavMultiStatusItem, DavPropfindRequest, DavRequestHead, DavRequestedProperty, DavXmlElement,
    DavXmlError, build_propfind_item, build_proppatch_item, dav_dead_property_element, dav_element,
    dav_property_child_element, dav_property_name_element, dav_property_text_element,
    format_creation_date, property_multistatus_response, propfind_finite_depth_response,
    propfind_request_label, propfind_xml_error_response, proppatch_xml_error_response,
};
use futures::{StreamExt, pin_mut};

use crate::services::content::property;
use crate::webdav::responses;
use crate::webdav::{
    child_relative_path, display_name, fs_error_response, href_for_dav_path, href_for_relative,
};
use aster_forge_webdav::Depth;
use aster_forge_webdav::{
    DavFileSystem, DavLock, DavLockSystem, DavMetaData, DavPath, DavProp, FsError, ReadDirMeta,
};

struct PropfindResource {
    path: Option<DavPath>,
    relative: String,
    meta: Box<dyn DavMetaData>,
}

#[derive(Default)]
struct PropfindPreload {
    dead_props: HashMap<DavPath, Vec<DavProp>>,
    locks: HashMap<DavPath, Vec<DavLock>>,
}

impl PropfindPreload {
    async fn load(
        dav_fs: &dyn DavFileSystem,
        lock_system: &dyn DavLockSystem,
        request_kind: &DavPropfindRequest,
        resources: &[PropfindResource],
    ) -> Result<Self, HttpResponse> {
        let mut preload = Self::default();

        if propfind_kind_needs_dead_props(request_kind) {
            let targets = resources
                .iter()
                .filter(|resource| !is_root_resource(resource))
                .filter_map(|resource| {
                    let target = resource.meta.property_target()?;
                    Some((resource.path.clone()?, target))
                })
                .collect::<Vec<_>>();
            preload.dead_props = dav_fs
                .get_props_many_for_targets(
                    &targets,
                    propfind_kind_needs_dead_prop_content(request_kind),
                )
                .await
                .map_err(fs_error_response)?;
        }

        if propfind_kind_needs_lockdiscovery(request_kind) {
            let paths = resources
                .iter()
                .filter_map(|resource| resource.path.clone())
                .collect::<Vec<_>>();
            preload.locks = lock_system.discover_many(&paths).await;
        }

        Ok(preload)
    }

    fn dead_props_for(&self, resource: &PropfindResource) -> &[DavProp] {
        resource
            .path
            .as_ref()
            .and_then(|path| self.dead_props.get(path))
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    fn locks_for(&self, resource: &PropfindResource) -> &[DavLock] {
        resource
            .path
            .as_ref()
            .and_then(|path| self.locks.get(path))
            .map(Vec::as_slice)
            .unwrap_or_default()
    }
}

pub(crate) async fn handle_propfind(
    request_head: &DavRequestHead,
    dav_fs: &dyn DavFileSystem,
    lock_system: &dyn DavLockSystem,
    prefix: &str,
    body: &[u8],
) -> HttpResponse {
    let path = request_head.target.clone();
    let relative = path.as_str().to_owned();
    let request_scheme = request_head.origin.scheme.as_str();
    let request_host = request_head.origin.host.as_str();
    if let Err(resp) = aster_forge_webdav::actix::enforce_if_header_with_backends(
        request_head.if_header.as_ref(),
        dav_fs,
        lock_system,
        &path,
        prefix,
        request_scheme,
        request_host,
    )
    .await
    {
        return resp;
    }
    let Some(depth) = request_head.depth else {
        return responses::empty(StatusCode::INTERNAL_SERVER_ERROR);
    };
    let request_kind = match parse_propfind_request(body) {
        Ok(kind) => kind,
        Err(resp) => return resp,
    };

    let request_started_at = Instant::now();
    let metadata_started_at = Instant::now();
    let root_meta = match dav_fs.metadata(&path).await {
        Ok(meta) => meta,
        Err(err) => return fs_error_response(err),
    };
    let metadata_elapsed_ms = metadata_started_at.elapsed().as_millis();
    if depth == Depth::Infinity && root_meta.is_dir() {
        return forge_xml_response(propfind_finite_depth_response());
    }
    let collect_started_at = Instant::now();
    let preload_needs_paths = propfind_kind_needs_dead_props(&request_kind)
        || propfind_kind_needs_lockdiscovery(&request_kind);
    let resources = match collect_propfind_resources(
        dav_fs,
        &path,
        &relative,
        depth,
        root_meta,
        preload_needs_paths,
    )
    .await
    {
        Ok(resources) => resources,
        Err(err) => return fs_error_response(err),
    };
    let collect_elapsed_ms = collect_started_at.elapsed().as_millis();
    let resource_count = resources.len();

    let preload_started_at = Instant::now();
    let preload = match PropfindPreload::load(dav_fs, lock_system, &request_kind, &resources).await
    {
        Ok(preload) => preload,
        Err(resp) => return resp,
    };
    let preload_elapsed_ms = preload_started_at.elapsed().as_millis();

    let render_started_at = Instant::now();
    let mut responses = Vec::with_capacity(resource_count);
    for resource in resources {
        let response = match build_propfind_response(prefix, &request_kind, &preload, resource) {
            Ok(response) => response,
            Err(resp) => return resp,
        };
        responses.push(response);
    }
    tracing::debug!(
        depth = ?depth,
        kind = propfind_request_label(&request_kind),
        resource_count,
        metadata_elapsed_ms,
        collect_elapsed_ms,
        preload_elapsed_ms,
        render_elapsed_ms = render_started_at.elapsed().as_millis(),
        total_elapsed_ms = request_started_at.elapsed().as_millis(),
        "WebDAV PROPFIND completed"
    );

    forge_xml_response(property_multistatus_response(responses))
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
        // The WebDAV mount root is a virtual listing boundary, not a persisted
        // file/folder entity. Dead properties are intentionally unavailable
        // there instead of being backed by an implicit root row.
        return responses::unsupported_root_proppatch();
    }
    let request_scheme = request_head.origin.scheme.as_str();
    let request_host = request_head.origin.host.as_str();
    if let Err(resp) = aster_forge_webdav::actix::enforce_if_header_with_backends(
        request_head.if_header.as_ref(),
        dav_fs,
        lock_system,
        &path,
        prefix,
        request_scheme,
        request_host,
    )
    .await
    {
        return resp;
    }
    if let Err(resp) = aster_forge_webdav::actix::enforce_unlocked(
        lock_system,
        &path,
        false,
        prefix,
        request_head.if_header.as_ref(),
        request_scheme,
        request_host,
    )
    .await
    {
        return resp;
    }

    let patches = match parse_proppatch_request(body) {
        Ok(patches) => patches,
        Err(resp) => return resp,
    };

    let results = match dav_fs.patch_props(&path, patches).await {
        Ok(results) => results,
        Err(err) => return fs_error_response(err),
    };

    let response = build_proppatch_item(
        href_for_dav_path(prefix, &path),
        results
            .into_iter()
            .map(|(status, prop)| (status.as_u16(), prop_element(&prop, None))),
    );
    forge_xml_response(property_multistatus_response(vec![response]))
}

fn parse_propfind_request(body: &[u8]) -> Result<DavPropfindRequest, HttpResponse> {
    aster_forge_webdav::parse_propfind_request(body)
        .map_err(|error| forge_xml_response(propfind_xml_error_response(error)))
}

fn parse_proppatch_request(body: &[u8]) -> Result<Vec<(bool, DavProp)>, HttpResponse> {
    aster_forge_webdav::parse_proppatch_request(body)
        .map_err(|error| forge_xml_response(proppatch_xml_error_response(error)))?
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

fn forge_xml_response(
    response: Result<aster_forge_webdav::DavResponse, DavXmlError>,
) -> HttpResponse {
    match response {
        Ok(response) => aster_forge_webdav::actix::into_response(response),
        Err(_) => responses::empty(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn collect_propfind_resources(
    dav_fs: &dyn DavFileSystem,
    path: &DavPath,
    relative: &str,
    depth: Depth,
    root_meta: Box<dyn DavMetaData>,
    include_paths: bool,
) -> Result<Vec<PropfindResource>, FsError> {
    let root_is_dir = root_meta.is_dir();
    let mut resources = vec![PropfindResource {
        path: include_paths.then(|| path.clone()),
        relative: relative.to_string(),
        meta: root_meta,
    }];

    if depth == Depth::One && root_is_dir {
        let entries = dav_fs.read_dir(path, ReadDirMeta::Data).await?;
        pin_mut!(entries);
        while let Some(entry) = entries.next().await {
            let entry = entry?;
            let meta = entry.metadata().await?;
            let child_relative = child_relative_path(relative, &entry.name(), meta.is_dir());
            let child_path = if include_paths {
                Some(DavPath::new(&child_relative).map_err(|_| FsError::GeneralFailure)?)
            } else {
                None
            };
            resources.push(PropfindResource {
                path: child_path,
                relative: child_relative,
                meta,
            });
        }
    }

    Ok(resources)
}

fn build_propfind_response(
    prefix: &str,
    request_kind: &DavPropfindRequest,
    preload: &PropfindPreload,
    resource: PropfindResource,
) -> Result<DavMultiStatusItem, HttpResponse> {
    let available = available_property_names(&resource, preload);
    build_propfind_item(
        href_for_relative(prefix, &resource.relative),
        request_kind,
        &available,
        |requested| resolve_property(prefix, &resource, requested, preload),
    )
}

fn available_property_names(
    resource: &PropfindResource,
    preload: &PropfindPreload,
) -> Vec<DavRequestedProperty> {
    let mut properties = standard_prop_name_list(resource)
        .into_iter()
        .map(|name| DavRequestedProperty {
            name: name.to_string(),
            namespace: Some("DAV:".to_string()),
            prefix: Some("D".to_string()),
        })
        .collect::<Vec<_>>();
    properties.extend(
        preload
            .dead_props_for(resource)
            .iter()
            .map(|prop| DavRequestedProperty {
                name: prop.name.clone(),
                namespace: prop.namespace.clone(),
                prefix: prop.prefix.clone(),
            }),
    );
    properties
}

fn resolve_property(
    prefix: &str,
    resource: &PropfindResource,
    requested: &DavRequestedProperty,
    preload: &PropfindPreload,
) -> Result<Option<DavXmlElement>, HttpResponse> {
    if is_system_property(requested) {
        return Ok(None);
    }
    if let Some(element) = standard_prop_element(prefix, resource, requested, preload)? {
        return Ok(Some(element));
    }
    Ok(preload
        .dead_props_for(resource)
        .iter()
        .find(|candidate| requested_property_matches(requested, candidate))
        .map(|stored| prop_element(stored, Some(requested))))
}

fn requested_props_may_need_dead_lookup(requested: &[DavRequestedProperty]) -> bool {
    requested.iter().any(requested_prop_may_be_dead_property)
}

fn propfind_kind_needs_dead_props(kind: &DavPropfindRequest) -> bool {
    match kind {
        DavPropfindRequest::AllProp { .. } | DavPropfindRequest::PropName => true,
        DavPropfindRequest::Prop(requested) => requested_props_may_need_dead_lookup(requested),
    }
}

fn propfind_kind_needs_dead_prop_content(kind: &DavPropfindRequest) -> bool {
    !matches!(kind, DavPropfindRequest::PropName)
}

fn propfind_kind_needs_lockdiscovery(kind: &DavPropfindRequest) -> bool {
    match kind {
        DavPropfindRequest::AllProp { .. } => true,
        DavPropfindRequest::PropName => false,
        DavPropfindRequest::Prop(requested) => requested.iter().any(is_lockdiscovery_prop),
    }
}

fn requested_prop_may_be_dead_property(prop: &DavRequestedProperty) -> bool {
    if is_system_property(prop) {
        return false;
    }
    match prop.namespace.as_deref() {
        Some("DAV:") => false,
        Some(_) => true,
        None => !is_standard_live_prop_name(&prop.name),
    }
}

fn is_lockdiscovery_prop(prop: &DavRequestedProperty) -> bool {
    prop.namespace.as_deref().unwrap_or("DAV:") == "DAV:" && prop.name == "lockdiscovery"
}

fn standard_prop_element(
    prefix: &str,
    resource: &PropfindResource,
    requested: &DavRequestedProperty,
    preload: &PropfindPreload,
) -> Result<Option<DavXmlElement>, HttpResponse> {
    if requested.namespace.as_deref().unwrap_or("DAV:") != "DAV:" {
        return Ok(None);
    }

    let property_name = requested.clone();
    match requested.name.as_str() {
        "displayname" => {
            let display = display_name(&resource.relative);
            Ok(Some(if display.is_empty() {
                dav_property_name_element(&property_name)
            } else {
                dav_property_text_element(&property_name, display)
            }))
        }
        "resourcetype" => Ok(Some(if resource.meta.is_dir() {
            dav_property_child_element(&property_name, dav_element("collection"))
        } else {
            dav_property_name_element(&property_name)
        })),
        "getcontentlength" => {
            if resource.meta.is_dir() {
                return Ok(None);
            }
            Ok(Some(dav_property_text_element(
                &property_name,
                resource.meta.len().to_string(),
            )))
        }
        "getcontenttype" => {
            if resource.meta.is_dir() {
                return Ok(None);
            }
            let Some(content_type) = resource
                .meta
                .content_type()
                .filter(|value| !value.is_empty())
            else {
                return Ok(None);
            };
            Ok(Some(dav_property_text_element(
                &property_name,
                content_type,
            )))
        }
        "getlastmodified" => {
            let modified = resource.meta.modified().map_err(fs_error_response)?;
            Ok(Some(dav_property_text_element(
                &property_name,
                format_http_date(modified),
            )))
        }
        "creationdate" => {
            let created = resource.meta.created().map_err(fs_error_response)?;
            Ok(Some(dav_property_text_element(
                &property_name,
                format_creation_date(created),
            )))
        }
        "getetag" => Ok(Some(resource.meta.etag().map_or_else(
            || dav_property_name_element(&property_name),
            |etag| dav_property_text_element(&property_name, format!("\"{etag}\"")),
        ))),
        "supportedlock" => {
            let supported = aster_forge_webdav::dav_supported_lock_element();
            Ok(Some(supported))
        }
        "lockdiscovery" => Ok(Some(aster_forge_webdav::lock_discovery_element(
            preload.locks_for(resource),
            prefix,
        ))),
        _ => Ok(None),
    }
}

fn prop_element(prop: &DavProp, requested: Option<&DavRequestedProperty>) -> DavXmlElement {
    let stored_name = DavRequestedProperty {
        name: prop.name.clone(),
        namespace: prop.namespace.clone(),
        prefix: prop.prefix.clone(),
    };
    dav_dead_property_element(&stored_name, requested, prop.xml.as_deref())
}

fn requested_property_matches(requested: &DavRequestedProperty, stored: &DavProp) -> bool {
    requested.name == stored.name && requested.namespace.as_deref() == stored.namespace.as_deref()
}

fn is_system_property(property_name: &DavRequestedProperty) -> bool {
    property_name
        .namespace
        .as_deref()
        .is_some_and(property::is_system_namespace)
}

fn is_root_resource(resource: &PropfindResource) -> bool {
    // The mount root has no dead-property backing store. PROPFIND may expose
    // its live DAV properties, while PROPPATCH rejects "/" explicitly.
    resource.relative == "/"
}

fn standard_prop_name_list(resource: &PropfindResource) -> Vec<&'static str> {
    let mut props = vec![
        "displayname",
        "resourcetype",
        "getlastmodified",
        "creationdate",
        "getetag",
        "lockdiscovery",
        "supportedlock",
    ];
    if !resource.meta.is_dir() {
        props.insert(2, "getcontentlength");
        props.insert(3, "getcontenttype");
    }
    props
}

fn is_standard_live_prop_name(name: &str) -> bool {
    matches!(
        name,
        "displayname"
            | "resourcetype"
            | "getcontentlength"
            | "getcontenttype"
            | "getlastmodified"
            | "creationdate"
            | "getetag"
            | "lockdiscovery"
            | "supportedlock"
    )
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use std::time::SystemTime;

    use actix_web::body::to_bytes;
    use actix_web::http::{Method, StatusCode, header};
    use actix_web::test::TestRequest;
    use aster_forge_webdav::{DavResourceKind, DavXmlElement};

    use super::handle_propfind;
    use aster_forge_webdav::{
        DavDirEntry, DavFile, DavFileSystem, DavLock, DavLockError, DavLockSystem, DavMetaData,
        DavPath, DavProp, DavPropertyTarget, FsError, FsFuture, FsResult, FsStream, LsFuture,
        OpenOptions, ReadDirMeta,
    };

    struct PropfindTestFs {
        child_count: usize,
        metadata_calls: Arc<AtomicUsize>,
        get_props_calls: Arc<AtomicUsize>,
    }

    struct PropfindTestMeta {
        is_dir: bool,
        len: u64,
        content_type: Option<&'static str>,
        property_target: Option<DavPropertyTarget>,
    }

    impl DavMetaData for PropfindTestMeta {
        fn len(&self) -> u64 {
            self.len
        }

        fn modified(&self) -> FsResult<SystemTime> {
            Ok(SystemTime::UNIX_EPOCH)
        }

        fn is_dir(&self) -> bool {
            self.is_dir
        }

        fn etag(&self) -> Option<String> {
            Some(if self.is_dir {
                "dir-etag".to_string()
            } else {
                format!("file-etag-{}", self.len)
            })
        }

        fn content_type(&self) -> Option<&str> {
            self.content_type
        }

        fn created(&self) -> FsResult<SystemTime> {
            Ok(SystemTime::UNIX_EPOCH)
        }

        fn property_target(&self) -> Option<DavPropertyTarget> {
            self.property_target
        }
    }

    struct PropfindTestEntry {
        name: Vec<u8>,
        len: u64,
    }

    fn contains_element(element: &DavXmlElement, namespace: &str, name: &str) -> bool {
        (element.name == name && element.namespace.as_deref() == Some(namespace))
            || element
                .child_elements()
                .any(|child| contains_element(child, namespace, name))
    }

    impl DavDirEntry for PropfindTestEntry {
        fn name(&self) -> Vec<u8> {
            self.name.clone()
        }

        fn metadata<'a>(&'a self) -> FsFuture<'a, Box<dyn DavMetaData>> {
            Box::pin(async move {
                Ok(Box::new(PropfindTestMeta {
                    is_dir: false,
                    len: self.len,
                    content_type: Some("text/plain"),
                    property_target: Some(DavPropertyTarget {
                        kind: DavResourceKind::File,
                        id: i64::try_from(self.len).expect("test len should fit i64"),
                    }),
                }) as Box<dyn DavMetaData>)
            })
        }
    }

    impl DavFileSystem for PropfindTestFs {
        fn open<'a>(
            &'a self,
            _path: &'a DavPath,
            _options: OpenOptions,
        ) -> FsFuture<'a, Box<dyn DavFile>> {
            Box::pin(async { Err(FsError::GeneralFailure) })
        }

        fn read_dir<'a>(
            &'a self,
            path: &'a DavPath,
            _meta: ReadDirMeta,
        ) -> FsFuture<'a, FsStream<Box<dyn DavDirEntry>>> {
            Box::pin(async move {
                if path.as_str() != "/" {
                    return Err(FsError::NotFound);
                }

                let entries = (0..self.child_count)
                    .map(|index| {
                        Ok(Box::new(PropfindTestEntry {
                            name: format!("file-{index}.txt").into_bytes(),
                            len: u64::try_from(index + 1).expect("test index should fit u64"),
                        }) as Box<dyn DavDirEntry>)
                    })
                    .collect::<Vec<_>>();
                Ok(Box::pin(futures::stream::iter(entries)) as FsStream<Box<dyn DavDirEntry>>)
            })
        }

        fn metadata<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, Box<dyn DavMetaData>> {
            Box::pin(async move {
                self.metadata_calls.fetch_add(1, Ordering::SeqCst);
                if path.as_str() == "/" {
                    return Ok(Box::new(PropfindTestMeta {
                        is_dir: true,
                        len: 0,
                        content_type: None,
                        property_target: None,
                    }) as Box<dyn DavMetaData>);
                }

                Ok(Box::new(PropfindTestMeta {
                    is_dir: false,
                    len: 1,
                    content_type: Some("text/plain"),
                    property_target: Some(DavPropertyTarget {
                        kind: DavResourceKind::File,
                        id: 1,
                    }),
                }) as Box<dyn DavMetaData>)
            })
        }

        fn create_dir<'a>(&'a self, _path: &'a DavPath) -> FsFuture<'a, ()> {
            Box::pin(async { Err(FsError::GeneralFailure) })
        }

        fn remove_dir<'a>(&'a self, _path: &'a DavPath) -> FsFuture<'a, ()> {
            Box::pin(async { Err(FsError::GeneralFailure) })
        }

        fn remove_file<'a>(&'a self, _path: &'a DavPath) -> FsFuture<'a, ()> {
            Box::pin(async { Err(FsError::GeneralFailure) })
        }

        fn rename<'a>(&'a self, _from: &'a DavPath, _to: &'a DavPath) -> FsFuture<'a, ()> {
            Box::pin(async { Err(FsError::GeneralFailure) })
        }

        fn copy<'a>(&'a self, _from: &'a DavPath, _to: &'a DavPath) -> FsFuture<'a, ()> {
            Box::pin(async { Err(FsError::GeneralFailure) })
        }

        fn get_props<'a>(
            &'a self,
            path: &'a DavPath,
            do_content: bool,
        ) -> FsFuture<'a, Vec<DavProp>> {
            Box::pin(async move {
                self.get_props_calls.fetch_add(1, Ordering::SeqCst);
                if path.as_str() == "/" {
                    return Ok(Vec::new());
                }
                Ok(vec![DavProp {
                    name: "color".to_string(),
                    prefix: Some("A".to_string()),
                    namespace: Some("urn:aster:test".to_string()),
                    xml: do_content
                        .then(|| b"<A:color xmlns:A=\"urn:aster:test\">blue</A:color>".to_vec()),
                }])
            })
        }
    }

    struct PropfindTestLockSystem {
        discover_calls: Arc<AtomicUsize>,
        discover_many_calls: Arc<AtomicUsize>,
    }

    impl DavLockSystem for PropfindTestLockSystem {
        fn lock(
            &self,
            _path: &DavPath,
            _principal: Option<&str>,
            _owner: Option<&DavXmlElement>,
            _timeout: Option<std::time::Duration>,
            _shared: bool,
            _deep: bool,
        ) -> LsFuture<'_, Result<DavLock, DavLockError>> {
            Box::pin(async { Err(DavLockError::Backend) })
        }

        fn unlock(&self, _path: &DavPath, _token: &str) -> LsFuture<'_, Result<(), ()>> {
            Box::pin(async { Ok(()) })
        }

        fn refresh(
            &self,
            _path: &DavPath,
            _token: &str,
            _timeout: Option<std::time::Duration>,
        ) -> LsFuture<'_, Result<DavLock, ()>> {
            Box::pin(async { Err(()) })
        }

        fn check(
            &self,
            _path: &DavPath,
            _principal: Option<&str>,
            _ignore_principal: bool,
            _deep: bool,
            _submitted_tokens: &[String],
        ) -> LsFuture<'_, Result<(), DavLock>> {
            Box::pin(async { Ok(()) })
        }

        fn discover(&self, _path: &DavPath) -> LsFuture<'_, Vec<DavLock>> {
            Box::pin(async move {
                self.discover_calls.fetch_add(1, Ordering::SeqCst);
                Vec::new()
            })
        }

        fn discover_many<'a>(
            &'a self,
            paths: &'a [DavPath],
        ) -> LsFuture<'a, HashMap<DavPath, Vec<DavLock>>> {
            Box::pin(async move {
                self.discover_many_calls.fetch_add(1, Ordering::SeqCst);
                paths
                    .iter()
                    .map(|path| (path.clone(), Vec::new()))
                    .collect::<HashMap<_, _>>()
            })
        }

        fn conflicting_locks(&self, _path: &DavPath, _deep: bool) -> LsFuture<'_, Vec<DavLock>> {
            Box::pin(async { Vec::new() })
        }

        fn delete(&self, _path: &DavPath) -> LsFuture<'_, Result<(), ()>> {
            Box::pin(async { Ok(()) })
        }
    }

    async fn propfind_depth_one(body: &'static str) -> (String, usize, usize, usize, usize) {
        const CHILD_COUNT: usize = 24;

        let metadata_calls = Arc::new(AtomicUsize::new(0));
        let get_props_calls = Arc::new(AtomicUsize::new(0));
        let discover_calls = Arc::new(AtomicUsize::new(0));
        let discover_many_calls = Arc::new(AtomicUsize::new(0));
        let fs = PropfindTestFs {
            child_count: CHILD_COUNT,
            metadata_calls: metadata_calls.clone(),
            get_props_calls: get_props_calls.clone(),
        };
        let lock_system = PropfindTestLockSystem {
            discover_calls: discover_calls.clone(),
            discover_many_calls: discover_many_calls.clone(),
        };
        let req = TestRequest::default()
            .method(Method::from_bytes(b"PROPFIND").expect("valid method"))
            .uri("/webdav/")
            .insert_header((header::HeaderName::from_static("depth"), "1"))
            .to_http_request();

        let request_head = aster_forge_webdav::actix::request_head(&req, "/webdav")
            .expect("test request head should parse")
            .expect("PROPFIND should be supported");
        let response =
            handle_propfind(&request_head, &fs, &lock_system, "/webdav", body.as_bytes()).await;
        assert_eq!(response.status(), StatusCode::MULTI_STATUS);
        let body = to_bytes(response.into_body())
            .await
            .expect("PROPFIND response body should be readable");
        (
            String::from_utf8(body.to_vec()).expect("PROPFIND body should be utf-8"),
            metadata_calls.load(Ordering::SeqCst),
            get_props_calls.load(Ordering::SeqCst),
            discover_calls.load(Ordering::SeqCst),
            discover_many_calls.load(Ordering::SeqCst),
        )
    }

    #[actix_web::test]
    async fn propfind_depth_one_live_props_do_not_load_dead_properties() {
        let body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:displayname />
    <D:resourcetype />
    <D:getcontentlength />
    <D:getcontenttype />
    <D:getlastmodified />
    <D:creationdate />
    <D:getetag />
  </D:prop>
</D:propfind>"#;

        let (xml, metadata_calls, calls, discover_calls, discover_many_calls) =
            propfind_depth_one(body).await;

        assert_eq!(
            metadata_calls, 1,
            "Depth: 1 PROPFIND should reuse root metadata instead of loading it twice: {xml}"
        );
        assert_eq!(
            calls, 0,
            "live-property-only Depth: 1 PROPFIND should not load dead properties: {xml}"
        );
        assert_eq!(
            xml.matches("<D:response>").count(),
            25,
            "large-directory fixture should include parent plus all children: {xml}"
        );
        assert!(
            xml.contains("file-23.txt") && xml.contains("getlastmodified"),
            "live property response should still include child resources and requested live props: {xml}"
        );
        assert!(
            xml.contains("<D:getcontenttype>text/plain</D:getcontenttype>"),
            "file live property response should include stored content type: {xml}"
        );
        assert_eq!(discover_calls, 0, "live props should not discover locks");
        assert_eq!(
            discover_many_calls, 0,
            "live props should not batch-discover locks"
        );
    }

    #[actix_web::test]
    async fn propfind_depth_one_custom_prop_still_loads_dead_properties() {
        let body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propfind xmlns:D="DAV:" xmlns:A="urn:aster:test">
  <D:prop>
    <D:displayname />
    <A:color />
  </D:prop>
</D:propfind>"#;

        let (xml, _, calls, _, _) = propfind_depth_one(body).await;

        assert_eq!(
            calls, 24,
            "custom prop lookup should still load child dead properties for Depth: 1: {xml}"
        );
        assert!(
            xml.contains("<A:color xmlns:A=\"urn:aster:test\">blue</A:color>"),
            "custom dead property should still be returned: {xml}"
        );
    }

    #[actix_web::test]
    async fn propfind_depth_one_allprop_still_loads_dead_properties() {
        let (xml, _, calls, _, discover_many_calls) = propfind_depth_one("").await;

        assert_eq!(
            calls, 24,
            "allprop must continue loading child dead properties for Depth: 1: {xml}"
        );
        assert!(
            xml.contains("<A:color xmlns:A=\"urn:aster:test\">blue</A:color>"),
            "allprop should include custom dead properties: {xml}"
        );
        assert_eq!(
            discover_many_calls, 1,
            "allprop should batch-load lockdiscovery once"
        );
    }

    #[actix_web::test]
    async fn propfind_depth_one_propname_does_not_load_lock_values() {
        let body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propfind xmlns:D="DAV:">
  <D:propname />
</D:propfind>"#;

        let (xml, _, _, discover_calls, discover_many_calls) = propfind_depth_one(body).await;

        let response = DavXmlElement::parse(xml.as_bytes())
            .expect("PROPFIND propname response should be valid XML");
        assert!(
            contains_element(&response, "DAV:", "lockdiscovery"),
            "propname should list lockdiscovery as a live property name: {xml}"
        );
        assert_eq!(
            discover_calls, 0,
            "propname must not load per-resource lock values"
        );
        assert_eq!(
            discover_many_calls, 0,
            "propname must not batch-load lock values"
        );
    }

    #[actix_web::test]
    async fn propfind_depth_one_lockdiscovery_uses_batch_discovery() {
        let body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:lockdiscovery />
  </D:prop>
</D:propfind>"#;

        let (xml, _, _, discover_calls, discover_many_calls) = propfind_depth_one(body).await;

        assert!(
            xml.contains("lockdiscovery"),
            "explicit lockdiscovery request should return lockdiscovery elements: {xml}"
        );
        assert_eq!(
            discover_calls, 0,
            "lockdiscovery should not fall back to per-resource discover calls"
        );
        assert_eq!(
            discover_many_calls, 1,
            "Depth: 1 lockdiscovery should use one batch discovery"
        );
    }
}

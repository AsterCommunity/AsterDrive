//! WebDAV PROPFIND / PROPPATCH handlers.

use std::collections::BTreeSet;
use std::collections::{BTreeMap, HashMap};
use std::io::Cursor;

use actix_web::http::StatusCode;
use actix_web::{HttpRequest, HttpResponse};
use futures::{StreamExt, pin_mut};
use xmltree::{Element, XMLNode};

use crate::services::property_service;
use crate::webdav::dav::{
    DavFileSystem, DavLock, DavLockSystem, DavMetaData, DavPath, DavProp, FsError, ReadDirMeta,
};
use crate::webdav::locks::{lockdiscovery_element, supportedlock_element};
use crate::webdav::protocol::{self, Depth};
use crate::webdav::responses;
use crate::webdav::{
    child_elements, child_relative_path, dav_element, display_name, ensure_unlocked,
    format_creation_date, format_http_date, fs_error_response, href_for_dav_path,
    href_for_relative, multi_status, request_origin, request_path, status_element, text_element,
    xml_bytes, xml_response,
};

#[derive(Clone)]
struct RequestedProp {
    name: String,
    namespace: Option<String>,
    prefix: Option<String>,
}

enum PropfindKind {
    AllProp { include: Vec<RequestedProp> },
    PropName,
    Prop(Vec<RequestedProp>),
}

struct PropfindResource {
    path: DavPath,
    relative: String,
    meta: Box<dyn DavMetaData>,
}

#[derive(Default)]
struct PropfindPreload {
    dead_props: HashMap<DavPath, Vec<DavProp>>,
    locks: HashMap<DavPath, Vec<DavLock>>,
}

impl RequestedProp {
    fn from(element: &Element) -> Self {
        Self {
            name: element.name.clone(),
            namespace: element.namespace.clone(),
            prefix: element.prefix.clone(),
        }
    }

    fn empty_element(&self) -> Element {
        let prefix = self
            .prefix
            .as_deref()
            .unwrap_or_else(|| default_prefix(self.namespace.as_deref()));
        let tag = if self.namespace.is_some() {
            format!("{prefix}:{}", self.name)
        } else {
            self.name.clone()
        };
        let mut element = Element::new(&tag);
        if let Some(namespace) = &self.namespace
            && should_declare_namespace(prefix, namespace)
        {
            element
                .attributes
                .insert(format!("xmlns:{prefix}"), namespace.clone());
        }
        element
    }

    fn matches(&self, prop: &DavProp) -> bool {
        self.name == prop.name && self.namespace.as_deref() == prop.namespace.as_deref()
    }

    fn key(&self) -> (String, Option<String>) {
        (self.name.clone(), self.namespace.clone())
    }

    fn is_system_namespace(&self) -> bool {
        self.namespace
            .as_deref()
            .is_some_and(property_service::is_system_namespace)
    }
}

impl PropfindPreload {
    async fn load(
        dav_fs: &dyn DavFileSystem,
        lock_system: &dyn DavLockSystem,
        request_kind: &PropfindKind,
        resources: &[PropfindResource],
    ) -> Result<Self, HttpResponse> {
        let mut preload = Self::default();

        if propfind_kind_needs_dead_props(request_kind) {
            let targets = resources
                .iter()
                .filter(|resource| !is_root_resource(resource))
                .filter_map(|resource| {
                    let (entity_type, entity_id) = resource.meta.property_entity()?;
                    Some((resource.path.clone(), entity_type, entity_id))
                })
                .collect::<Vec<_>>();
            preload.dead_props = dav_fs
                .get_props_many_for_entities(
                    &targets,
                    propfind_kind_needs_dead_prop_content(request_kind),
                )
                .await
                .map_err(fs_error_response)?;
        }

        if propfind_kind_needs_lockdiscovery(request_kind) {
            let paths = resources
                .iter()
                .map(|resource| resource.path.clone())
                .collect::<Vec<_>>();
            preload.locks = lock_system.discover_many(&paths).await;
        }

        Ok(preload)
    }

    fn dead_props_for(&self, resource: &PropfindResource) -> &[DavProp] {
        self.dead_props
            .get(&resource.path)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    fn locks_for(&self, resource: &PropfindResource) -> &[DavLock] {
        self.locks
            .get(&resource.path)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }
}

pub(crate) async fn handle_propfind(
    req: &HttpRequest,
    dav_fs: &dyn DavFileSystem,
    lock_system: &dyn DavLockSystem,
    prefix: &str,
    body: &[u8],
) -> HttpResponse {
    let (path, relative) = match request_path(req, prefix) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let (request_scheme, request_host) = request_origin(req);
    if let Err(resp) = protocol::ensure_if_header(
        req.headers(),
        dav_fs,
        lock_system,
        &path,
        prefix,
        &request_scheme,
        &request_host,
    )
    .await
    {
        return resp;
    }
    let depth = match protocol::parse_propfind_depth(req.headers()) {
        Ok(depth) => depth,
        Err(resp) => return resp,
    };
    let request_kind = match parse_propfind_request(body) {
        Ok(kind) => kind,
        Err(resp) => return resp,
    };

    let root_meta = match dav_fs.metadata(&path).await {
        Ok(meta) => meta,
        Err(err) => return fs_error_response(err),
    };
    if depth == Depth::Infinity && root_meta.is_dir() {
        return responses::propfind_finite_depth_response();
    }
    let resources = match collect_propfind_resources(dav_fs, &path, &relative, depth).await {
        Ok(resources) => resources,
        Err(err) => return fs_error_response(err),
    };
    let preload = match PropfindPreload::load(dav_fs, lock_system, &request_kind, &resources).await
    {
        Ok(preload) => preload,
        Err(resp) => return resp,
    };

    let mut multistatus = dav_element("multistatus");
    multistatus
        .attributes
        .insert("xmlns:D".to_string(), "DAV:".to_string());

    for resource in resources {
        let response =
            match build_propfind_response(prefix, &request_kind, &preload, resource).await {
                Ok(response) => response,
                Err(resp) => return resp,
            };
        multistatus.children.push(XMLNode::Element(response));
    }

    xml_response(multistatus, multi_status())
}

pub(crate) async fn handle_proppatch(
    req: &HttpRequest,
    dav_fs: &dyn DavFileSystem,
    lock_system: &dyn DavLockSystem,
    prefix: &str,
    body: &[u8],
) -> HttpResponse {
    let (path, _) = match request_path(req, prefix) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    if path.as_str() == "/" {
        // The WebDAV mount root is a virtual listing boundary, not a persisted
        // file/folder entity. Dead properties are intentionally unavailable
        // there instead of being backed by an implicit root row.
        return responses::unsupported_root_proppatch();
    }
    let (request_scheme, request_host) = request_origin(req);
    if let Err(resp) = protocol::ensure_if_header(
        req.headers(),
        dav_fs,
        lock_system,
        &path,
        prefix,
        &request_scheme,
        &request_host,
    )
    .await
    {
        return resp;
    }
    if let Err(resp) = ensure_unlocked(
        lock_system,
        &path,
        false,
        prefix,
        req.headers(),
        &request_scheme,
        &request_host,
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

    let mut multistatus = dav_element("multistatus");
    multistatus
        .attributes
        .insert("xmlns:D".to_string(), "DAV:".to_string());

    let mut response = dav_element("response");
    response.children.push(XMLNode::Element(text_element(
        "D:href",
        &href_for_dav_path(prefix, &path),
    )));

    let mut groups: BTreeMap<u16, Vec<DavProp>> = BTreeMap::new();
    for (status, prop) in results {
        groups.entry(status.as_u16()).or_default().push(prop);
    }

    for (status_code, props) in groups {
        let status = StatusCode::from_u16(status_code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let mut propstat = dav_element("propstat");
        let mut prop = dav_element("prop");
        for item in props {
            prop.children
                .push(XMLNode::Element(prop_element(&item, None)));
        }
        propstat.children.push(XMLNode::Element(prop));
        propstat
            .children
            .push(XMLNode::Element(status_element(status)));
        response.children.push(XMLNode::Element(propstat));
    }

    multistatus.children.push(XMLNode::Element(response));
    xml_response(multistatus, multi_status())
}

fn parse_propfind_request(body: &[u8]) -> Result<PropfindKind, HttpResponse> {
    if body.is_empty() {
        return Ok(PropfindKind::AllProp {
            include: Vec::new(),
        });
    }

    crate::webdav::reject_xml_dtd_or_entity(body).map_err(|_| responses::no_external_entities())?;
    let root = Element::parse(Cursor::new(body)).map_err(|_| responses::invalid_xml_body())?;
    if root.name != "propfind" {
        return Err(invalid_propfind_body());
    }

    let mut kind = None;
    let mut include = Vec::new();

    for child in child_elements(&root) {
        match child.name.as_str() {
            "propname" => {
                if kind.is_some() {
                    return Err(invalid_propfind_body());
                }
                kind = Some(PropfindKind::PropName);
            }
            "allprop" => {
                if kind.is_some() {
                    return Err(invalid_propfind_body());
                }
                kind = Some(PropfindKind::AllProp {
                    include: Vec::new(),
                });
            }
            "include" => {
                if !matches!(kind.as_ref(), Some(PropfindKind::AllProp { .. })) {
                    return Err(invalid_propfind_body());
                }
                include.extend(child_elements(child).map(RequestedProp::from));
            }
            "prop" => {
                if kind.is_some() {
                    return Err(invalid_propfind_body());
                }
                let props = child_elements(child).map(RequestedProp::from).collect();
                kind = Some(PropfindKind::Prop(props));
            }
            _ => return Err(invalid_propfind_body()),
        }
    }

    match kind {
        Some(PropfindKind::AllProp { .. }) => Ok(PropfindKind::AllProp { include }),
        Some(kind) => {
            if include.is_empty() {
                Ok(kind)
            } else {
                Err(invalid_propfind_body())
            }
        }
        None => Ok(PropfindKind::AllProp {
            include: Vec::new(),
        }),
    }
}

fn parse_proppatch_request(body: &[u8]) -> Result<Vec<(bool, DavProp)>, HttpResponse> {
    crate::webdav::reject_xml_dtd_or_entity(body).map_err(|_| responses::no_external_entities())?;
    let root = Element::parse(Cursor::new(body)).map_err(|_| responses::invalid_xml_body())?;
    if root.name != "propertyupdate" {
        return Err(invalid_proppatch_body());
    }

    let root_lang = xml_lang_value(&root).map(str::to_string);
    let mut patches = Vec::new();
    for action in child_elements(&root) {
        let action_lang = xml_lang_value(action).or(root_lang.as_deref());
        let set = match action.name.as_str() {
            "set" => true,
            "remove" => false,
            _ => return Err(invalid_proppatch_body()),
        };
        let mut action_children = child_elements(action);
        let Some(prop_container) = action_children.next() else {
            return Err(invalid_proppatch_body());
        };
        if prop_container.name != "prop" || action_children.next().is_some() {
            return Err(invalid_proppatch_body());
        }
        let prop_container_lang = xml_lang_value(prop_container).or(action_lang);
        for prop in child_elements(prop_container) {
            let inherited_lang = xml_lang_value(prop).or(prop_container_lang);
            patches.push((set, prop_from_xml(prop, inherited_lang)));
        }
    }
    if patches.is_empty() {
        return Err(invalid_proppatch_body());
    }
    Ok(patches)
}

fn invalid_propfind_body() -> HttpResponse {
    responses::bad_request_text("Invalid PROPFIND body")
}

fn invalid_proppatch_body() -> HttpResponse {
    responses::bad_request_text("Invalid PROPPATCH body")
}

async fn collect_propfind_resources(
    dav_fs: &dyn DavFileSystem,
    path: &DavPath,
    relative: &str,
    depth: Depth,
) -> Result<Vec<PropfindResource>, FsError> {
    let root_meta = dav_fs.metadata(path).await?;
    let root_is_dir = root_meta.is_dir();
    let mut resources = vec![PropfindResource {
        path: path.clone(),
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
            let child_path = DavPath::new(&child_relative).map_err(|_| FsError::GeneralFailure)?;
            resources.push(PropfindResource {
                path: child_path,
                relative: child_relative,
                meta,
            });
        }
    }

    Ok(resources)
}

async fn build_propfind_response(
    prefix: &str,
    request_kind: &PropfindKind,
    preload: &PropfindPreload,
    resource: PropfindResource,
) -> Result<Element, HttpResponse> {
    let mut response = dav_element("response");
    response.children.push(XMLNode::Element(text_element(
        "D:href",
        &href_for_relative(prefix, &resource.relative),
    )));

    let propstats = match request_kind {
        PropfindKind::AllProp { include } => {
            all_propstat_elements(prefix, &resource, include, preload).await?
        }
        PropfindKind::PropName => {
            vec![(StatusCode::OK, prop_name_elements(&resource, preload))]
        }
        PropfindKind::Prop(requested) => {
            requested_prop_elements(prefix, &resource, requested, preload).await?
        }
    };

    for (status, props) in propstats {
        if props.is_empty() {
            continue;
        }
        let mut propstat = dav_element("propstat");
        let mut prop = dav_element("prop");
        for item in props {
            prop.children.push(XMLNode::Element(item));
        }
        propstat.children.push(XMLNode::Element(prop));
        propstat
            .children
            .push(XMLNode::Element(status_element(status)));
        response.children.push(XMLNode::Element(propstat));
    }

    Ok(response)
}

async fn all_prop_elements(
    prefix: &str,
    resource: &PropfindResource,
    preload: &PropfindPreload,
) -> Result<(Vec<Element>, BTreeSet<(String, Option<String>)>), HttpResponse> {
    let mut props = standard_prop_name_list(resource)
        .into_iter()
        .map(|prop| RequestedProp {
            name: prop.to_string(),
            namespace: Some("DAV:".to_string()),
            prefix: Some("D".to_string()),
        })
        .collect::<Vec<_>>();
    let mut keys = props
        .iter()
        .map(RequestedProp::key)
        .collect::<BTreeSet<_>>();
    let custom_props = preload.dead_props_for(resource);
    let mut elements = Vec::new();
    for requested in props.drain(..) {
        if let Some(element) = standard_prop_element(prefix, resource, &requested, preload).await? {
            elements.push(element);
        }
    }
    for prop in custom_props {
        keys.insert(dav_prop_key(&prop));
        elements.push(prop_element(&prop, None));
    }
    Ok((elements, keys))
}

async fn all_propstat_elements(
    prefix: &str,
    resource: &PropfindResource,
    include: &[RequestedProp],
    preload: &PropfindPreload,
) -> Result<Vec<(StatusCode, Vec<Element>)>, HttpResponse> {
    let (all_props, all_prop_keys) = all_prop_elements(prefix, resource, preload).await?;
    if include.is_empty() {
        return Ok(vec![(StatusCode::OK, all_props)]);
    }

    let include = include
        .iter()
        .filter(|prop| !all_prop_keys.contains(&prop.key()))
        .cloned()
        .collect::<Vec<_>>();
    if include.is_empty() {
        return Ok(vec![(StatusCode::OK, all_props)]);
    }

    let mut result = vec![(StatusCode::OK, all_props)];
    let requested = requested_prop_elements(prefix, resource, &include, preload).await?;
    for (status, props) in requested {
        if !props.is_empty() {
            result.push((status, props));
        }
    }
    Ok(result)
}

fn prop_name_elements(resource: &PropfindResource, preload: &PropfindPreload) -> Vec<Element> {
    let mut elements = Vec::new();
    for name in standard_prop_name_list(resource) {
        let requested = RequestedProp {
            name: name.to_string(),
            namespace: Some("DAV:".to_string()),
            prefix: Some("D".to_string()),
        };
        elements.push(requested.empty_element());
    }
    for prop in preload.dead_props_for(resource) {
        elements.push(prop_element(
            prop,
            Some(&RequestedProp {
                name: prop.name.clone(),
                namespace: prop.namespace.clone(),
                prefix: prop.prefix.clone(),
            }),
        ));
    }
    elements
}

async fn requested_prop_elements(
    prefix: &str,
    resource: &PropfindResource,
    requested: &[RequestedProp],
    preload: &PropfindPreload,
) -> Result<Vec<(StatusCode, Vec<Element>)>, HttpResponse> {
    let custom_props = preload.dead_props_for(resource);
    let mut ok = Vec::new();
    let mut missing = Vec::new();

    for prop in requested {
        if prop.is_system_namespace() {
            missing.push(prop.empty_element());
            continue;
        }

        if let Some(element) = standard_prop_element(prefix, resource, prop, preload).await? {
            ok.push(element);
            continue;
        }

        if let Some(stored) = custom_props
            .iter()
            .find(|candidate| prop.matches(candidate))
        {
            ok.push(prop_element(stored, Some(prop)));
        } else {
            missing.push(prop.empty_element());
        }
    }

    let mut result = Vec::new();
    if !ok.is_empty() {
        result.push((StatusCode::OK, ok));
    }
    if !missing.is_empty() {
        result.push((StatusCode::NOT_FOUND, missing));
    }
    Ok(result)
}

fn requested_props_may_need_dead_lookup(requested: &[RequestedProp]) -> bool {
    requested.iter().any(requested_prop_may_be_dead_property)
}

fn propfind_kind_needs_dead_props(kind: &PropfindKind) -> bool {
    match kind {
        PropfindKind::AllProp { .. } | PropfindKind::PropName => true,
        PropfindKind::Prop(requested) => requested_props_may_need_dead_lookup(requested),
    }
}

fn propfind_kind_needs_dead_prop_content(kind: &PropfindKind) -> bool {
    !matches!(kind, PropfindKind::PropName)
}

fn propfind_kind_needs_lockdiscovery(kind: &PropfindKind) -> bool {
    match kind {
        PropfindKind::AllProp { .. } => true,
        PropfindKind::PropName => false,
        PropfindKind::Prop(requested) => requested.iter().any(is_lockdiscovery_prop),
    }
}

fn requested_prop_may_be_dead_property(prop: &RequestedProp) -> bool {
    if prop.is_system_namespace() {
        return false;
    }
    match prop.namespace.as_deref() {
        Some("DAV:") => false,
        Some(_) => true,
        None => !is_standard_live_prop_name(&prop.name),
    }
}

fn is_lockdiscovery_prop(prop: &RequestedProp) -> bool {
    prop.namespace.as_deref().unwrap_or("DAV:") == "DAV:" && prop.name == "lockdiscovery"
}

async fn standard_prop_element(
    prefix: &str,
    resource: &PropfindResource,
    requested: &RequestedProp,
    preload: &PropfindPreload,
) -> Result<Option<Element>, HttpResponse> {
    if requested.namespace.as_deref().unwrap_or("DAV:") != "DAV:" {
        return Ok(None);
    }

    let mut element = requested.empty_element();
    match requested.name.as_str() {
        "displayname" => {
            let display = display_name(&resource.relative);
            if !display.is_empty() {
                element.children.push(XMLNode::Text(display.to_string()));
            }
            Ok(Some(element))
        }
        "resourcetype" => {
            if resource.meta.is_dir() {
                element
                    .children
                    .push(XMLNode::Element(dav_element("collection")));
            }
            Ok(Some(element))
        }
        "getcontentlength" => {
            if resource.meta.is_dir() {
                return Ok(None);
            }
            element
                .children
                .push(XMLNode::Text(resource.meta.len().to_string()));
            Ok(Some(element))
        }
        "getlastmodified" => {
            let modified = resource.meta.modified().map_err(fs_error_response)?;
            element
                .children
                .push(XMLNode::Text(format_http_date(modified)));
            Ok(Some(element))
        }
        "creationdate" => {
            let created = resource.meta.created().map_err(fs_error_response)?;
            element
                .children
                .push(XMLNode::Text(format_creation_date(created)));
            Ok(Some(element))
        }
        "getetag" => {
            if let Some(etag) = resource.meta.etag() {
                element.children.push(XMLNode::Text(format!("\"{etag}\"")));
            }
            Ok(Some(element))
        }
        "supportedlock" => {
            let supported = supportedlock_element();
            Ok(Some(supported))
        }
        "lockdiscovery" => Ok(Some(lockdiscovery_element(
            preload.locks_for(resource),
            prefix,
        ))),
        _ => Ok(None),
    }
}

fn prop_from_xml(prop: &Element, inherited_lang: Option<&str>) -> DavProp {
    let mut prop = prop.clone();
    if let Some(lang) = inherited_lang
        && !lang.is_empty()
    {
        prop.attributes
            .entry("xml:lang".to_string())
            .or_insert_with(|| lang.to_string());
    }

    DavProp {
        name: prop.name.clone(),
        prefix: prop.prefix.clone(),
        namespace: prop.namespace.clone(),
        xml: xml_bytes(&prop).ok(),
    }
}

fn prop_element(prop: &DavProp, requested: Option<&RequestedProp>) -> Element {
    let requested_prefix = requested.and_then(|prop| prop.prefix.as_deref());
    let requested_namespace = requested.and_then(|prop| prop.namespace.as_deref());
    let namespace = requested_namespace.or(prop.namespace.as_deref());
    let prefix = requested_prefix
        .or(prop.prefix.as_deref())
        .unwrap_or_else(|| default_prefix(namespace));
    let tag = if namespace.is_some() {
        format!("{prefix}:{}", prop.name)
    } else {
        prop.name.clone()
    };
    let mut element = Element::new(&tag);
    if let Some(namespace) = namespace
        && should_declare_namespace(prefix, namespace)
    {
        element
            .attributes
            .insert(format!("xmlns:{prefix}"), namespace.to_string());
    }
    append_stored_property_data(&mut element, prop);
    element
}

fn dav_prop_key(prop: &DavProp) -> (String, Option<String>) {
    (prop.name.clone(), prop.namespace.clone())
}

fn append_stored_property_data(element: &mut Element, prop: &DavProp) {
    let Some(xml) = &prop.xml else {
        return;
    };
    if xml.is_empty() {
        return;
    }

    if let Ok(stored) = Element::parse(Cursor::new(xml))
        && stored.name == prop.name
        && stored.namespace.as_deref() == prop.namespace.as_deref()
    {
        copy_dead_property_attributes(element, &stored);
        element.children.extend(stored.children);
        return;
    }

    element
        .children
        .push(XMLNode::Text(String::from_utf8_lossy(xml).into_owned()));
}

fn copy_dead_property_attributes(target: &mut Element, stored: &Element) {
    for (key, value) in &stored.attributes {
        if key.starts_with("xmlns") {
            continue;
        }
        let key = if key == "lang" {
            "xml:lang"
        } else {
            key.as_str()
        };
        target
            .attributes
            .entry(key.to_string())
            .or_insert_with(|| value.clone());
    }
}

fn xml_lang_value(element: &Element) -> Option<&str> {
    element
        .attributes
        .get("xml:lang")
        .or_else(|| element.attributes.get("lang"))
        .map(String::as_str)
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
    }
    props
}

fn is_standard_live_prop_name(name: &str) -> bool {
    matches!(
        name,
        "displayname"
            | "resourcetype"
            | "getcontentlength"
            | "getlastmodified"
            | "creationdate"
            | "getetag"
            | "lockdiscovery"
            | "supportedlock"
    )
}

fn default_prefix(namespace: Option<&str>) -> &str {
    match namespace {
        Some("DAV:") => "D",
        Some(_) => "A",
        None => "",
    }
}

fn should_declare_namespace(prefix: &str, namespace: &str) -> bool {
    namespace != "DAV:" || prefix != "D"
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
    use xmltree::Element;

    use super::handle_propfind;
    use crate::webdav::dav::{
        DavDirEntry, DavFile, DavFileSystem, DavLock, DavLockError, DavLockSystem, DavMetaData,
        DavPath, DavProp, FsError, FsFuture, FsResult, FsStream, LsFuture, OpenOptions,
        ReadDirMeta,
    };

    struct PropfindTestFs {
        child_count: usize,
        get_props_calls: Arc<AtomicUsize>,
    }

    struct PropfindTestMeta {
        is_dir: bool,
        len: u64,
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

        fn created(&self) -> FsResult<SystemTime> {
            Ok(SystemTime::UNIX_EPOCH)
        }
    }

    struct PropfindTestEntry {
        name: Vec<u8>,
        len: u64,
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
                if path.as_str() == "/" {
                    return Ok(Box::new(PropfindTestMeta {
                        is_dir: true,
                        len: 0,
                    }) as Box<dyn DavMetaData>);
                }

                Ok(Box::new(PropfindTestMeta {
                    is_dir: false,
                    len: 1,
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
            _owner: Option<&Element>,
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

    async fn propfind_depth_one(body: &'static str) -> (String, usize, usize, usize) {
        const CHILD_COUNT: usize = 24;

        let get_props_calls = Arc::new(AtomicUsize::new(0));
        let discover_calls = Arc::new(AtomicUsize::new(0));
        let discover_many_calls = Arc::new(AtomicUsize::new(0));
        let fs = PropfindTestFs {
            child_count: CHILD_COUNT,
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

        let response = handle_propfind(&req, &fs, &lock_system, "/webdav", body.as_bytes()).await;
        assert_eq!(response.status(), StatusCode::MULTI_STATUS);
        let body = to_bytes(response.into_body())
            .await
            .expect("PROPFIND response body should be readable");
        (
            String::from_utf8(body.to_vec()).expect("PROPFIND body should be utf-8"),
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
    <D:getlastmodified />
    <D:creationdate />
    <D:getetag />
  </D:prop>
</D:propfind>"#;

        let (xml, calls, discover_calls, discover_many_calls) = propfind_depth_one(body).await;

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

        let (xml, calls, _, _) = propfind_depth_one(body).await;

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
        let (xml, calls, _, discover_many_calls) = propfind_depth_one("").await;

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

        let (xml, _, discover_calls, discover_many_calls) = propfind_depth_one(body).await;

        assert!(
            xml.contains("<D:lockdiscovery />"),
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

        let (xml, _, discover_calls, discover_many_calls) = propfind_depth_one(body).await;

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

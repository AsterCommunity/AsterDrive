//! WebDAV PROPFIND / PROPPATCH handlers.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::io::Cursor;

use actix_web::http::StatusCode;
use actix_web::{HttpRequest, HttpResponse};
use futures::{StreamExt, pin_mut};
use xmltree::{Element, XMLNode};

use crate::services::property_service;
use crate::webdav::dav::{
    DavFileSystem, DavLockSystem, DavMetaData, DavPath, DavProp, FsError, ReadDirMeta,
};
use crate::webdav::locks::{lockdiscovery_element, supportedlock_element};
use crate::webdav::protocol::{self, Depth};
use crate::webdav::{
    child_elements, child_relative_path, dav_element, display_name, ensure_unlocked,
    format_creation_date, format_http_date, fs, fs_error_response, href_for_dav_path,
    href_for_relative, multi_status, request_path, status_element, text_element, xml_bytes,
    xml_response,
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

pub(crate) async fn handle_propfind(
    req: &HttpRequest,
    dav_fs: &fs::AsterDavFs,
    lock_system: &dyn DavLockSystem,
    prefix: &str,
    body: &[u8],
) -> HttpResponse {
    let (path, relative) = match request_path(req, prefix) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let connection = req.connection_info();
    let request_scheme = connection.scheme().to_string();
    let request_host = connection.host().to_string();
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
        return propfind_finite_depth_error();
    }
    let mut resources = match collect_propfind_resources(dav_fs, &path, &relative, depth).await {
        Ok(resources) => resources,
        Err(err) => return fs_error_response(err),
    };

    let mut multistatus = dav_element("multistatus");
    multistatus
        .attributes
        .insert("xmlns:D".to_string(), "DAV:".to_string());

    for resource in resources.drain(..) {
        let response =
            match build_propfind_response(dav_fs, lock_system, prefix, &request_kind, resource)
                .await
            {
                Ok(response) => response,
                Err(resp) => return resp,
            };
        multistatus.children.push(XMLNode::Element(response));
    }

    xml_response(multistatus, multi_status())
}

fn propfind_finite_depth_error() -> HttpResponse {
    let mut error = dav_element("error");
    error
        .attributes
        .insert("xmlns:D".to_string(), "DAV:".to_string());
    error
        .children
        .push(XMLNode::Element(dav_element("propfind-finite-depth")));
    xml_response(error, StatusCode::FORBIDDEN)
}

pub(crate) async fn handle_proppatch(
    req: &HttpRequest,
    dav_fs: &fs::AsterDavFs,
    lock_system: &dyn DavLockSystem,
    prefix: &str,
    body: &[u8],
) -> HttpResponse {
    let (path, _) = match request_path(req, prefix) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let connection = req.connection_info();
    let request_scheme = connection.scheme().to_string();
    let request_host = connection.host().to_string();
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

    let root = Element::parse(Cursor::new(body))
        .map_err(|_| HttpResponse::BadRequest().body("Invalid XML body"))?;
    if root.name != "propfind" {
        return Err(HttpResponse::BadRequest().body("Invalid PROPFIND body"));
    }

    let mut kind = None;
    let mut include = Vec::new();

    for child in child_elements(&root) {
        match child.name.as_str() {
            "propname" => {
                if kind.is_some() {
                    return Err(HttpResponse::BadRequest().body("Invalid PROPFIND body"));
                }
                kind = Some(PropfindKind::PropName);
            }
            "allprop" => {
                if kind.is_some() {
                    return Err(HttpResponse::BadRequest().body("Invalid PROPFIND body"));
                }
                kind = Some(PropfindKind::AllProp {
                    include: Vec::new(),
                });
            }
            "include" => {
                if !matches!(kind.as_ref(), Some(PropfindKind::AllProp { .. })) {
                    return Err(HttpResponse::BadRequest().body("Invalid PROPFIND body"));
                }
                include.extend(child_elements(child).map(RequestedProp::from));
            }
            "prop" => {
                if kind.is_some() {
                    return Err(HttpResponse::BadRequest().body("Invalid PROPFIND body"));
                }
                let props = child_elements(child).map(RequestedProp::from).collect();
                kind = Some(PropfindKind::Prop(props));
            }
            _ => return Err(HttpResponse::BadRequest().body("Invalid PROPFIND body")),
        }
    }

    match kind {
        Some(PropfindKind::AllProp { .. }) => Ok(PropfindKind::AllProp { include }),
        Some(kind) => {
            if include.is_empty() {
                Ok(kind)
            } else {
                Err(HttpResponse::BadRequest().body("Invalid PROPFIND body"))
            }
        }
        None => Ok(PropfindKind::AllProp {
            include: Vec::new(),
        }),
    }
}

fn parse_proppatch_request(body: &[u8]) -> Result<Vec<(bool, DavProp)>, HttpResponse> {
    let root = Element::parse(Cursor::new(body))
        .map_err(|_| HttpResponse::BadRequest().body("Invalid XML body"))?;
    if root.name != "propertyupdate" {
        return Err(HttpResponse::BadRequest().body("Invalid PROPPATCH body"));
    }

    let root_lang = xml_lang_value(&root).map(str::to_string);
    let mut patches = Vec::new();
    for action in child_elements(&root) {
        let action_lang = xml_lang_value(action).or(root_lang.as_deref());
        let set = match action.name.as_str() {
            "set" => true,
            "remove" => false,
            _ => return Err(HttpResponse::BadRequest().body("Invalid PROPPATCH body")),
        };
        let mut action_children = child_elements(action);
        let Some(prop_container) = action_children.next() else {
            return Err(HttpResponse::BadRequest().body("Invalid PROPPATCH body"));
        };
        if prop_container.name != "prop" || action_children.next().is_some() {
            return Err(HttpResponse::BadRequest().body("Invalid PROPPATCH body"));
        }
        let prop_container_lang = xml_lang_value(prop_container).or(action_lang);
        for prop in child_elements(prop_container) {
            let inherited_lang = xml_lang_value(prop).or(prop_container_lang);
            patches.push((set, prop_from_xml(prop, inherited_lang)));
        }
    }
    if patches.is_empty() {
        return Err(HttpResponse::BadRequest().body("Invalid PROPPATCH body"));
    }
    Ok(patches)
}

async fn collect_propfind_resources(
    dav_fs: &fs::AsterDavFs,
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
    dav_fs: &fs::AsterDavFs,
    lock_system: &dyn DavLockSystem,
    prefix: &str,
    request_kind: &PropfindKind,
    resource: PropfindResource,
) -> Result<Element, HttpResponse> {
    let mut response = dav_element("response");
    response.children.push(XMLNode::Element(text_element(
        "D:href",
        &href_for_relative(prefix, &resource.relative),
    )));

    let propstats = match request_kind {
        PropfindKind::AllProp { include } => {
            all_propstat_elements(dav_fs, lock_system, prefix, &resource, include).await?
        }
        PropfindKind::PropName => {
            vec![(
                StatusCode::OK,
                prop_name_elements(dav_fs, lock_system, prefix, &resource).await?,
            )]
        }
        PropfindKind::Prop(requested) => {
            requested_prop_elements(dav_fs, lock_system, prefix, &resource, requested).await?
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
    dav_fs: &fs::AsterDavFs,
    lock_system: &dyn DavLockSystem,
    prefix: &str,
    resource: &PropfindResource,
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
    let custom_props = if is_root_resource(resource) {
        Vec::new()
    } else {
        dav_fs
            .get_props(&resource.path, true)
            .await
            .map_err(fs_error_response)?
    };
    let mut elements = Vec::new();
    for requested in props.drain(..) {
        if let Some(element) =
            standard_prop_element(lock_system, prefix, resource, &requested).await?
        {
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
    dav_fs: &fs::AsterDavFs,
    lock_system: &dyn DavLockSystem,
    prefix: &str,
    resource: &PropfindResource,
    include: &[RequestedProp],
) -> Result<Vec<(StatusCode, Vec<Element>)>, HttpResponse> {
    let (all_props, all_prop_keys) =
        all_prop_elements(dav_fs, lock_system, prefix, resource).await?;
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
    let requested =
        requested_prop_elements(dav_fs, lock_system, prefix, resource, &include).await?;
    for (status, props) in requested {
        if !props.is_empty() {
            result.push((status, props));
        }
    }
    Ok(result)
}

async fn prop_name_elements(
    dav_fs: &fs::AsterDavFs,
    lock_system: &dyn DavLockSystem,
    prefix: &str,
    resource: &PropfindResource,
) -> Result<Vec<Element>, HttpResponse> {
    let mut elements = Vec::new();
    for name in standard_prop_name_list(resource) {
        let requested = RequestedProp {
            name: name.to_string(),
            namespace: Some("DAV:".to_string()),
            prefix: Some("D".to_string()),
        };
        if standard_prop_element(lock_system, prefix, resource, &requested)
            .await?
            .is_some()
        {
            elements.push(requested.empty_element());
        }
    }
    let custom_props = if is_root_resource(resource) {
        Vec::new()
    } else {
        dav_fs
            .get_props(&resource.path, false)
            .await
            .map_err(fs_error_response)?
    };
    for prop in custom_props {
        elements.push(prop_element(
            &prop,
            Some(&RequestedProp {
                name: prop.name.clone(),
                namespace: prop.namespace.clone(),
                prefix: prop.prefix.clone(),
            }),
        ));
    }
    Ok(elements)
}

async fn requested_prop_elements(
    dav_fs: &fs::AsterDavFs,
    lock_system: &dyn DavLockSystem,
    prefix: &str,
    resource: &PropfindResource,
    requested: &[RequestedProp],
) -> Result<Vec<(StatusCode, Vec<Element>)>, HttpResponse> {
    let custom_props = if is_root_resource(resource) {
        Vec::new()
    } else {
        dav_fs
            .get_props(&resource.path, true)
            .await
            .map_err(fs_error_response)?
    };
    let mut ok = Vec::new();
    let mut missing = Vec::new();

    for prop in requested {
        if prop.is_system_namespace() {
            missing.push(prop.empty_element());
            continue;
        }

        if let Some(element) = standard_prop_element(lock_system, prefix, resource, prop).await? {
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

async fn standard_prop_element(
    lock_system: &dyn DavLockSystem,
    prefix: &str,
    resource: &PropfindResource,
    requested: &RequestedProp,
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
        "lockdiscovery" => {
            let locks = lock_system.discover(&resource.path).await;
            Ok(Some(lockdiscovery_element(&locks, prefix)))
        }
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

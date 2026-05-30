//! WebDAV PROPFIND / PROPPATCH handlers.

use std::collections::BTreeMap;
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
use crate::webdav::{
    child_elements, child_relative_path, dav_element, display_name, ensure_unlocked,
    format_creation_date, format_http_date, fs, fs_error_response, href_for_dav_path,
    href_for_relative, multi_status, parse_propfind_depth, request_path, status_element,
    text_element, xml_response,
};

struct RequestedProp {
    name: String,
    namespace: Option<String>,
    prefix: Option<String>,
}

enum PropfindKind {
    AllProp,
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
            && namespace != "DAV:"
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
    let depth = match parse_propfind_depth(req.headers()) {
        Ok(depth) => depth,
        Err(resp) => return resp,
    };
    let request_kind = match parse_propfind_request(body) {
        Ok(kind) => kind,
        Err(resp) => return resp,
    };

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
    if let Err(resp) = ensure_unlocked(lock_system, &path, false, req.headers()).await {
        return resp;
    }

    let root = match Element::parse(Cursor::new(body)) {
        Ok(root) => root,
        Err(_) => return HttpResponse::BadRequest().body("Invalid XML body"),
    };
    if root.name != "propertyupdate" {
        return HttpResponse::BadRequest().body("Invalid PROPPATCH body");
    }

    let mut patches = Vec::new();
    for action in child_elements(&root) {
        let set = match action.name.as_str() {
            "set" => true,
            "remove" => false,
            _ => continue,
        };
        for prop_container in child_elements(action).filter(|elem| elem.name == "prop") {
            for prop in child_elements(prop_container) {
                patches.push((set, prop_from_xml(prop)));
            }
        }
    }

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
        return Ok(PropfindKind::AllProp);
    }

    let root = Element::parse(Cursor::new(body))
        .map_err(|_| HttpResponse::BadRequest().body("Invalid XML body"))?;
    if root.name != "propfind" {
        return Err(HttpResponse::BadRequest().body("Invalid PROPFIND body"));
    }

    for child in child_elements(&root) {
        match child.name.as_str() {
            "propname" => return Ok(PropfindKind::PropName),
            "allprop" => return Ok(PropfindKind::AllProp),
            "prop" => {
                let props = child_elements(child).map(RequestedProp::from).collect();
                return Ok(PropfindKind::Prop(props));
            }
            _ => {}
        }
    }

    Ok(PropfindKind::AllProp)
}

async fn collect_propfind_resources(
    dav_fs: &fs::AsterDavFs,
    path: &DavPath,
    relative: &str,
    depth: u8,
) -> Result<Vec<PropfindResource>, FsError> {
    let root_meta = dav_fs.metadata(path).await?;
    let root_is_dir = root_meta.is_dir();
    let mut resources = vec![PropfindResource {
        path: path.clone(),
        relative: relative.to_string(),
        meta: root_meta,
    }];

    if depth == 1 && root_is_dir {
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
        PropfindKind::AllProp => vec![(
            StatusCode::OK,
            all_prop_elements(dav_fs, lock_system, &resource).await?,
        )],
        PropfindKind::PropName => {
            vec![(
                StatusCode::OK,
                prop_name_elements(dav_fs, lock_system, &resource).await?,
            )]
        }
        PropfindKind::Prop(requested) => {
            requested_prop_elements(dav_fs, lock_system, &resource, requested).await?
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
    resource: &PropfindResource,
) -> Result<Vec<Element>, HttpResponse> {
    let mut props = standard_prop_name_list()
        .into_iter()
        .map(|prop| RequestedProp {
            name: prop.to_string(),
            namespace: Some("DAV:".to_string()),
            prefix: Some("D".to_string()),
        })
        .collect::<Vec<_>>();
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
        if let Some(element) = standard_prop_element(lock_system, resource, &requested).await? {
            elements.push(element);
        }
    }
    for prop in custom_props {
        elements.push(prop_element(&prop, None));
    }
    Ok(elements)
}

async fn prop_name_elements(
    dav_fs: &fs::AsterDavFs,
    lock_system: &dyn DavLockSystem,
    resource: &PropfindResource,
) -> Result<Vec<Element>, HttpResponse> {
    let mut elements = Vec::new();
    for name in standard_prop_name_list() {
        let requested = RequestedProp {
            name: name.to_string(),
            namespace: Some("DAV:".to_string()),
            prefix: Some("D".to_string()),
        };
        if standard_prop_element(lock_system, resource, &requested)
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

        if let Some(element) = standard_prop_element(lock_system, resource, prop).await? {
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
            Ok(Some(lockdiscovery_element(&locks)))
        }
        _ => Ok(None),
    }
}

fn prop_from_xml(prop: &Element) -> DavProp {
    DavProp {
        name: prop.name.clone(),
        prefix: prop.prefix.clone(),
        namespace: prop.namespace.clone(),
        xml: prop.get_text().map(|text| text.into_owned().into_bytes()),
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
        && namespace != "DAV:"
    {
        element
            .attributes
            .insert(format!("xmlns:{prefix}"), namespace.to_string());
    }
    if let Some(xml) = &prop.xml
        && !xml.is_empty()
    {
        element
            .children
            .push(XMLNode::Text(String::from_utf8_lossy(xml).into_owned()));
    }
    element
}

fn is_root_resource(resource: &PropfindResource) -> bool {
    resource.relative == "/"
}

fn standard_prop_name_list() -> [&'static str; 8] {
    [
        "displayname",
        "resourcetype",
        "getcontentlength",
        "getlastmodified",
        "creationdate",
        "getetag",
        "lockdiscovery",
        "supportedlock",
    ]
}

fn default_prefix(namespace: Option<&str>) -> &str {
    match namespace {
        Some("DAV:") => "D",
        Some(_) => "A",
        None => "",
    }
}

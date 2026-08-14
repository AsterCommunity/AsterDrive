//! AsterDrive RFC 3253 core resource adapter.

use actix_web::http::StatusCode;
use actix_web::{HttpRequest, HttpResponse};
use aster_drive_model::entities::{file_revision, file_revision_property};
use aster_forge_utils::http_range::HttpByteRange;
use aster_forge_utils::http_validators;
use aster_forge_webdav::{
    DavBackendError, DavBackendErrorKind, DavCancellationToken, DavCapabilitySnapshot,
    DavDownloadOpenError, DavDownloadPlanError, DavDownloadSource, DavExpandPropertyProvider,
    DavExpandPropertyValue, DavIfResourceState, DavIfStateResolver, DavLiveProperty, DavLockSystem,
    DavMetaData, DavMethod, DavOpenedDownload, DavPath, DavReportErrorResponsePolicy,
    DavReportLimits, DavReportPlanError, DavReportRequest, DavRequestHead, DavRequestedProperty,
    DavResponse, DavResponseBody, DavVersionControlPlan, DavVersionControlPort,
    DavVersionControlResult, DavVersionProperty, DavVersionReportItem, DavVersioningPrecondition,
    DavVersioningState, DavXmlElement, DavXmlNode, dav_element, dav_text_element,
    execute_expand_property, execute_version_control, open_download,
    plan_download_response_with_multi_range, plan_report_request_with_limits,
    plan_version_control_request, report_plan_error_response, version_control_plan_error_response,
    version_control_response, version_tree_response, versioning_precondition_response,
};
use async_trait::async_trait;
use std::collections::HashMap;
use std::time::Duration;

use crate::webdav::{
    backend::{self, AsterDavFs, AsterDavMeta, AuthorizedDeltavRevision},
    handlers::transfer::MULTI_RANGE_POLICY,
    responses,
};

pub(crate) const DELTAV_NAMESPACE_PATH: &str = "/.asterdrive-deltav";
const VERSION_RESOURCE_PREFIX: &str = "/.asterdrive-deltav/versions/";
const REPORT_LIMITS: DavReportLimits = DavReportLimits {
    maximum_input_bytes: 1024 * 1024,
    maximum_xml_depth: 32,
    maximum_selection_depth: 16,
    maximum_selection_properties: 512,
    maximum_expansion_depth: 16,
    maximum_expanded_resources: 10_000,
    maximum_expanded_properties: 10_000,
    multistatus: aster_forge_webdav::DavMultiStatusLimits {
        maximum_output_bytes: 16 * 1024 * 1024,
        maximum_items: 10_000,
        maximum_properties_per_item: 512,
        chunk_bytes: 16 * 1024,
    },
};
const REPORT_MAXIMUM_DURATION: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ReservedDeltavPath {
    Ordinary,
    Reserved,
    Version(String),
}

pub(crate) fn classify_reserved_path(path: &DavPath) -> ReservedDeltavPath {
    let value = path.as_str().trim_end_matches('/');
    if let Some(public_id) = value.strip_prefix(VERSION_RESOURCE_PREFIX)
        && !public_id.is_empty()
        && !public_id.contains('/')
        && uuid::Uuid::parse_str(public_id).is_ok()
    {
        return ReservedDeltavPath::Version(public_id.to_owned());
    }
    if value == DELTAV_NAMESPACE_PATH || value.starts_with(&format!("{DELTAV_NAMESPACE_PATH}/")) {
        ReservedDeltavPath::Reserved
    } else {
        ReservedDeltavPath::Ordinary
    }
}

pub(crate) fn version_resource_path(public_id: &str) -> String {
    format!("{VERSION_RESOURCE_PREFIX}{public_id}")
}

fn href_property(local_name: &str, hrefs: impl IntoIterator<Item = String>) -> DavXmlElement {
    let mut property = dav_element(local_name);
    property.children.extend(
        hrefs
            .into_iter()
            .map(|href| DavXmlNode::Element(dav_text_element("href", href))),
    );
    property
}

fn auto_version_property() -> DavXmlElement {
    let mut property = dav_element("auto-version");
    property
        .children
        .push(DavXmlNode::Element(dav_element("checkout-checkin")));
    property
}

pub(crate) fn live_extension_values(
    revision: &file_revision::Model,
    controlled: bool,
    prefix: &str,
) -> Vec<(DavLiveProperty, DavXmlElement)> {
    let mut values = vec![
        (
            DavLiveProperty::Comment,
            dav_text_element("comment", revision.comment.as_deref().unwrap_or("")),
        ),
        (
            DavLiveProperty::CreatorDisplayName,
            dav_text_element(
                "creator-displayname",
                revision.creator_display_name.as_deref().unwrap_or(""),
            ),
        ),
    ];
    if controlled {
        values.push((
            DavLiveProperty::CheckedIn,
            href_property(
                "checked-in",
                [crate::webdav::href_for_relative(
                    prefix,
                    &version_resource_path(&revision.public_id),
                )],
            ),
        ));
        values.push((DavLiveProperty::AutoVersion, auto_version_property()));
    }
    values
}

fn version_href(prefix: &str, revision: &file_revision::Model) -> String {
    crate::webdav::href_for_relative(prefix, &version_resource_path(&revision.public_id))
}

fn version_property_name(name: &str) -> DavRequestedProperty {
    DavRequestedProperty {
        name: name.to_owned(),
        prefix: Some("D".to_owned()),
        namespace: Some("DAV:".to_owned()),
    }
}

fn version_properties(
    revision: &file_revision::Model,
    revisions: &[file_revision::Model],
    snapshots: &[file_revision_property::Model],
    prefix: &str,
) -> Vec<DavVersionProperty> {
    let by_id = revisions
        .iter()
        .map(|candidate| (candidate.id, candidate))
        .collect::<HashMap<_, _>>();
    let predecessors = revision
        .predecessor_revision_id
        .and_then(|id| {
            by_id
                .get(&id)
                .map(|candidate| version_href(prefix, candidate))
        })
        .into_iter()
        .collect::<Vec<_>>();
    let successors = revisions
        .iter()
        .filter(|candidate| candidate.predecessor_revision_id == Some(revision.id))
        .map(|candidate| version_href(prefix, candidate))
        .collect::<Vec<_>>();
    let mut values = vec![
        DavVersionProperty::text(
            version_property_name("version-name"),
            revision.sequence.to_string(),
        ),
        DavVersionProperty::text(
            version_property_name("creator-displayname"),
            revision.creator_display_name.as_deref().unwrap_or(""),
        ),
        DavVersionProperty::text(
            version_property_name("comment"),
            revision.comment.as_deref().unwrap_or(""),
        ),
        DavVersionProperty::hrefs(version_property_name("predecessor-set"), predecessors),
        DavVersionProperty::hrefs(version_property_name("successor-set"), successors),
        DavVersionProperty::hrefs(version_property_name("checkout-set"), Vec::new()),
        DavVersionProperty::text(version_property_name("getetag"), revision.etag.clone()),
        DavVersionProperty::text(
            version_property_name("getcontentlength"),
            revision.logical_size.to_string(),
        ),
        DavVersionProperty::text(
            version_property_name("getcontenttype"),
            revision
                .mime_type
                .as_deref()
                .unwrap_or("application/octet-stream"),
        ),
        DavVersionProperty::text(
            version_property_name("getlastmodified"),
            http_validators::try_format_http_date(revision.created_at.into()).unwrap_or_default(),
        ),
    ];
    values.extend(snapshots.iter().filter_map(|property| {
        let element = property
            .xml_value
            .as_deref()
            .and_then(|xml| DavXmlElement::parse(xml.as_bytes()).ok())?;
        Some(DavVersionProperty::value(
            DavRequestedProperty {
                name: property.name.clone(),
                prefix: None,
                namespace: (!property.namespace.is_empty()).then(|| property.namespace.clone()),
            },
            element,
        ))
    }));
    values
}

pub(crate) async fn live_extension_values_for_path(
    filesystem: &AsterDavFs,
    path: &DavPath,
    prefix: &str,
) -> Result<Vec<(DavLiveProperty, DavXmlElement)>, DavBackendError> {
    let target = filesystem.deltav_history_target(path).await?;
    if let Some(revision) = target.selected_revision {
        let revisions = filesystem
            .deltav_revisions(
                &target.history,
                REPORT_LIMITS.multistatus.maximum_items as u64,
            )
            .await?;
        let ids = revisions
            .iter()
            .map(|revision| revision.id)
            .collect::<Vec<_>>();
        let snapshots = filesystem.deltav_revision_properties(&ids).await?;
        return Ok(version_properties(
            &revision,
            &revisions,
            snapshots
                .get(&revision.id)
                .map(Vec::as_slice)
                .unwrap_or(&[]),
            prefix,
        )
        .into_iter()
        .filter_map(|property| {
            let live = match property.property.name.as_str() {
                "comment" => DavLiveProperty::Comment,
                "creator-displayname" => DavLiveProperty::CreatorDisplayName,
                "predecessor-set" => DavLiveProperty::PredecessorSet,
                "successor-set" => DavLiveProperty::SuccessorSet,
                "checkout-set" => DavLiveProperty::CheckoutSet,
                "version-name" => DavLiveProperty::VersionName,
                _ => return None,
            };
            match property.result {
                aster_forge_webdav::DavVersionPropertyResult::Value(value) => Some((live, value)),
                _ => None,
            }
        })
        .collect());
    }
    let revision = filesystem.deltav_current_revision(target.file.id).await?;
    Ok(live_extension_values(
        &revision,
        target.history.deltav_controlled_at.is_some(),
        prefix,
    ))
}

pub(crate) fn immutable_method_rejection(
    method: DavMethod,
    snapshot: &DavCapabilitySnapshot,
) -> Option<HttpResponse> {
    if snapshot.declaration().versioning.state != DavVersioningState::Version {
        return None;
    }
    let precondition = match method {
        DavMethod::Put | DavMethod::Proppatch | DavMethod::Copy => {
            DavVersioningPrecondition::CannotModifyVersion
        }
        DavMethod::Move => DavVersioningPrecondition::CannotRenameVersion,
        DavMethod::Delete => DavVersioningPrecondition::NoVersionDelete,
        _ => return None,
    };
    Some(forge_response(versioning_precondition_response(
        snapshot,
        precondition,
    )))
}

struct DriveVersionControlPort<'a> {
    filesystem: &'a AsterDavFs,
    path: &'a DavPath,
}

#[async_trait]
impl DavVersionControlPort for DriveVersionControlPort<'_> {
    async fn version_control(
        &self,
        _plan: DavVersionControlPlan,
    ) -> Result<DavVersionControlResult, DavBackendError> {
        self.filesystem.activate_deltav(self.path).await?;
        Ok(DavVersionControlResult {
            response_extensions: Vec::new(),
        })
    }
}

pub(crate) async fn handle_version_control(
    request_head: &DavRequestHead,
    filesystem: &AsterDavFs,
    lock_system: &dyn DavLockSystem,
    prefix: &str,
    body: &[u8],
    snapshot: &DavCapabilitySnapshot,
) -> HttpResponse {
    let path = &request_head.target;
    if let Err(response) = aster_forge_webdav::actix::enforce_if_header_with_backends(
        request_head.if_header.as_ref(),
        filesystem,
        lock_system,
        path,
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
        path,
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
    let plan = match plan_version_control_request(snapshot, body) {
        Ok(plan) => plan,
        Err(error) => return forge_response(version_control_plan_error_response(snapshot, error)),
    };
    let port = DriveVersionControlPort { filesystem, path };
    match execute_version_control(&port, plan).await {
        Ok(result) => forge_response(version_control_response(result)),
        Err(error) => aster_forge_webdav::actix::into_response(
            aster_forge_webdav::backend_error_response(&error),
        ),
    }
}

struct ReportResponsePolicy;

impl DavReportErrorResponsePolicy for ReportResponsePolicy {
    fn unknown_type(&self, namespace: Option<&str>, name: &str) -> DavResponse {
        let mut response = DavResponse::bytes(
            http::StatusCode::UNPROCESSABLE_ENTITY,
            format!("unknown REPORT {namespace:?}:{name}"),
        );
        response.headers.insert(
            http::header::CACHE_CONTROL,
            http::HeaderValue::from_static("no-store"),
        );
        response
    }

    fn not_available(&self, report: aster_forge_webdav::DavReportType) -> DavResponse {
        let mut response = DavResponse::bytes(
            http::StatusCode::CONFLICT,
            format!("REPORT {} is not available", report.local_name()),
        );
        response.headers.insert(
            http::header::CACHE_CONTROL,
            http::HeaderValue::from_static("no-store"),
        );
        response
    }
}

fn report_error_response(error: &DavReportPlanError) -> HttpResponse {
    match report_plan_error_response(error, &ReportResponsePolicy) {
        Ok(response) => aster_forge_webdav::actix::into_response(response),
        Err(error) => {
            tracing::warn!(error = %error, "failed to build REPORT planning response");
            responses::empty(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

struct HistoryExpandProvider {
    values: HashMap<String, Vec<DavVersionProperty>>,
}

#[async_trait]
impl DavExpandPropertyProvider for HistoryExpandProvider {
    async fn property(
        &self,
        href: &str,
        property: &DavRequestedProperty,
    ) -> Result<Option<DavExpandPropertyValue>, DavBackendError> {
        let Some(values) = self.values.get(href) else {
            return Err(DavBackendError::new(DavBackendErrorKind::NotFound));
        };
        let Some(value) = values.iter().find(|candidate| {
            candidate.property.name == property.name
                && candidate.property.namespace == property.namespace
        }) else {
            return Ok(None);
        };
        match &value.result {
            aster_forge_webdav::DavVersionPropertyResult::Value(element) => {
                if property.name.ends_with("-set") || property.name == "checked-in" {
                    let hrefs = element
                        .children
                        .iter()
                        .filter_map(|child| match child {
                            DavXmlNode::Element(element) if element.name == "href" => Some(
                                element
                                    .children
                                    .iter()
                                    .filter_map(|child| match child {
                                        DavXmlNode::Text(text) | DavXmlNode::CData(text) => {
                                            Some(text.as_str())
                                        }
                                        _ => None,
                                    })
                                    .collect::<String>(),
                            ),
                            _ => None,
                        })
                        .collect::<Vec<_>>();
                    Ok(Some(DavExpandPropertyValue::Hrefs(hrefs)))
                } else {
                    Ok(Some(DavExpandPropertyValue::Element(element.clone())))
                }
            }
            aster_forge_webdav::DavVersionPropertyResult::Missing => Ok(None),
            aster_forge_webdav::DavVersionPropertyResult::BackendError(kind) => {
                Err(DavBackendError::new(*kind))
            }
        }
    }
}

fn report_items(
    revisions: &[file_revision::Model],
    properties: &HashMap<i64, Vec<file_revision_property::Model>>,
    prefix: &str,
) -> Vec<DavVersionReportItem> {
    revisions
        .iter()
        .map(|revision| DavVersionReportItem {
            href: version_href(prefix, revision),
            properties: version_properties(
                revision,
                revisions,
                properties
                    .get(&revision.id)
                    .map(Vec::as_slice)
                    .unwrap_or(&[]),
                prefix,
            ),
        })
        .collect()
}

async fn load_history_report(
    filesystem: &AsterDavFs,
    path: &DavPath,
    prefix: &str,
) -> Result<(Vec<DavVersionReportItem>, String, Vec<DavVersionProperty>), DavBackendError> {
    let target = filesystem.deltav_history_target(path).await?;
    let revisions = filesystem
        .deltav_revisions(
            &target.history,
            REPORT_LIMITS.multistatus.maximum_items as u64 + 1,
        )
        .await?;
    if revisions.len() > REPORT_LIMITS.multistatus.maximum_items {
        return Err(DavBackendError::new(DavBackendErrorKind::PayloadTooLarge));
    }
    let ids = revisions
        .iter()
        .map(|revision| revision.id)
        .collect::<Vec<_>>();
    let properties = filesystem.deltav_revision_properties(&ids).await?;
    let items = report_items(&revisions, &properties, prefix);
    let root_href = crate::webdav::href_for_dav_path(prefix, path);
    let mut root_values = match classify_reserved_path(path) {
        ReservedDeltavPath::Version(public_id) => items
            .iter()
            .find(|item| item.href.ends_with(&public_id))
            .map(|item| item.properties.clone())
            .unwrap_or_default(),
        ReservedDeltavPath::Ordinary | ReservedDeltavPath::Reserved => items
            .last()
            .map(|item| item.properties.clone())
            .unwrap_or_default(),
    };
    if matches!(classify_reserved_path(path), ReservedDeltavPath::Ordinary)
        && let Some(current) = items.last()
    {
        let mut checked_in = dav_element("checked-in");
        checked_in
            .children
            .push(DavXmlNode::Element(dav_text_element(
                "href",
                current.href.clone(),
            )));
        root_values.push(DavVersionProperty::value(
            version_property_name("checked-in"),
            checked_in,
        ));
        root_values.push(DavVersionProperty::value(
            version_property_name("auto-version"),
            auto_version_property(),
        ));
    }
    Ok((items, root_href, root_values))
}

pub(crate) async fn handle_report(
    request_head: &DavRequestHead,
    filesystem: &AsterDavFs,
    lock_system: &dyn DavLockSystem,
    prefix: &str,
    body: &[u8],
    snapshot: &DavCapabilitySnapshot,
) -> HttpResponse {
    if let Err(response) = aster_forge_webdav::actix::enforce_if_header_with_backends(
        request_head.if_header.as_ref(),
        filesystem,
        lock_system,
        &request_head.target,
        prefix,
        &request_head.origin.scheme,
        &request_head.origin.host,
    )
    .await
    {
        return response;
    }
    let plan =
        match plan_report_request_with_limits(snapshot, body, request_head.depth, REPORT_LIMITS) {
            Ok(plan) => plan,
            Err(error) => return report_error_response(&error),
        };
    let (items, root_href, root_values) =
        match load_history_report(filesystem, &request_head.target, prefix).await {
            Ok(value) => value,
            Err(error) => {
                return aster_forge_webdav::actix::into_response(
                    aster_forge_webdav::backend_error_response(&error),
                );
            }
        };
    match plan {
        DavReportRequest::VersionTree(request) => match version_tree_response(&request, items) {
            Ok(response) => aster_forge_webdav::actix::into_response(response),
            Err(error) => {
                tracing::warn!(error = %error, "DeltaV version-tree response exceeded limits");
                responses::empty(StatusCode::INSUFFICIENT_STORAGE)
            }
        },
        DavReportRequest::ExpandProperty(request) => {
            let values = items
                .iter()
                .map(|item| (item.href.clone(), item.properties.clone()))
                .chain(std::iter::once((root_href.clone(), root_values)))
                .collect::<HashMap<_, _>>();
            let provider = HistoryExpandProvider { values };
            let cancellation = DavCancellationToken::new();
            match tokio::time::timeout(
                REPORT_MAXIMUM_DURATION,
                execute_expand_property(
                    &provider,
                    &root_href,
                    &request,
                    REPORT_LIMITS,
                    &cancellation,
                ),
            )
            .await
            {
                Ok(Ok(response)) => aster_forge_webdav::actix::into_response(response),
                Ok(Err(error)) => {
                    match aster_forge_webdav::expand_property_error_response(&error) {
                        Ok(response) => aster_forge_webdav::actix::into_response(response),
                        Err(_) => responses::empty(StatusCode::INTERNAL_SERVER_ERROR),
                    }
                }
                Err(_) => {
                    cancellation.cancel();
                    responses::empty(StatusCode::SERVICE_UNAVAILABLE)
                }
            }
        }
        DavReportRequest::Other { report, .. } => {
            aster_forge_webdav::actix::into_response(ReportResponsePolicy.not_available(report))
        }
    }
}

struct ImmutableIfResolver<'a> {
    target_path: &'a DavPath,
    etag: &'a str,
}

#[async_trait]
impl DavIfStateResolver for ImmutableIfResolver<'_> {
    async fn resolve_if_state(
        &self,
        path: &DavPath,
    ) -> Result<DavIfResourceState, DavBackendError> {
        Ok(if path == self.target_path {
            DavIfResourceState {
                etag: Some(self.etag.to_owned()),
                lock_tokens: Vec::new(),
            }
        } else {
            DavIfResourceState::default()
        })
    }
}

async fn enforce_immutable_if(
    request_head: &DavRequestHead,
    etag: &str,
    prefix: &str,
) -> Result<(), HttpResponse> {
    let resolver = ImmutableIfResolver {
        target_path: &request_head.target,
        etag,
    };
    match aster_forge_webdav::enforce_if_header(
        request_head.if_header.as_ref(),
        &resolver,
        &request_head.target,
        prefix,
        &request_head.origin.scheme,
        &request_head.origin.host,
    )
    .await
    {
        Ok(()) => Ok(()),
        Err(aster_forge_webdav::DavIfEvaluationError::Protocol(error)) => {
            Err(aster_forge_webdav::actix::protocol_error_response(error))
        }
        Err(aster_forge_webdav::DavIfEvaluationError::Backend(error)) => {
            Err(aster_forge_webdav::actix::into_response(
                aster_forge_webdav::backend_error_response(&error),
            ))
        }
    }
}

struct ImmutableRevisionSource<'a> {
    filesystem: &'a AsterDavFs,
    target: &'a AuthorizedDeltavRevision,
}

impl ImmutableRevisionSource<'_> {
    async fn open(
        &self,
        range: Option<HttpByteRange>,
    ) -> Result<DavOpenedDownload, DavBackendError> {
        crate::webdav::observation::add_backend_open();
        let expected_length = range.map_or_else(
            || u64::try_from(self.target.revision.logical_size).unwrap_or_default(),
            |range| range.length(),
        );
        let (offset, length) = range.map_or((None, None), |range| {
            (Some(range.start()), Some(range.length()))
        });
        let reader = self
            .filesystem
            .open_download_stream_for_file(&self.target.file, &self.target.blob, offset, length)
            .await
            .map_err(DavBackendError::from)?;
        Ok(DavOpenedDownload::new(
            backend::exact_length_stream(reader, expected_length),
            expected_length,
        ))
    }
}

impl DavDownloadSource for ImmutableRevisionSource<'_> {
    type Metadata = AsterDavMeta;

    async fn metadata<'a>(&'a self, _path: &'a DavPath) -> Result<Self::Metadata, DavBackendError> {
        Ok(AsterDavMeta::from_revision(
            &self.target.file,
            &self.target.revision,
        ))
    }

    async fn open_full<'a>(
        &'a self,
        _path: &'a DavPath,
    ) -> Result<DavOpenedDownload, DavBackendError> {
        self.open(None).await
    }

    async fn open_range<'a>(
        &'a self,
        _path: &'a DavPath,
        range: HttpByteRange,
    ) -> Result<DavOpenedDownload, DavBackendError> {
        self.open(Some(range)).await
    }
}

pub(crate) async fn handle_version_get_head(
    req: &HttpRequest,
    request_head: &DavRequestHead,
    filesystem: &AsterDavFs,
    prefix: &str,
    head_only: bool,
) -> HttpResponse {
    let ReservedDeltavPath::Version(public_id) = classify_reserved_path(&request_head.target)
    else {
        return responses::empty(StatusCode::NOT_FOUND);
    };
    let target = match filesystem.load_deltav_revision(&public_id).await {
        Ok(target) => target,
        Err(error) => {
            return aster_forge_webdav::actix::into_response(
                aster_forge_webdav::backend_error_response(&error),
            );
        }
    };
    if let Err(response) = enforce_immutable_if(request_head, &target.revision.etag, prefix).await {
        return response;
    }
    let source = ImmutableRevisionSource {
        filesystem,
        target: &target,
    };
    let metadata = match source.metadata(&request_head.target).await {
        Ok(metadata) => metadata,
        Err(error) => {
            return aster_forge_webdav::actix::into_response(
                aster_forge_webdav::backend_error_response(&error),
            );
        }
    };
    let headers = match aster_forge_webdav::actix::converted_headers(req.headers()) {
        Ok(headers) => headers,
        Err(response) => return response,
    };
    let content_type = metadata
        .content_type()
        .unwrap_or("application/octet-stream");
    let last_modified = match metadata.modified() {
        Ok(value) => value,
        Err(error) => return crate::webdav::fs_error_response(error),
    };
    let etag = metadata.etag();
    let plan = match plan_download_response_with_multi_range(
        &headers,
        head_only,
        metadata.len(),
        content_type,
        etag.as_deref(),
        last_modified,
        MULTI_RANGE_POLICY,
    ) {
        Ok(plan) => plan,
        Err(DavDownloadPlanError::Protocol(error)) => {
            return aster_forge_webdav::actix::protocol_error_response(error);
        }
        Err(DavDownloadPlanError::InvalidRepresentation) => {
            return responses::empty(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    let mut response = plan.response;
    response.headers.insert(
        http::header::CACHE_CONTROL,
        http::HeaderValue::from_static("private, max-age=31536000, immutable"),
    );
    match open_download(&source, &request_head.target, plan.body).await {
        Ok(Some(opened)) => response.body = DavResponseBody::Stream(opened.stream),
        Ok(None) if head_only => {
            // Actix derives a zero Content-Length from an empty body at the real HTTP
            // boundary. A metadata-only stream preserves the representation length
            // selected by the Forge HEAD plan while still opening no storage stream.
            response.body = DavResponseBody::Stream(Box::pin(futures::stream::empty()));
        }
        Ok(None) => {}
        Err(DavDownloadOpenError::Backend(error)) => {
            return aster_forge_webdav::actix::into_response(
                aster_forge_webdav::backend_error_response(&error),
            );
        }
        Err(DavDownloadOpenError::LengthMismatch { planned, opened }) => {
            tracing::warn!(planned, opened, "immutable revision download length drift");
            return responses::empty(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }
    aster_forge_webdav::actix::into_response(response)
}

fn forge_response<E: std::fmt::Debug>(
    response: Result<aster_forge_webdav::DavResponse, E>,
) -> HttpResponse {
    match response {
        Ok(response) => aster_forge_webdav::actix::into_response(response),
        Err(error) => {
            tracing::warn!(error = ?error, "failed to build DeltaV response");
            responses::empty(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_namespace_only_accepts_opaque_version_urls() {
        let id = uuid::Uuid::new_v4().hyphenated().to_string();
        assert_eq!(
            classify_reserved_path(&DavPath::new(&version_resource_path(&id)).unwrap()),
            ReservedDeltavPath::Version(id)
        );
        assert_eq!(
            classify_reserved_path(
                &DavPath::new("/.asterdrive-deltav/versions/not-an-id").unwrap()
            ),
            ReservedDeltavPath::Reserved
        );
        assert_eq!(
            classify_reserved_path(&DavPath::new("/ordinary.txt").unwrap()),
            ReservedDeltavPath::Ordinary
        );
    }
}

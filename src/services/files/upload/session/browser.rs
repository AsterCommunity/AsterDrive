//! Product coordination: authorization, bounded execution, durable protocol state and CAS.
use super::{
    kind::resolve_upload_session_kind,
    provider::{decrypt_provider_session, encrypt_provider_session},
    scope::load_upload_session,
};
use crate::{
    db::repository::upload_session_repo,
    errors::{AsterError, Result},
    runtime::{PrimaryAppState, SharedRuntimeState},
    services::workspace::storage::WorkspaceStorageScope,
};
use aster_drive_model::{
    entities::upload_session,
    types::{UploadSessionKind, UploadSessionStatus},
};
use aster_drive_storage::{
    BrowserUploadContext, BrowserUploadObservation, BrowserUploadProtocol, BrowserUploadReceipt,
    BrowserUploadRequest, BrowserUploadTransition,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

pub const RESPONSE_BODY_LIMIT: usize = 64 * 1024;
const PRIVATE_STATE_LIMIT: usize = 32 * 1024;
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

#[derive(Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct BrowserUploadInstruction {
    pub request_id: String,
    pub request: BrowserUploadRequest,
    pub response_body_limit: u32,
    pub timeout_ms: u32,
}

#[derive(Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct BrowserUploadStep {
    /// Version of the finite browser executor, independent from driver protocol versions.
    pub executor_version: u32,
    pub protocol: BrowserUploadProtocol,
    pub confirmed_offset: u64,
    /// Increments only when driver state or confirmed progress advances, not for a fresh request ID.
    pub progress_revision: u64,
    pub provider_completed: bool,
    pub instruction: Option<BrowserUploadInstruction>,
}

#[derive(Clone, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct BrowserUploadReport {
    pub request_id: Option<String>,
    pub observation: Option<BrowserUploadObservation>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct BrowserSession {
    pub protocol: BrowserUploadProtocol,
    private_state: serde_json::Value,
    confirmed_offset: u64,
    #[serde(default)]
    progress_revision: u64,
    provider_completed: bool,
    pending: Option<BrowserUploadInstruction>,
}
impl BrowserSession {
    pub(crate) fn new(protocol: BrowserUploadProtocol) -> Self {
        Self {
            protocol,
            private_state: serde_json::Value::Null,
            confirmed_offset: 0,
            progress_revision: 0,
            provider_completed: false,
            pending: None,
        }
    }
    pub(crate) fn step(&self) -> BrowserUploadStep {
        BrowserUploadStep {
            executor_version: 1,
            protocol: self.protocol.clone(),
            confirmed_offset: self.confirmed_offset,
            progress_revision: self.progress_revision,
            provider_completed: self.provider_completed,
            instruction: self.pending.clone(),
        }
    }
}

fn invalid(message: &str) -> AsterError {
    AsterError::validation_error(message)
}

fn validate_report(report: &BrowserUploadReport) -> Result<()> {
    if report.request_id.is_some() != report.observation.is_some()
        || report.request_id.as_ref().is_some_and(|id| id.len() > 64)
    {
        return Err(invalid(
            "browser upload report must pair request ID and observation",
        ));
    }
    if let Some(value) = &report.observation
        && (value.status > 599
            || (value.status != 0 && value.status < 100)
            || value.body.len() > RESPONSE_BODY_LIMIT
            || value.headers.len() > 32
            || value.headers.iter().any(|(k, v)| {
                k.len() > 128 || v.len() > 8192 || !valid_header_name(k) || v.contains(['\r', '\n'])
            }))
    {
        return Err(invalid("browser upload observation exceeds its contract"));
    }
    Ok(())
}
fn valid_header_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
}
fn forbidden_header(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    matches!(
        name.as_str(),
        "authorization"
            | "proxy-authorization"
            | "cookie"
            | "cookie2"
            | "host"
            | "origin"
            | "referer"
            | "content-length"
            | "connection"
            | "transfer-encoding"
            | "accept-encoding"
            | "x-csrf-token"
            | "set-cookie"
    ) || name.starts_with("sec-")
        || name.starts_with("proxy-")
        || name.starts_with("access-control-")
}
fn validate_transition(value: &BrowserUploadTransition, previous: u64, total: u64) -> Result<()> {
    if total == 0
        || total > MAX_SAFE_INTEGER
        || value.confirmed_offset < previous
        || value.confirmed_offset > total
        || (value.provider_completed && value.confirmed_offset != total)
        || value.provider_completed == value.request.is_some()
    {
        return Err(invalid(
            "driver returned inconsistent browser upload progress",
        ));
    }
    if serde_json::to_vec(&value.private_state)
        .map_err(|_| invalid("invalid browser protocol state"))?
        .len()
        > PRIVATE_STATE_LIMIT
    {
        return Err(invalid("browser protocol state is too large"));
    }
    if let Some(request) = &value.request {
        let url =
            url::Url::parse(&request.url).map_err(|_| invalid("invalid browser upload URL"))?;
        if !matches!(url.scheme(), "https" | "http")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
            || request.url.len() > 8192
        {
            return Err(invalid("unsafe browser upload URL"));
        }
        if request.headers.len() > 32
            || request.headers.iter().any(|(k, v)| {
                !valid_header_name(k)
                    || forbidden_header(k)
                    || v.len() > 8192
                    || v.contains(['\r', '\n'])
            })
            || request.response_headers.len() > 32
            || request
                .response_headers
                .iter()
                .any(|k| !valid_header_name(k) || forbidden_header(k))
        {
            return Err(invalid("unsafe browser upload headers"));
        }
        if let Some(range) = request.body {
            if range.start >= range.end || range.end > total {
                return Err(invalid("invalid browser upload file range"));
            }
            if matches!(
                request.method,
                aster_drive_storage::BrowserUploadMethod::Get
            ) {
                return Err(invalid("GET upload instruction cannot carry file bytes"));
            }
        }
    }
    Ok(())
}

/// Legacy sessions predate snapshots. Only the two historically shipped protocols can be inferred.
fn legacy_protocol(provider: &str) -> Result<BrowserUploadProtocol> {
    let id = match provider {
        "microsoft_graph" => "asterdrive.microsoft_graph.resumable",
        "google_drive" => "asterdrive.google_drive.resumable",
        _ => {
            return Err(invalid(
                "legacy upload protocol is unknown; restart the upload",
            ));
        }
    };
    Ok(BrowserUploadProtocol {
        id: id.into(),
        version: 1,
    })
}

pub(crate) async fn advance_session(
    state: &PrimaryAppState,
    session: &upload_session::Model,
    report: &BrowserUploadReport,
) -> Result<BrowserUploadStep> {
    validate_report(report)?;
    if resolve_upload_session_kind(session)? != UploadSessionKind::ProviderDirectResumable {
        return Err(invalid("upload does not support browser coordination"));
    }
    if session.status != UploadSessionStatus::Uploading {
        return Err(super::super::upload_status_conflict(
            session.status,
            UploadSessionStatus::Uploading,
        ));
    }
    if session.expires_at <= chrono::Utc::now() {
        return Err(AsterError::upload_session_expired("upload session expired"));
    }
    let mut secret = decrypt_provider_session(state, session)?;
    let mut browser = match secret.browser.clone() {
        Some(value) => value,
        None => BrowserSession::new(legacy_protocol(&secret.provider)?),
    };
    let policy = state
        .policy_snapshot()
        .get_policy_or_err(session.policy_id)?;
    let driver = state.driver_registry().get_driver(&policy)?;
    let provider = driver
        .extensions()
        .provider_resumable
        .ok_or_else(|| invalid("provider resumable driver is unavailable"))?;
    let adapter = provider
        .browser_upload()
        .ok_or_else(|| invalid("browser upload driver is unavailable"))?;
    if !adapter.supports_protocol(&browser.protocol) {
        return Err(invalid(
            "stored browser upload protocol is unsupported; restart the upload",
        ));
    }
    let pending = browser.pending.as_ref();
    let observation = report.observation.as_ref().map(|value| {
        let mut value = value.clone();
        // Normalize names and discard anything the issued instruction did not request.
        let allowed = pending.map(|p| &p.request.response_headers);
        value.headers = value
            .headers
            .into_iter()
            .filter_map(|(k, v)| {
                allowed
                    .is_some_and(|names| names.iter().any(|name| name.eq_ignore_ascii_case(&k)))
                    .then(|| (k.to_ascii_lowercase(), v))
            })
            .collect::<BTreeMap<_, _>>();
        value
    });
    // A stale/replayed report is never reinterpreted against a newer request. Reconcile instead.
    let receipt = pending
        .zip(observation.as_ref())
        .filter(|(p, _)| report.request_id.as_deref() == Some(p.request_id.as_str()))
        .map(|(p, o)| BrowserUploadReceipt {
            request: &p.request,
            observation: o,
        });
    let transition = adapter
        .advance(
            BrowserUploadContext {
                protocol: &browser.protocol,
                upload_url: &secret.upload_url,
                storage_path: session
                    .object_temp_key
                    .as_deref()
                    .ok_or_else(|| invalid("upload object key is missing"))?,
                total_size: u64::try_from(session.total_size)
                    .map_err(|_| invalid("invalid upload size"))?,
                fragment_size: u64::try_from(session.chunk_size)
                    .map_err(|_| invalid("invalid fragment size"))?,
            },
            browser.private_state.clone(),
            receipt,
        )
        .await?;
    validate_transition(
        &transition,
        browser.confirmed_offset,
        u64::try_from(session.total_size).map_err(|_| invalid("invalid upload size"))?,
    )?;
    if browser.private_state != transition.private_state
        || browser.confirmed_offset != transition.confirmed_offset
        || browser.provider_completed != transition.provider_completed
    {
        browser.progress_revision = browser
            .progress_revision
            .checked_add(1)
            .filter(|value| *value <= MAX_SAFE_INTEGER)
            .ok_or_else(|| invalid("browser protocol progress revision overflow"))?;
    }
    browser.private_state = transition.private_state;
    browser.confirmed_offset = transition.confirmed_offset;
    browser.provider_completed = transition.provider_completed;
    browser.pending = transition.request.map(|request| BrowserUploadInstruction {
        request_id: aster_forge_utils::id::new_uuid(),
        request,
        response_body_limit: u32::try_from(RESPONSE_BODY_LIMIT).unwrap_or(u32::MAX),
        timeout_ms: 60_000,
    });
    let step = browser.step();
    secret.browser = Some(browser);
    let next = encrypt_provider_session(state, &session.id, &secret)?;
    let previous = session
        .provider_session_ciphertext
        .as_deref()
        .ok_or_else(|| invalid("provider session is missing"))?;
    if !upload_session_repo::compare_exchange_provider_session(
        state.writer_db(),
        &session.id,
        previous,
        &next,
    )
    .await?
    {
        // Includes cancellation/expiry during provider I/O. Never issue the stale instruction.
        return Err(aster_drive_storage::storage_driver_error(
            aster_drive_storage::StorageErrorKind::Transient,
            "upload changed while coordinating; reload its authoritative state",
        )
        .into());
    }
    Ok(step)
}

pub(crate) async fn coordinate_for_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    upload_id: &str,
    report: BrowserUploadReport,
) -> Result<BrowserUploadStep> {
    let session = load_upload_session(state, scope, upload_id).await?;
    advance_session(state, &session, &report).await
}

pub async fn coordinate_browser_upload(
    state: &PrimaryAppState,
    upload_id: &str,
    user_id: i64,
    report: BrowserUploadReport,
) -> Result<BrowserUploadStep> {
    coordinate_for_scope(
        state,
        super::scope::personal_scope(user_id),
        upload_id,
        report,
    )
    .await
}
pub async fn coordinate_browser_upload_for_team(
    state: &PrimaryAppState,
    team_id: i64,
    upload_id: &str,
    user_id: i64,
    report: BrowserUploadReport,
) -> Result<BrowserUploadStep> {
    coordinate_for_scope(
        state,
        super::scope::team_scope(team_id, user_id),
        upload_id,
        report,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use aster_drive_storage::{BrowserUploadMethod, BrowserUploadRange};
    fn transition() -> BrowserUploadTransition {
        BrowserUploadTransition {
            private_state: serde_json::json!({}),
            confirmed_offset: 4,
            provider_completed: false,
            request: Some(BrowserUploadRequest {
                url: "https://upload.invalid/capability".into(),
                method: BrowserUploadMethod::Patch,
                headers: BTreeMap::new(),
                body: Some(BrowserUploadRange { start: 4, end: 8 }),
                response_headers: vec!["x-plugin-offset".into()],
            }),
        }
    }

    #[test]
    fn persisted_snapshots_preserve_opaque_protocol_state_and_decode_old_sessions() {
        let legacy: super::super::provider::ProviderSessionSecret = serde_json::from_value(
            serde_json::json!({"provider":"google_drive","upload_url":"https://upload.invalid"}),
        )
        .unwrap();
        assert!(legacy.browser.is_none());
        let mut session = BrowserSession::new(BrowserUploadProtocol {
            id: "third.party.plugin".into(),
            version: 27,
        });
        session.private_state = serde_json::json!({"opaque":"driver-owned-state","revision":9});
        let restored: BrowserSession =
            serde_json::from_slice(&serde_json::to_vec(&session).unwrap()).unwrap();
        assert_eq!(restored.protocol, session.protocol);
        assert_eq!(restored.private_state, session.private_state);
        assert_eq!(restored.step().executor_version, 1);
    }
    #[test]
    fn instruction_contract_checks_integer_bounds_progress_and_completion() {
        assert!(validate_transition(&transition(), 0, 8).is_ok());
        for mutate in [
            |t: &mut BrowserUploadTransition| t.confirmed_offset = 9,
            |t: &mut BrowserUploadTransition| t.provider_completed = true,
            |t: &mut BrowserUploadTransition| t.request = None,
            |t: &mut BrowserUploadTransition| {
                t.private_state = serde_json::json!("x".repeat(PRIVATE_STATE_LIMIT))
            },
        ] {
            let mut t = transition();
            mutate(&mut t);
            assert!(validate_transition(&t, 0, 8).is_err());
        }
        assert!(validate_transition(&transition(), 5, 8).is_err());
        assert!(validate_transition(&transition(), 0, MAX_SAFE_INTEGER + 1).is_err());
        let mut t = transition();
        t.confirmed_offset = 8;
        t.provider_completed = true;
        t.request = None;
        assert!(validate_transition(&t, 4, 8).is_ok());
    }
    #[test]
    fn execution_contract_rejects_credentials_forbidden_headers_and_invalid_ranges() {
        for url in [
            "javascript:alert(1)",
            "file:///tmp/file",
            "https://user:secret@upload.invalid",
            "https://upload.invalid/#secret",
            "invalid",
        ] {
            let mut t = transition();
            t.request.as_mut().unwrap().url = url.into();
            assert!(validate_transition(&t, 0, 8).is_err());
        }
        for name in [
            "Authorization",
            "COOKIE",
            "X-CSRF-Token",
            "Host",
            "Sec-Fetch-Mode",
            "Content-Length",
            "bad\nheader",
        ] {
            let mut t = transition();
            t.request
                .as_mut()
                .unwrap()
                .headers
                .insert(name.into(), "secret".into());
            assert!(validate_transition(&t, 0, 8).is_err(), "{name}");
        }
        for range in [
            BrowserUploadRange { start: 0, end: 9 },
            BrowserUploadRange { start: 4, end: 4 },
            BrowserUploadRange { start: 5, end: 4 },
        ] {
            let mut t = transition();
            t.request.as_mut().unwrap().body = Some(range);
            assert!(validate_transition(&t, 0, 8).is_err());
        }
    }
    #[test]
    fn observations_are_bounded_and_legacy_inference_is_explicit() {
        assert!(
            validate_report(&BrowserUploadReport {
                request_id: None,
                observation: None
            })
            .is_ok()
        );
        let observation = BrowserUploadObservation {
            status: 308,
            headers: BTreeMap::new(),
            body: String::new(),
            body_truncated: false,
        };
        assert!(
            validate_report(&BrowserUploadReport {
                request_id: None,
                observation: Some(observation.clone())
            })
            .is_err()
        );
        for (status, size) in [(99, 0), (600, 0), (200, RESPONSE_BODY_LIMIT + 1)] {
            let mut o = observation.clone();
            o.status = status;
            o.body = "x".repeat(size);
            assert!(
                validate_report(&BrowserUploadReport {
                    request_id: Some("id".into()),
                    observation: Some(o)
                })
                .is_err()
            );
        }
        assert_eq!(legacy_protocol("google_drive").unwrap().version, 1);
        assert!(legacy_protocol("microsoft_graph").is_ok());
        assert!(legacy_protocol("new_plugin").is_err());
    }
}

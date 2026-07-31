//! WebDAV resource mutation handlers: MKCOL, DELETE, COPY, MOVE.

use actix_web::http::StatusCode;
use actix_web::{HttpRequest, HttpResponse};
use aster_forge_webdav::{
    DavConditionalOutcome, DavConditionalResource, DavCopyMoveMethod, DavDirectoryPageLimits,
    DavMetaData, DavMethod, DavMutationCommand, DavMutationExecutorLimits, DavMutationOperation,
    DavMutationPlanError, DavMutationPort, DavMutationRequest, DavMutationStepError,
    DavMutationStepKind, DavNeverCancelled, DavRequestHead, DavResourceKind, DavResponse,
    DavTraversalLimits, collection_created_response, execute_recursive_mutation,
    mutation_outcome_response, mutation_plan_error_response, plan_copy_move_request,
    validate_collection_create_target, validate_delete_target,
};

use crate::services::files::folder::FolderTreeTraversalLimits;
use crate::webdav::{
    backend, ensure_system_file_name_allowed, fs_error_response, responses, system_file,
};
use aster_forge_webdav::{DavFileSystem, DavLockSystem, FsError};

const MUTATION_DIRECTORY_PAGE_ENTRIES: usize = 256;
const MUTATION_DIRECTORY_MAXIMUM_PAGES: usize = 40;
const MUTATION_DIRECTORY_LIMITS: DavDirectoryPageLimits = DavDirectoryPageLimits {
    maximum_entries: MUTATION_DIRECTORY_PAGE_ENTRIES,
    maximum_pages: MUTATION_DIRECTORY_MAXIMUM_PAGES,
};
const MUTATION_MAXIMUM_VISITED_RESOURCES: usize = 10_000;
const MUTATION_MAXIMUM_QUEUED_WORK_ITEMS: usize = 10_000;
const MUTATION_MAXIMUM_FAILURES: usize = 512;
const MUTATION_MAXIMUM_DEPTH: usize = 128;
pub(crate) const MUTATION_FOLDER_TREE_LIMITS: FolderTreeTraversalLimits =
    FolderTreeTraversalLimits::new(
        MUTATION_MAXIMUM_VISITED_RESOURCES,
        MUTATION_MAXIMUM_QUEUED_WORK_ITEMS,
        MUTATION_MAXIMUM_DEPTH,
    );

struct AsterDavMutationPort<'a> {
    dav_fs: &'a backend::AsterDavFs,
    request_head: &'a DavRequestHead,
    prefix: &'a str,
}

impl DavMutationPort for AsterDavMutationPort<'_> {
    async fn execute(&self, command: DavMutationCommand) -> Result<(), DavMutationStepError> {
        let affected_path = command.affected_path().clone();
        let conditions = backend::DavMutationConditions {
            prefix: self.prefix,
            if_header: self.request_head.if_header.as_ref(),
            request_scheme: &self.request_head.origin.scheme,
            request_host: &self.request_head.origin.host,
        };
        let result = match command.step {
            DavMutationStepKind::CopyFile => {
                let Some(destination) = command.destination.as_ref() else {
                    return Err(DavMutationStepError::backend(affected_path));
                };
                self.dav_fs
                    .copy_file_with_locks(&command.source, destination, conditions)
                    .await
            }
            DavMutationStepKind::MoveFile => {
                let Some(destination) = command.destination.as_ref() else {
                    return Err(DavMutationStepError::backend(affected_path));
                };
                self.dav_fs
                    .move_with_locks(&command.source, destination, conditions)
                    .await
            }
            DavMutationStepKind::PrepareCollection => {
                let Some(destination) = command.destination.as_ref() else {
                    return Err(DavMutationStepError::backend(affected_path));
                };
                self.dav_fs
                    .prepare_collection_with_locks(
                        &command.source,
                        destination,
                        command.operation,
                        conditions,
                    )
                    .await
            }
            DavMutationStepKind::DeleteFile => {
                self.dav_fs
                    .delete_with_locks(
                        &command.source,
                        false,
                        command.operation,
                        command.role,
                        conditions,
                    )
                    .await
            }
            DavMutationStepKind::DeleteCollection => {
                self.dav_fs
                    .delete_with_locks(
                        &command.source,
                        true,
                        command.operation,
                        command.role,
                        conditions,
                    )
                    .await
            }
        };
        result.map_err(|error| match error {
            backend::AsterDavMutationError::FileSystem(error) => {
                let backend_error = error.into();
                DavMutationStepError::from_backend(affected_path, &backend_error)
            }
            backend::AsterDavMutationError::Locked(lock_root) => {
                DavMutationStepError::locked(affected_path, lock_root)
            }
            backend::AsterDavMutationError::Backend => DavMutationStepError::backend(affected_path),
        })
    }
}

const fn mutation_executor_limits() -> DavMutationExecutorLimits {
    DavMutationExecutorLimits::new(
        DavTraversalLimits::new(
            MUTATION_MAXIMUM_VISITED_RESOURCES,
            MUTATION_MAXIMUM_QUEUED_WORK_ITEMS,
            MUTATION_MAXIMUM_FAILURES,
            Some(MUTATION_MAXIMUM_DEPTH),
        ),
        MUTATION_DIRECTORY_LIMITS,
        MUTATION_DIRECTORY_PAGE_ENTRIES,
        MUTATION_MAXIMUM_VISITED_RESOURCES,
    )
}

fn enforce_http_conditionals(
    headers: &actix_web::http::header::HeaderMap,
    method: DavMethod,
    metadata: &dyn DavMetaData,
) -> Result<(), HttpResponse> {
    let last_modified = metadata.modified().ok();
    let etag = metadata.etag();
    let plan = aster_forge_webdav::actix::plan_http_conditionals(
        headers,
        method,
        DavConditionalResource {
            exists: true,
            etag: etag.as_deref(),
            last_modified,
        },
    )?;
    if plan.outcome == DavConditionalOutcome::Proceed {
        return Ok(());
    }
    let status = match plan.outcome {
        DavConditionalOutcome::Proceed => http::StatusCode::OK,
        DavConditionalOutcome::NotModified => http::StatusCode::NOT_MODIFIED,
        DavConditionalOutcome::PreconditionFailed => http::StatusCode::PRECONDITION_FAILED,
    };
    let mut response = DavResponse::empty(status);
    response.headers.insert(
        http::header::CACHE_CONTROL,
        http::HeaderValue::from_static("no-store"),
    );
    plan.apply_response_headers(status, &mut response.headers);
    Err(aster_forge_webdav::actix::into_response(response))
}

pub(crate) async fn handle_mkcol(
    request_head: &DavRequestHead,
    dav_fs: &backend::AsterDavFs,
    lock_system: &dyn DavLockSystem,
    prefix: &str,
    system_file_policy: &system_file::SystemFileBlockPolicy,
) -> HttpResponse {
    let path = request_head.target.clone();
    let relative = path.as_str().to_owned();
    if let Err(error) = validate_collection_create_target(&relative) {
        return aster_forge_webdav::actix::into_response(mutation_plan_error_response(error));
    }
    if let Err(resp) = ensure_system_file_name_allowed(system_file_policy, &relative) {
        return resp;
    }

    if let Err(response) = aster_forge_webdav::enforce_parent_collection(dav_fs, &path).await {
        return aster_forge_webdav::actix::into_response(response);
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
    if let Err(resp) = aster_forge_webdav::actix::enforce_parent_unlocked(
        lock_system,
        &path,
        prefix,
        request_head.if_header.as_ref(),
        request_scheme,
        request_host,
    )
    .await
    {
        return resp;
    }

    match dav_fs.create_dir(&path).await {
        Ok(()) => match collection_created_response(prefix, &path) {
            Ok(response) => aster_forge_webdav::actix::into_response(response),
            Err(_) => responses::empty(StatusCode::INTERNAL_SERVER_ERROR),
        },
        Err(FsError::Exists) => aster_forge_webdav::actix::into_response(
            mutation_plan_error_response(DavMutationPlanError::MethodNotAllowed),
        ),
        Err(FsError::NotFound) => aster_forge_webdav::actix::into_response(
            mutation_plan_error_response(DavMutationPlanError::Conflict),
        ),
        Err(err) => fs_error_response(err),
    }
}

pub(crate) async fn handle_delete(
    req: &HttpRequest,
    request_head: &DavRequestHead,
    dav_fs: &backend::AsterDavFs,
    lock_system: &dyn DavLockSystem,
    prefix: &str,
) -> HttpResponse {
    let Some(depth) = request_head.depth else {
        return responses::empty(StatusCode::INTERNAL_SERVER_ERROR);
    };
    let path = request_head.target.clone();

    let meta = match dav_fs.metadata_for_write(&path).await {
        Ok(meta) => meta,
        Err(err) => return fs_error_response(err),
    };
    let resource_kind = if meta.is_dir() {
        DavResourceKind::Collection
    } else {
        DavResourceKind::File
    };
    if let Err(error) = validate_delete_target(resource_kind, depth) {
        return aster_forge_webdav::actix::into_response(mutation_plan_error_response(error));
    }
    if let Err(resp) = enforce_http_conditionals(req.headers(), DavMethod::Delete, &meta) {
        return resp;
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
    if !meta.is_dir()
        && let Err(resp) = aster_forge_webdav::actix::enforce_unlocked(
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
    if let Err(resp) = aster_forge_webdav::actix::enforce_parent_unlocked(
        lock_system,
        &path,
        prefix,
        request_head.if_header.as_ref(),
        request_scheme,
        request_host,
    )
    .await
    {
        return resp;
    }

    let port = AsterDavMutationPort {
        dav_fs,
        request_head,
        prefix,
    };
    let enumerator = dav_fs.write_directory_enumerator();
    let outcome = execute_recursive_mutation(
        DavMutationRequest {
            operation: DavMutationOperation::Delete,
            source: path,
            source_kind: resource_kind,
            destination: None,
            destination_kind: None,
            destination_existed: false,
            recurse_collections: true,
        },
        &enumerator,
        &port,
        &DavNeverCancelled,
        mutation_executor_limits(),
    )
    .await;
    match mutation_outcome_response(prefix, &outcome, Default::default()) {
        Ok(response) => aster_forge_webdav::actix::into_response(response),
        Err(error) => {
            tracing::warn!(error = %error, "failed to compose WebDAV DELETE response");
            responses::empty(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub(crate) async fn handle_copy_move(
    req: &HttpRequest,
    request_head: &DavRequestHead,
    dav_fs: &backend::AsterDavFs,
    lock_system: &dyn DavLockSystem,
    prefix: &str,
    system_file_policy: &system_file::SystemFileBlockPolicy,
    is_move: bool,
) -> HttpResponse {
    let Some(depth) = request_head.depth else {
        return responses::empty(StatusCode::INTERNAL_SERVER_ERROR);
    };
    let source = request_head.target.clone();
    let source_relative = source.as_str().to_owned();

    let request_scheme = request_head.origin.scheme.as_str();
    let request_host = request_head.origin.host.as_str();
    let Some(destination) = request_head.destination.as_ref() else {
        return responses::empty(StatusCode::INTERNAL_SERVER_ERROR);
    };
    let destination_relative = destination.relative.clone();
    let destination = destination.path.clone();
    if let Err(resp) = ensure_system_file_name_allowed(system_file_policy, &destination_relative) {
        return resp;
    }
    if let Err(response) = aster_forge_webdav::enforce_parent_collection(dav_fs, &destination).await
    {
        return aster_forge_webdav::actix::into_response(response);
    }

    let source_meta = match dav_fs.metadata_for_write(&source).await {
        Ok(meta) => meta,
        Err(err) => return fs_error_response(err),
    };
    if let Err(resp) = enforce_http_conditionals(req.headers(), request_head.method, &source_meta) {
        return resp;
    }
    if let Err(resp) = aster_forge_webdav::actix::enforce_if_header_with_backends(
        request_head.if_header.as_ref(),
        dav_fs,
        lock_system,
        &source,
        prefix,
        request_scheme,
        request_host,
    )
    .await
    {
        return resp;
    }
    if is_move
        && let Err(resp) = aster_forge_webdav::actix::enforce_unlocked(
            lock_system,
            &source,
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
    if is_move
        && let Err(resp) = aster_forge_webdav::actix::enforce_parent_unlocked(
            lock_system,
            &source,
            prefix,
            request_head.if_header.as_ref(),
            request_scheme,
            request_host,
        )
        .await
    {
        return resp;
    }

    let destination_meta = match dav_fs.metadata_for_write(&destination).await {
        Ok(meta) => Some(meta),
        Err(FsError::NotFound) => None,
        Err(err) => return fs_error_response(err),
    };
    let destination_exists = destination_meta.is_some();
    let Some(overwrite) = request_head.overwrite else {
        return responses::empty(StatusCode::INTERNAL_SERVER_ERROR);
    };
    let source_kind = if source_meta.is_dir() {
        DavResourceKind::Collection
    } else {
        DavResourceKind::File
    };
    let destination_kind = destination_meta.as_ref().map(|meta| {
        if meta.is_dir() {
            DavResourceKind::Collection
        } else {
            DavResourceKind::File
        }
    });
    let method = if is_move {
        DavCopyMoveMethod::Move
    } else {
        DavCopyMoveMethod::Copy
    };
    let plan = match plan_copy_move_request(
        method,
        depth,
        source_kind,
        destination_kind,
        &source_relative,
        &destination_relative,
        overwrite,
    ) {
        Ok(plan) => plan,
        Err(error) => {
            return aster_forge_webdav::actix::into_response(mutation_plan_error_response(error));
        }
    };
    if !plan.destination_deep
        && let Err(resp) = aster_forge_webdav::actix::enforce_unlocked(
            lock_system,
            &destination,
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
    if let Err(resp) = aster_forge_webdav::actix::enforce_parent_unlocked(
        lock_system,
        &destination,
        prefix,
        request_head.if_header.as_ref(),
        request_scheme,
        request_host,
    )
    .await
    {
        return resp;
    }

    let operation = if is_move {
        DavMutationOperation::Move
    } else {
        DavMutationOperation::Copy
    };
    let port = AsterDavMutationPort {
        dav_fs,
        request_head,
        prefix,
    };
    let enumerator = dav_fs.write_directory_enumerator();
    let outcome = execute_recursive_mutation(
        DavMutationRequest {
            operation,
            source,
            source_kind,
            destination: Some(destination),
            destination_kind,
            destination_existed: destination_exists,
            recurse_collections: plan.recursive_collection,
        },
        &enumerator,
        &port,
        &DavNeverCancelled,
        mutation_executor_limits(),
    )
    .await;
    match mutation_outcome_response(prefix, &outcome, Default::default()) {
        Ok(response) => aster_forge_webdav::actix::into_response(response),
        Err(error) => {
            tracing::warn!(error = %error, "failed to compose WebDAV COPY/MOVE response");
            responses::empty(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

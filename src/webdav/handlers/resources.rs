//! WebDAV resource mutation handlers: MKCOL, DELETE, COPY, MOVE.

use std::collections::HashMap;

use actix_web::http::StatusCode;
use actix_web::{HttpRequest, HttpResponse};
use aster_forge_webdav::{
    DavBackendErrorKind, DavCancellation, DavConditionalOutcome, DavConditionalResource,
    DavCopyMoveMethod, DavDirectoryEntry, DavDirectoryPageLimits, DavDirectoryPageState,
    DavDirectoryReadError, DavMetaData, DavMethod, DavMutationFailure, DavMutationPlanError,
    DavNeverCancelled, DavRequestHead, DavResourceKind, DavResponse, DavTraversalBudget,
    DavTraversalError, DavTraversalErrorKind, DavTraversalLimits, Depth,
    collection_created_response, delete_success_response, mutation_multistatus_response,
    mutation_plan_error_response, mutation_success_response, plan_copy_move_request,
    read_next_directory_page, resource_identity_path, validate_collection_create_target,
    validate_delete_target,
};

use crate::services::files::folder::FolderTreeTraversalLimits;
use crate::webdav::{
    backend, child_relative_path, ensure_system_file_name_allowed, fs_error_response, responses,
    system_file,
};
use aster_forge_webdav::{DavFileSystem, DavLockSystem, DavPath, FsError};

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

struct DavChild {
    path: DavPath,
    relative: String,
    is_dir: bool,
}

#[derive(Debug)]
struct MutationTraversalNode {
    path: DavPath,
    relative: String,
    is_dir: bool,
    depth: usize,
}

enum MutationTraversalFailure {
    Traversal(DavTraversalError),
    FileSystem(FsError),
}

struct MutationTraversal<'a, C: DavCancellation> {
    budget: DavTraversalBudget,
    cancellation: &'a C,
    work: Vec<MutationTraversalNode>,
}

impl<'a, C: DavCancellation> MutationTraversal<'a, C> {
    fn new(
        roots: impl IntoIterator<Item = MutationTraversalNode>,
        cancellation: &'a C,
        limits: DavTraversalLimits,
    ) -> Result<Self, DavTraversalError> {
        let mut budget = DavTraversalBudget::new(limits)?;
        let work = roots.into_iter().collect::<Vec<_>>();
        budget.reserve_work(work.len())?;
        Ok(Self {
            budget,
            cancellation,
            work,
        })
    }

    fn next(&mut self) -> Result<Option<MutationTraversalNode>, DavTraversalError> {
        self.budget.checkpoint(self.cancellation)?;
        let Some(node) = self.work.pop() else {
            return Ok(None);
        };
        self.budget.complete_work();
        self.budget.visit(node.depth)?;
        Ok(Some(node))
    }

    fn checkpoint(&self) -> Result<(), DavTraversalError> {
        self.budget.checkpoint(self.cancellation)
    }

    fn push_children(
        &mut self,
        parent_depth: usize,
        children: impl IntoIterator<Item = DavChild>,
    ) -> Result<(), DavTraversalError> {
        self.budget.checkpoint(self.cancellation)?;
        let child_depth = parent_depth
            .checked_add(1)
            .ok_or_else(|| DavTraversalError {
                kind: DavTraversalErrorKind::DepthLimitExceeded,
                progress: self.budget.progress(),
            })?;
        let children = children
            .into_iter()
            .map(|child| MutationTraversalNode {
                path: child.path,
                relative: child.relative,
                is_dir: child.is_dir,
                depth: child_depth,
            })
            .collect::<Vec<_>>();
        self.budget.reserve_work(children.len())?;
        self.work.extend(children.into_iter().rev());
        Ok(())
    }

    #[cfg(test)]
    fn record_failure(&mut self) -> Result<(), DavTraversalError> {
        self.budget.record_failure()
    }

    #[cfg(test)]
    fn record_completed_mutation(&mut self) {
        self.budget.record_completed_mutation();
    }
}

struct PartialMutationOutcome {
    failures: Vec<DavMutationFailure>,
    destination_exists: bool,
}

struct PartialMutationContext<'a> {
    dav_fs: &'a backend::AsterDavFs,
    lock_system: &'a dyn DavLockSystem,
    request_head: &'a DavRequestHead,
    prefix: &'a str,
    is_move: bool,
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

#[derive(Clone)]
struct PartialMutationNode {
    source: DavPath,
    source_relative: String,
    destination: DavPath,
    destination_relative: String,
    depth: usize,
}

#[derive(Clone)]
struct DestinationMutationNode {
    path: DavPath,
    relative: String,
    depth: usize,
}

enum PartialMutationWork {
    VisitDirectory(PartialMutationNode),
    ProcessFile(PartialMutationNode),
    FinalizeDirectory(PartialMutationNode),
    RemoveDestinationFile(DestinationMutationNode),
    VisitDestinationDirectory(DestinationMutationNode),
    FinalizeDestinationDirectory(DestinationMutationNode),
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
    if meta.is_dir() {
        if let Some(resp) =
            locked_multi_status_response(lock_system, &path, true, prefix, request_head).await
        {
            return resp;
        }
    } else if let Err(resp) = aster_forge_webdav::actix::enforce_unlocked(
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

    if meta.is_dir()
        && let Err(error) = preflight_recursive_mutation(
            dav_fs,
            [(path.clone(), path.as_str().to_owned())],
            &DavNeverCancelled,
            mutation_traversal_limits(),
        )
        .await
    {
        return mutation_traversal_failure_response(error);
    }

    let result = if meta.is_dir() {
        dav_fs.remove_dir(&path).await
    } else {
        dav_fs.remove_file(&path).await
    };
    match result {
        Ok(()) => {
            if lock_system.delete(&path).await.is_err() {
                return responses::empty(StatusCode::INTERNAL_SERVER_ERROR);
            }
            aster_forge_webdav::actix::into_response(delete_success_response())
        }
        Err(err) => fs_error_response(err),
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
    let destination_is_collection = destination_meta.as_ref().is_some_and(|meta| meta.is_dir());
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

    if plan.recursive_collection {
        let mut roots = vec![(source.clone(), source_relative.clone())];
        if destination_is_collection {
            roots.push((destination.clone(), destination_relative.clone()));
        }
        if let Err(error) = preflight_recursive_mutation(
            dav_fs,
            roots,
            &DavNeverCancelled,
            mutation_traversal_limits(),
        )
        .await
        {
            return mutation_traversal_failure_response(error);
        }
    }

    if plan.recursive_collection {
        let source_conflicts = if is_move {
            match aster_forge_webdav::unsubmitted_lock_conflicts(
                lock_system,
                &source,
                true,
                prefix,
                request_head.if_header.as_ref(),
                &request_head.origin.scheme,
                &request_head.origin.host,
            )
            .await
            {
                Ok(conflicts) => conflicts,
                Err(error) => {
                    return aster_forge_webdav::actix::into_response(
                        aster_forge_webdav::backend_error_response(&error),
                    );
                }
            }
        } else {
            Vec::new()
        };
        let destination_conflicts = match aster_forge_webdav::unsubmitted_lock_conflicts(
            lock_system,
            &destination,
            true,
            prefix,
            request_head.if_header.as_ref(),
            &request_head.origin.scheme,
            &request_head.origin.host,
        )
        .await
        {
            Ok(conflicts) => conflicts,
            Err(error) => {
                return aster_forge_webdav::actix::into_response(
                    aster_forge_webdav::backend_error_response(&error),
                );
            }
        };
        if !source_conflicts.is_empty() || !destination_conflicts.is_empty() {
            let ctx = PartialMutationContext {
                dav_fs,
                lock_system,
                request_head,
                prefix,
                is_move,
            };
            let root = PartialMutationNode {
                source,
                source_relative,
                destination,
                destination_relative,
                depth: 0,
            };
            let outcome = partial_recursive_copy_move(
                &ctx,
                root,
                destination_exists,
                destination_is_collection,
            )
            .await;
            if !outcome.failures.is_empty() {
                return mutation_failure_response(prefix, &outcome.failures);
            }
            return aster_forge_webdav::actix::into_response(mutation_success_response(
                outcome.destination_exists,
            ));
        }
    }

    if plan.destination_deep
        && let Some(resp) =
            locked_multi_status_response(lock_system, &destination, true, prefix, request_head)
                .await
    {
        return resp;
    }

    let result = if is_move {
        dav_fs.rename(&source, &destination).await
    } else if source_meta.is_dir() && depth == Depth::Zero {
        dav_fs.copy_dir_shallow(&source, &destination).await
    } else {
        dav_fs.copy(&source, &destination).await
    };

    match result {
        Ok(()) => {
            if is_move && lock_system.delete(&source).await.is_err() {
                return responses::empty(StatusCode::INTERNAL_SERVER_ERROR);
            }
            aster_forge_webdav::actix::into_response(mutation_success_response(destination_exists))
        }
        Err(err) => fs_error_response(err),
    }
}

async fn locked_multi_status_response(
    lock_system: &dyn DavLockSystem,
    path: &DavPath,
    deep: bool,
    prefix: &str,
    request_head: &DavRequestHead,
) -> Option<HttpResponse> {
    let conflicts = match aster_forge_webdav::unsubmitted_lock_conflicts(
        lock_system,
        path,
        deep,
        prefix,
        request_head.if_header.as_ref(),
        &request_head.origin.scheme,
        &request_head.origin.host,
    )
    .await
    {
        Ok(conflicts) => conflicts,
        Err(error) => {
            return Some(aster_forge_webdav::actix::into_response(
                aster_forge_webdav::backend_error_response(&error),
            ));
        }
    };
    if conflicts.is_empty() {
        return None;
    }

    Some(multi_status_locked_response(prefix, path, &conflicts))
}

fn multi_status_locked_response(
    prefix: &str,
    affected_path: &DavPath,
    locks: &[aster_forge_webdav::DavLock],
) -> HttpResponse {
    let failures = locks
        .iter()
        .map(|lock| DavMutationFailure::locked(affected_path.clone(), (*lock.path).clone()))
        .collect::<Vec<_>>();
    mutation_failure_response(prefix, &failures)
}

async fn partial_recursive_copy_move(
    ctx: &PartialMutationContext<'_>,
    root: PartialMutationNode,
    destination_exists: bool,
    destination_is_collection: bool,
) -> PartialMutationOutcome {
    let mut failures = Vec::new();
    let mut budget = match DavTraversalBudget::new(mutation_traversal_limits()) {
        Ok(mut budget) => {
            if budget.visit(root.depth).is_err() {
                push_fs_failure(
                    &mut failures,
                    &root.destination,
                    FsError::InsufficientStorage,
                );
                return PartialMutationOutcome {
                    failures,
                    destination_exists,
                };
            }
            budget
        }
        Err(_) => {
            push_fs_failure(&mut failures, &root.destination, FsError::GeneralFailure);
            return PartialMutationOutcome {
                failures,
                destination_exists,
            };
        }
    };
    if destination_exists && !destination_is_collection {
        let conflicts = collect_lock_failures(ctx, &root.destination, false).await;
        if !conflicts.is_empty() {
            extend_unique_failures(&mut failures, conflicts);
            return PartialMutationOutcome {
                failures,
                destination_exists,
            };
        }
        if let Err(error) = ctx.dav_fs.remove_file(&root.destination).await {
            push_fs_failure(&mut failures, &root.destination, error);
            return PartialMutationOutcome {
                failures,
                destination_exists,
            };
        }
        if let Err(error) = ctx
            .dav_fs
            .copy_dir_shallow(&root.source, &root.destination)
            .await
        {
            push_fs_failure(&mut failures, &root.destination, error);
            return PartialMutationOutcome {
                failures,
                destination_exists,
            };
        }
    } else if destination_exists && destination_is_collection {
        let conflicts = collect_lock_failures(ctx, &root.destination, false).await;
        if !conflicts.is_empty() {
            extend_unique_failures(&mut failures, conflicts);
            return PartialMutationOutcome {
                failures,
                destination_exists,
            };
        }
    }

    if !destination_exists
        && let Err(error) = ctx
            .dav_fs
            .copy_dir_shallow(&root.source, &root.destination)
            .await
    {
        push_fs_failure(&mut failures, &root.destination, error);
        return PartialMutationOutcome {
            failures,
            destination_exists,
        };
    }

    let mut work = Vec::new();
    if budget.reserve_work(1).is_err() {
        push_fs_failure(
            &mut failures,
            &root.destination,
            FsError::InsufficientStorage,
        );
        return PartialMutationOutcome {
            failures,
            destination_exists,
        };
    }
    work.push(PartialMutationWork::FinalizeDirectory(root.clone()));
    if !push_directory_children(ctx, &root, &mut work, &mut failures, &mut budget).await {
        budget.complete_work();
        work.pop();
    }

    while failures.len() < MUTATION_MAXIMUM_FAILURES
        && let Some(work_item) = work.pop()
    {
        budget.complete_work();
        let node = match &work_item {
            PartialMutationWork::VisitDirectory(node) | PartialMutationWork::ProcessFile(node) => {
                Some((&node.destination, node.depth))
            }
            PartialMutationWork::RemoveDestinationFile(node)
            | PartialMutationWork::VisitDestinationDirectory(node) => {
                Some((&node.path, node.depth))
            }
            PartialMutationWork::FinalizeDirectory(_)
            | PartialMutationWork::FinalizeDestinationDirectory(_) => None,
        };
        if let Some((path, depth)) = node
            && budget.visit(depth).is_err()
        {
            push_fs_failure(&mut failures, path, FsError::InsufficientStorage);
            break;
        }
        match work_item {
            PartialMutationWork::ProcessFile(node) => {
                partial_copy_move_file(ctx, &node, &mut failures).await;
            }
            PartialMutationWork::VisitDirectory(node) => {
                let dest_meta = match ctx.dav_fs.metadata_for_write(&node.destination).await {
                    Ok(meta) => Some(meta),
                    Err(FsError::NotFound) => None,
                    Err(error) => {
                        push_fs_failure(&mut failures, &node.destination, error);
                        continue;
                    }
                };
                if dest_meta.as_ref().is_some_and(|meta| !meta.is_dir()) {
                    let conflicts = collect_lock_failures(ctx, &node.destination, false).await;
                    if !conflicts.is_empty() {
                        extend_unique_failures(&mut failures, conflicts);
                        continue;
                    }
                    if let Err(error) = ctx.dav_fs.remove_file(&node.destination).await {
                        push_fs_failure(&mut failures, &node.destination, error);
                        continue;
                    }
                    if let Err(error) = ctx
                        .dav_fs
                        .copy_dir_shallow(&node.source, &node.destination)
                        .await
                    {
                        push_fs_failure(&mut failures, &node.destination, error);
                        continue;
                    }
                } else if dest_meta.as_ref().is_some_and(|meta| meta.is_dir()) {
                    let conflicts = collect_lock_failures(ctx, &node.destination, false).await;
                    if !conflicts.is_empty() {
                        extend_unique_failures(&mut failures, conflicts);
                        continue;
                    }
                } else {
                    if let Err(error) = ctx
                        .dav_fs
                        .copy_dir_shallow(&node.source, &node.destination)
                        .await
                    {
                        push_fs_failure(&mut failures, &node.destination, error);
                        continue;
                    }
                }

                if budget.reserve_work(1).is_err() {
                    push_fs_failure(
                        &mut failures,
                        &node.destination,
                        FsError::InsufficientStorage,
                    );
                    continue;
                }
                work.push(PartialMutationWork::FinalizeDirectory(node.clone()));
                if !push_directory_children(ctx, &node, &mut work, &mut failures, &mut budget).await
                {
                    budget.complete_work();
                    work.pop();
                }
            }
            PartialMutationWork::FinalizeDirectory(node) if ctx.is_move => {
                let conflicts = collect_lock_failures(ctx, &node.source, false).await;
                if !conflicts.is_empty() {
                    extend_unique_failures(&mut failures, conflicts);
                    continue;
                }
                let remaining =
                    match collect_children(ctx.dav_fs, &node.source, &node.source_relative).await {
                        Ok(remaining) => remaining,
                        Err(error) => {
                            push_fs_failure(&mut failures, &node.source, error);
                            continue;
                        }
                    };
                if remaining.is_empty() {
                    if let Err(error) = ctx.dav_fs.remove_dir(&node.source).await {
                        push_fs_failure(&mut failures, &node.source, error);
                        continue;
                    }
                    if ctx.lock_system.delete(&node.source).await.is_err() {
                        extend_unique_failures(
                            &mut failures,
                            [DavMutationFailure::status(
                                node.source.clone(),
                                StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                            )],
                        );
                    }
                }
            }
            PartialMutationWork::FinalizeDirectory(_) => {}
            PartialMutationWork::RemoveDestinationFile(node) => {
                let conflicts = collect_lock_failures(ctx, &node.path, false).await;
                if !conflicts.is_empty() {
                    extend_unique_failures(&mut failures, conflicts);
                    continue;
                }
                if let Err(error) = ctx.dav_fs.remove_file(&node.path).await {
                    push_fs_failure(&mut failures, &node.path, error);
                }
            }
            PartialMutationWork::VisitDestinationDirectory(node) => {
                if budget.reserve_work(1).is_err() {
                    push_fs_failure(&mut failures, &node.path, FsError::InsufficientStorage);
                    continue;
                }
                work.push(PartialMutationWork::FinalizeDestinationDirectory(
                    node.clone(),
                ));
                if !push_destination_children(ctx, &node, &mut work, &mut failures, &mut budget)
                    .await
                {
                    budget.complete_work();
                    work.pop();
                }
            }
            PartialMutationWork::FinalizeDestinationDirectory(node) => {
                let conflicts = collect_lock_failures(ctx, &node.path, false).await;
                if !conflicts.is_empty() {
                    extend_unique_failures(&mut failures, conflicts);
                    continue;
                }
                let remaining = match collect_children(ctx.dav_fs, &node.path, &node.relative).await
                {
                    Ok(remaining) => remaining,
                    Err(error) => {
                        push_fs_failure(&mut failures, &node.path, error);
                        continue;
                    }
                };
                if remaining.is_empty() {
                    if let Err(error) = ctx.dav_fs.remove_dir(&node.path).await {
                        push_fs_failure(&mut failures, &node.path, error);
                        continue;
                    }
                    if ctx.lock_system.delete(&node.path).await.is_err() {
                        extend_unique_failures(
                            &mut failures,
                            [DavMutationFailure::status(
                                node.path.clone(),
                                StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                            )],
                        );
                    }
                }
            }
        }
    }

    PartialMutationOutcome {
        failures,
        destination_exists,
    }
}

async fn push_directory_children(
    ctx: &PartialMutationContext<'_>,
    node: &PartialMutationNode,
    work: &mut Vec<PartialMutationWork>,
    failures: &mut Vec<DavMutationFailure>,
    budget: &mut DavTraversalBudget,
) -> bool {
    let children = match collect_children(ctx.dav_fs, &node.source, &node.source_relative).await {
        Ok(children) => children,
        Err(error) => {
            push_fs_failure(failures, &node.source, error);
            return false;
        }
    };
    let mut destination_nodes = HashMap::with_capacity(children.len());
    let mut source_work = Vec::with_capacity(children.len());
    let child_depth = match node.depth.checked_add(1) {
        Some(depth) => depth,
        None => {
            push_fs_failure(failures, &node.source, FsError::InsufficientStorage);
            return false;
        }
    };
    for child in children {
        let dest_relative = aster_forge_webdav::replace_relative_prefix(
            &child.relative,
            &node.source_relative,
            &node.destination_relative,
        );
        let dest_path = match DavPath::new(&dest_relative) {
            Ok(path) => path,
            Err(_) => {
                push_fs_failure(failures, &node.source, FsError::BadRequest);
                return false;
            }
        };
        let child_node = PartialMutationNode {
            source: child.path,
            source_relative: child.relative,
            destination: dest_path,
            destination_relative: dest_relative,
            depth: child_depth,
        };
        destination_nodes.insert(
            resource_identity_path(&child_node.destination_relative),
            child.is_dir,
        );
        source_work.push(if child.is_dir {
            PartialMutationWork::VisitDirectory(child_node)
        } else {
            PartialMutationWork::ProcessFile(child_node)
        });
    }
    let destination_children =
        match collect_children(ctx.dav_fs, &node.destination, &node.destination_relative).await {
            Ok(children) => children,
            Err(error) => {
                push_fs_failure(failures, &node.destination, error);
                return true;
            }
        };
    for child in destination_children.into_iter().rev() {
        if destination_nodes.get(&resource_identity_path(&child.relative)) == Some(&child.is_dir) {
            continue;
        }
        let node = DestinationMutationNode {
            path: child.path,
            relative: child.relative,
            depth: child_depth,
        };
        source_work.push(if child.is_dir {
            PartialMutationWork::VisitDestinationDirectory(node)
        } else {
            PartialMutationWork::RemoveDestinationFile(node)
        });
    }
    if budget.reserve_work(source_work.len()).is_err() {
        push_fs_failure(failures, &node.destination, FsError::InsufficientStorage);
        return false;
    }
    for work_item in source_work.into_iter().rev() {
        work.push(work_item);
    }
    true
}

async fn push_destination_children(
    ctx: &PartialMutationContext<'_>,
    node: &DestinationMutationNode,
    work: &mut Vec<PartialMutationWork>,
    failures: &mut Vec<DavMutationFailure>,
    budget: &mut DavTraversalBudget,
) -> bool {
    let children = match collect_children(ctx.dav_fs, &node.path, &node.relative).await {
        Ok(children) => children,
        Err(error) => {
            push_fs_failure(failures, &node.path, error);
            return false;
        }
    };
    let child_depth = match node.depth.checked_add(1) {
        Some(depth) => depth,
        None => {
            push_fs_failure(failures, &node.path, FsError::InsufficientStorage);
            return false;
        }
    };
    let mut child_work = Vec::with_capacity(children.len());
    for child in children {
        let child_node = DestinationMutationNode {
            path: child.path,
            relative: child.relative,
            depth: child_depth,
        };
        child_work.push(if child.is_dir {
            PartialMutationWork::VisitDestinationDirectory(child_node)
        } else {
            PartialMutationWork::RemoveDestinationFile(child_node)
        });
    }
    if budget.reserve_work(child_work.len()).is_err() {
        push_fs_failure(failures, &node.path, FsError::InsufficientStorage);
        return false;
    }
    work.extend(child_work.into_iter().rev());
    true
}

async fn partial_copy_move_file(
    ctx: &PartialMutationContext<'_>,
    node: &PartialMutationNode,
    failures: &mut Vec<DavMutationFailure>,
) {
    if ctx.is_move {
        let conflicts = collect_lock_failures(ctx, &node.source, false).await;
        if !conflicts.is_empty() {
            extend_unique_failures(failures, conflicts);
            return;
        }
    }
    let destination_is_collection = match ctx.dav_fs.metadata_for_write(&node.destination).await {
        Ok(meta) => meta.is_dir(),
        Err(FsError::NotFound) => false,
        Err(error) => {
            push_fs_failure(failures, &node.destination, error);
            return;
        }
    };
    let dest_conflicts =
        collect_lock_failures(ctx, &node.destination, destination_is_collection).await;
    if !dest_conflicts.is_empty() {
        extend_unique_failures(failures, dest_conflicts);
        return;
    }
    if ctx.is_move {
        if let Err(error) = ctx.dav_fs.rename(&node.source, &node.destination).await {
            push_fs_failure(failures, &node.destination, error);
            return;
        }
        if ctx.lock_system.delete(&node.source).await.is_err() {
            extend_unique_failures(
                failures,
                [DavMutationFailure::status(
                    node.source.clone(),
                    StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                )],
            );
        }
    } else if let Err(error) = ctx.dav_fs.copy(&node.source, &node.destination).await {
        push_fs_failure(failures, &node.destination, error);
    }
}

fn push_fs_failure(failures: &mut Vec<DavMutationFailure>, path: &DavPath, error: FsError) {
    let status = match error {
        FsError::NotFound => StatusCode::NOT_FOUND,
        FsError::Forbidden => StatusCode::FORBIDDEN,
        FsError::Exists => StatusCode::CONFLICT,
        FsError::InsufficientStorage => StatusCode::INSUFFICIENT_STORAGE,
        FsError::TooLarge => StatusCode::PAYLOAD_TOO_LARGE,
        FsError::BadRequest => StatusCode::BAD_REQUEST,
        FsError::GeneralFailure => StatusCode::INTERNAL_SERVER_ERROR,
    };
    extend_unique_failures(
        failures,
        [DavMutationFailure::status(path.clone(), status.as_u16())],
    );
}

fn extend_unique_failures(
    failures: &mut Vec<DavMutationFailure>,
    additions: impl IntoIterator<Item = DavMutationFailure>,
) {
    for failure in additions {
        if failures.len() >= MUTATION_MAXIMUM_FAILURES {
            break;
        }
        if failures.contains(&failure) {
            continue;
        }
        failures.push(failure);
    }
}

async fn collect_lock_failures(
    ctx: &PartialMutationContext<'_>,
    path: &DavPath,
    deep: bool,
) -> Vec<DavMutationFailure> {
    match aster_forge_webdav::unsubmitted_lock_conflicts(
        ctx.lock_system,
        path,
        deep,
        ctx.prefix,
        ctx.request_head.if_header.as_ref(),
        &ctx.request_head.origin.scheme,
        &ctx.request_head.origin.host,
    )
    .await
    {
        Ok(conflicts) => conflicts
            .into_iter()
            .map(|lock| DavMutationFailure::locked(path.clone(), (*lock.path).clone()))
            .collect(),
        Err(_) => vec![DavMutationFailure::status(
            path.clone(),
            StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
        )],
    }
}

async fn collect_children(
    dav_fs: &backend::AsterDavFs,
    path: &DavPath,
    relative: &str,
) -> Result<Vec<DavChild>, FsError> {
    collect_children_with_cancellation(dav_fs, path, relative, &DavNeverCancelled).await
}

async fn collect_children_with_cancellation(
    dav_fs: &backend::AsterDavFs,
    path: &DavPath,
    relative: &str,
    cancellation: &impl DavCancellation,
) -> Result<Vec<DavChild>, FsError> {
    let mut state = DavDirectoryPageState::new();
    let enumerator = dav_fs.write_directory_enumerator();
    let mut children = Vec::new();
    loop {
        let page = read_next_directory_page(
            &enumerator,
            path,
            &mut state,
            MUTATION_DIRECTORY_PAGE_ENTRIES,
            MUTATION_DIRECTORY_LIMITS,
            cancellation,
        )
        .await
        .map_err(directory_read_error_to_fs)?;
        let Some(page) = page else {
            break;
        };
        for entry in page.entries {
            let is_dir = entry.metadata().is_dir();
            let child_relative = child_relative_path(relative, entry.name(), is_dir)
                .map_err(|_| FsError::GeneralFailure)?;
            let child_path = DavPath::new(&child_relative).map_err(|_| FsError::GeneralFailure)?;
            children.push(DavChild {
                path: child_path,
                relative: child_relative,
                is_dir,
            });
        }
        if !page.has_more {
            break;
        }
    }
    Ok(children)
}

const fn mutation_traversal_limits() -> DavTraversalLimits {
    DavTraversalLimits::new(
        MUTATION_MAXIMUM_VISITED_RESOURCES,
        MUTATION_MAXIMUM_QUEUED_WORK_ITEMS,
        MUTATION_MAXIMUM_FAILURES,
        Some(MUTATION_MAXIMUM_DEPTH),
    )
}

async fn preflight_recursive_mutation(
    dav_fs: &backend::AsterDavFs,
    roots: impl IntoIterator<Item = (DavPath, String)>,
    cancellation: &impl DavCancellation,
    limits: DavTraversalLimits,
) -> Result<(), MutationTraversalFailure> {
    let roots = roots
        .into_iter()
        .map(|(path, relative)| MutationTraversalNode {
            path,
            relative,
            is_dir: true,
            depth: 0,
        })
        .collect::<Vec<_>>();
    let mut traversal = MutationTraversal::new(roots, cancellation, limits)
        .map_err(MutationTraversalFailure::Traversal)?;

    while let Some(node) = traversal
        .next()
        .map_err(MutationTraversalFailure::Traversal)?
    {
        if !node.is_dir {
            continue;
        }

        let children = match collect_children_with_cancellation(
            dav_fs,
            &node.path,
            &node.relative,
            cancellation,
        )
        .await
        {
            Ok(children) => children,
            Err(error) => {
                traversal
                    .checkpoint()
                    .map_err(MutationTraversalFailure::Traversal)?;
                return Err(MutationTraversalFailure::FileSystem(error));
            }
        };
        traversal
            .push_children(node.depth, children)
            .map_err(MutationTraversalFailure::Traversal)?;
    }
    Ok(())
}

fn mutation_traversal_failure_response(error: MutationTraversalFailure) -> HttpResponse {
    match error {
        MutationTraversalFailure::FileSystem(error) => fs_error_response(error),
        MutationTraversalFailure::Traversal(error) => {
            let status = match error.kind {
                DavTraversalErrorKind::InvalidLimits => StatusCode::INTERNAL_SERVER_ERROR,
                DavTraversalErrorKind::Cancelled => StatusCode::SERVICE_UNAVAILABLE,
                DavTraversalErrorKind::VisitedResourceLimitExceeded
                | DavTraversalErrorKind::QueuedWorkLimitExceeded
                | DavTraversalErrorKind::FailureLimitExceeded
                | DavTraversalErrorKind::DepthLimitExceeded => StatusCode::INSUFFICIENT_STORAGE,
            };
            responses::empty(status)
        }
    }
}

fn directory_read_error_to_fs(error: DavDirectoryReadError) -> FsError {
    match error {
        DavDirectoryReadError::PageLimitExceeded => FsError::InsufficientStorage,
        DavDirectoryReadError::Backend(error) => match error.kind {
            DavBackendErrorKind::NotFound => FsError::NotFound,
            DavBackendErrorKind::Forbidden | DavBackendErrorKind::Locked => FsError::Forbidden,
            DavBackendErrorKind::Conflict | DavBackendErrorKind::AlreadyExists => FsError::Exists,
            DavBackendErrorKind::InsufficientStorage => FsError::InsufficientStorage,
            DavBackendErrorKind::PayloadTooLarge => FsError::TooLarge,
            DavBackendErrorKind::InvalidInput => FsError::BadRequest,
            DavBackendErrorKind::Unsupported | DavBackendErrorKind::Internal => {
                FsError::GeneralFailure
            }
        },
        DavDirectoryReadError::InvalidLimit
        | DavDirectoryReadError::Cancelled
        | DavDirectoryReadError::InvalidPage(_) => FsError::GeneralFailure,
    }
}

fn mutation_failure_response(prefix: &str, failures: &[DavMutationFailure]) -> HttpResponse {
    match mutation_multistatus_response(prefix, failures) {
        Ok(response) => aster_forge_webdav::actix::into_response(response),
        Err(_) => responses::empty(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::{
        DavChild, MUTATION_MAXIMUM_FAILURES, MutationTraversal, MutationTraversalNode,
        directory_read_error_to_fs, extend_unique_failures, push_fs_failure,
    };
    use actix_web::http::StatusCode;
    use aster_forge_webdav::{
        DavCancellation, DavMutationFailure, DavPath, DavTraversalErrorKind, DavTraversalLimits,
        FsError,
    };

    struct TestCancellation(AtomicBool);

    impl TestCancellation {
        const fn new() -> Self {
            Self(AtomicBool::new(false))
        }
    }

    impl DavCancellation for TestCancellation {
        fn is_cancelled(&self) -> bool {
            self.0.load(Ordering::SeqCst)
        }
    }

    fn traversal_node(path: &str, depth: usize) -> MutationTraversalNode {
        MutationTraversalNode {
            path: DavPath::new(path).unwrap(),
            relative: path.to_owned(),
            is_dir: true,
            depth,
        }
    }

    fn child(path: &str, is_dir: bool) -> DavChild {
        DavChild {
            path: DavPath::new(path).unwrap(),
            relative: path.to_owned(),
            is_dir,
        }
    }

    #[test]
    fn recursive_mutation_fs_errors_keep_resource_statuses() {
        let path = DavPath::new("/failed.txt").unwrap();
        let cases = [
            (FsError::NotFound, StatusCode::NOT_FOUND),
            (FsError::Forbidden, StatusCode::FORBIDDEN),
            (FsError::Exists, StatusCode::CONFLICT),
            (
                FsError::InsufficientStorage,
                StatusCode::INSUFFICIENT_STORAGE,
            ),
            (FsError::TooLarge, StatusCode::PAYLOAD_TOO_LARGE),
            (FsError::BadRequest, StatusCode::BAD_REQUEST),
            (FsError::GeneralFailure, StatusCode::INTERNAL_SERVER_ERROR),
        ];

        for (error, status) in cases {
            let mut failures = Vec::new();
            push_fs_failure(&mut failures, &path, error);
            assert_eq!(
                failures,
                vec![DavMutationFailure::status(path.clone(), status.as_u16())]
            );
        }
    }

    #[test]
    fn recursive_mutation_directory_page_limit_maps_to_insufficient_storage() {
        assert_eq!(
            directory_read_error_to_fs(
                aster_forge_webdav::DavDirectoryReadError::PageLimitExceeded,
            ),
            FsError::InsufficientStorage
        );
    }

    #[test]
    fn recursive_mutation_traversal_enforces_exact_limits() {
        let cancellation = TestCancellation::new();
        let mut traversal = MutationTraversal::new(
            [traversal_node("/root/", 0)],
            &cancellation,
            DavTraversalLimits::new(2, 2, 1, Some(1)),
        )
        .unwrap();
        let root = traversal.next().unwrap().unwrap();
        traversal
            .push_children(
                root.depth,
                [child("/root/a/", true), child("/root/b", false)],
            )
            .expect("two children exactly fill the queue limit");
        assert_eq!(traversal.next().unwrap().unwrap().path.as_str(), "/root/a/");
        assert_eq!(
            traversal
                .next()
                .expect_err("root plus two children exceeds visit limit")
                .kind,
            DavTraversalErrorKind::VisitedResourceLimitExceeded
        );

        let mut queue_limited = MutationTraversal::new(
            [traversal_node("/root/", 0)],
            &cancellation,
            DavTraversalLimits::new(8, 2, 1, Some(8)),
        )
        .unwrap();
        let root = queue_limited.next().unwrap().unwrap();
        queue_limited
            .push_children(
                root.depth,
                [child("/root/a", false), child("/root/b", false)],
            )
            .expect("exact queue limit");
        assert_eq!(
            queue_limited
                .push_children(root.depth, [child("/root/c", false)])
                .expect_err("limit plus one queued item")
                .kind,
            DavTraversalErrorKind::QueuedWorkLimitExceeded
        );

        let mut depth_limited = MutationTraversal::new(
            [traversal_node("/root/", 1)],
            &cancellation,
            DavTraversalLimits::new(2, 2, 1, Some(1)),
        )
        .unwrap();
        let root = depth_limited.next().unwrap().unwrap();
        depth_limited
            .push_children(root.depth, [child("/root/deep/", true)])
            .unwrap();
        assert_eq!(
            depth_limited.next().expect_err("depth limit plus one").kind,
            DavTraversalErrorKind::DepthLimitExceeded
        );

        traversal.record_failure().expect("exact failure limit");
        assert_eq!(
            traversal
                .record_failure()
                .expect_err("failure limit plus one")
                .kind,
            DavTraversalErrorKind::FailureLimitExceeded
        );
    }

    #[test]
    fn recursive_mutation_cancellation_keeps_partial_execution_progress() {
        let cancellation = TestCancellation::new();
        let mut traversal = MutationTraversal::new(
            [traversal_node("/root/", 0)],
            &cancellation,
            DavTraversalLimits::new(2, 2, 1, Some(1)),
        )
        .unwrap();
        traversal.next().unwrap().unwrap();
        traversal.record_completed_mutation();
        cancellation.0.store(true, Ordering::SeqCst);

        let error = traversal.next().expect_err("cancelled traversal");
        assert_eq!(error.kind, DavTraversalErrorKind::Cancelled);
        assert!(error.partial_execution());
        assert_eq!(error.progress.completed_mutations, 1);
        assert_eq!(error.progress.visited_resources, 1);
    }

    #[test]
    fn recursive_mutation_failure_collection_has_a_hard_limit() {
        let mut failures = Vec::new();
        extend_unique_failures(
            &mut failures,
            (0..=MUTATION_MAXIMUM_FAILURES).map(|index| {
                DavMutationFailure::status(
                    DavPath::new(&format!("/failure-{index}")).unwrap(),
                    StatusCode::LOCKED.as_u16(),
                )
            }),
        );
        assert_eq!(failures.len(), MUTATION_MAXIMUM_FAILURES);
        assert_eq!(
            failures.last(),
            Some(&DavMutationFailure::status(
                DavPath::new(&format!("/failure-{}", MUTATION_MAXIMUM_FAILURES - 1)).unwrap(),
                StatusCode::LOCKED.as_u16(),
            ))
        );
    }
}

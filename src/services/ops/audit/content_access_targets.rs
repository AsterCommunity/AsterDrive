//! Target names for legacy administrator access rows that recorded only the stable ID.
use crate::{
    db::repository::{team_repo, user_repo},
    errors::Result,
    runtime::SharedRuntimeState,
};
use aster_drive_model::{entities::audit_log, types::AuditAction};
use std::collections::HashMap;

pub(super) fn is_content_access(action: AuditAction) -> bool {
    matches!(
        action,
        AuditAction::AdminWorkspaceInspect
            | AuditAction::AdminFileDownload
            | AuditAction::AdminRevisionDownload
    )
}
pub(super) async fn enrich(
    state: &impl SharedRuntimeState,
    entries: &mut [audit_log::Model],
) -> Result<()> {
    let user_ids = entries
        .iter()
        .filter(|e| {
            is_content_access(e.action) && e.entity_type == "user" && e.entity_name.is_none()
        })
        .filter_map(|e| e.entity_id)
        .collect::<Vec<_>>();
    let team_ids = entries
        .iter()
        .filter(|e| {
            is_content_access(e.action) && e.entity_type == "team" && e.entity_name.is_none()
        })
        .filter_map(|e| e.entity_id)
        .collect::<Vec<_>>();
    let users = user_repo::find_by_ids(state.writer_db(), &user_ids)
        .await?
        .into_iter()
        .map(|u| (u.id, u.username))
        .collect::<HashMap<_, _>>();
    let teams = team_repo::find_by_ids(state.writer_db(), &team_ids)
        .await?
        .into_iter()
        .map(|t| (t.id, t.name))
        .collect::<HashMap<_, _>>();
    for entry in entries
        .iter_mut()
        .filter(|e| is_content_access(e.action) && e.entity_name.is_none())
    {
        entry.entity_name = entry
            .entity_id
            .and_then(|id| match entry.entity_type.as_str() {
                "user" => users.get(&id),
                "team" => teams.get(&id),
                _ => None,
            })
            .cloned();
    }
    Ok(())
}

//! Bounded streaming export from the same activity facts used by resource/team views.
use super::{
    activity::{self, ActivityListQuery},
    scope::WorkspaceStorageScope,
};
use crate::{
    db::repository::workspace_activity_repo::{self, ActivityQuery},
    errors::{AsterError, Result},
    runtime::PrimaryAppState,
};
use bytes::Bytes;
use futures::Stream;
use std::pin::Pin;

pub type ActivityCsvStream = Pin<Box<dyn Stream<Item = Result<Bytes>> + Send>>;
pub struct ActivityCsvExport {
    pub total: u64,
    pub stream: ActivityCsvStream,
}

fn chunk(rows: Vec<Vec<String>>) -> Result<Bytes> {
    let mut writer = csv::WriterBuilder::new()
        .has_headers(false)
        .from_writer(Vec::new());
    for row in rows {
        writer
            .write_record(
                row.into_iter()
                    .map(crate::services::ops::audit::neutralize_csv_formula),
            )
            .map_err(|e| AsterError::internal_error(format!("write activity CSV: {e}")))?;
    }
    writer
        .into_inner()
        .map(Bytes::from)
        .map_err(|e| AsterError::internal_error(format!("finish activity CSV: {e}")))
}

pub(crate) async fn prepare(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
) -> Result<ActivityCsvExport> {
    activity::list_activities(
        state,
        scope,
        None,
        &ActivityListQuery {
            before_id: None,
            limit: Some(1),
        },
    )
    .await?;
    let team_id = scope
        .team_id()
        .ok_or_else(|| AsterError::auth_forbidden("team scope required"))?;
    prepare_authorized_team(state, team_id).await
}

pub(crate) async fn prepare_admin(
    state: &PrimaryAppState,
    team_id: i64,
) -> Result<ActivityCsvExport> {
    activity::list_admin_team(
        state,
        team_id,
        &ActivityListQuery {
            before_id: None,
            limit: Some(1),
        },
    )
    .await?;
    prepare_authorized_team(state, team_id).await
}

async fn prepare_authorized_team(
    state: &PrimaryAppState,
    team_id: i64,
) -> Result<ActivityCsvExport> {
    let snapshot = workspace_activity_repo::export_snapshot(
        state.writer_db(),
        team_id,
        crate::services::ops::audit::AUDIT_CSV_EXPORT_MAX_ROWS,
    )
    .await?;
    let total = snapshot.map_or(0, |(_, count)| count);
    if total > crate::services::ops::audit::AUDIT_CSV_EXPORT_MAX_ROWS {
        return Err(AsterError::validation_error(
            "activity export exceeds row limit",
        ));
    }
    let state = state.clone();
    let stream = Box::pin(async_stream::try_stream! {
        yield chunk(vec![vec!["event_id","operation_id","created_at","entity_type","entity_id","entity_name","actor_user_id","actor_name","action","source","details"].into_iter().map(str::to_owned).collect()])?;
        let mut sent=0u64;
        if let Some((max_id,_))=snapshot {
            let mut before_id=None;
            loop {
                let records=workspace_activity_repo::list(state.writer_db(),ActivityQuery{team_id:Some(team_id),owner_user_id:None,entity:None,before_id,at_most_id:Some(max_id),limit:200}).await?;
                if records.is_empty() { break; }
                before_id=records.last().map(|r|r.id);
                sent=sent.checked_add(u64::try_from(records.len()).map_err(|_|AsterError::internal_error("activity export count overflow"))?).ok_or_else(||AsterError::internal_error("activity export count overflow"))?;
                yield chunk(records.into_iter().map(|r|vec![r.event_id,r.operation_id,r.created_at.to_rfc3339(),r.entity_type,r.entity_id.to_string(),r.entity_name,r.actor_user_id.map(|id|id.to_string()).unwrap_or_default(),r.actor_name,r.action,r.source,r.details]).collect())?;
            }
        }
        if sent!=total { Err(AsterError::internal_error("activity export incomplete; retention changed its snapshot"))?; }
    });
    Ok(ActivityCsvExport { total, stream })
}

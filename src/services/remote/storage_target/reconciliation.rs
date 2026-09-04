use crate::db::repository::remote_storage_target_repo;
use crate::errors::Result;
use crate::runtime::FollowerRuntimeState;
use aster_drive_model::entities::remote_storage_target;

use super::driver::validate_driver_from_target;

pub(super) async fn reconcile_target<S: FollowerRuntimeState>(
    state: &S,
    target: remote_storage_target::Model,
) -> Result<remote_storage_target::Model> {
    let apply_result = validate_driver_from_target(state, &target).await;

    match apply_result {
        Ok(()) => {
            remote_storage_target_repo::update_reconciliation_if_revision(
                state.writer_db(),
                target.id,
                target.desired_revision,
                Some(target.desired_revision),
                "",
            )
            .await
        }
        Err(error) => {
            remote_storage_target_repo::update_reconciliation_if_revision(
                state.writer_db(),
                target.id,
                target.desired_revision,
                None,
                error.message(),
            )
            .await
        }
    }
}

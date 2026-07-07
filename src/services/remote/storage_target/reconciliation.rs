use chrono::Utc;
use sea_orm::Set;

use crate::db::repository::remote_storage_target_repo;
use crate::entities::remote_storage_target;
use crate::errors::Result;
use crate::runtime::FollowerRuntimeState;

use super::driver::validate_driver_from_target;

pub(super) async fn reconcile_target<S: FollowerRuntimeState>(
    state: &S,
    target: remote_storage_target::Model,
) -> Result<remote_storage_target::Model> {
    let apply_result = validate_driver_from_target(state, &target);

    let mut active: remote_storage_target::ActiveModel = target.clone().into();
    match apply_result {
        Ok(()) => {
            active.applied_revision = Set(target.desired_revision);
            active.last_error = Set(String::new());
        }
        Err(error) => {
            active.last_error = Set(error.message().to_string());
        }
    }
    active.updated_at = Set(Utc::now());
    remote_storage_target_repo::update(state.writer_db(), active).await
}

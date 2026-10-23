//! 仓储模块：`mfa_totp_used_step_repo`。

use chrono::{DateTime, Utc};
use sea_orm::{
    ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, Set, SqlErr, TryInsertResult,
};

use crate::errors::{AsterError, Result};
use aster_drive_model::entities::mfa_totp_used_step::{self, Entity as MfaTotpUsedStep};

/// 原子记录一个已消费的时间步。同一 factor 的同一 time_step 只能成功
/// 记录一次；再次记录返回 `false`，由调用方按验证码重放拒绝。
pub async fn record_used_step<C: ConnectionTrait>(
    db: &C,
    factor_id: i64,
    time_step: i64,
    now: DateTime<Utc>,
) -> Result<bool> {
    match MfaTotpUsedStep::insert(mfa_totp_used_step::ActiveModel {
        factor_id: Set(factor_id),
        time_step: Set(time_step),
        used_at: Set(now),
        ..Default::default()
    })
    .on_conflict_do_nothing_on([
        mfa_totp_used_step::Column::FactorId,
        mfa_totp_used_step::Column::TimeStep,
    ])
    .exec(db)
    .await
    {
        Ok(TryInsertResult::Inserted(_)) => Ok(true),
        Ok(TryInsertResult::Conflicted) => Ok(false),
        Ok(TryInsertResult::Empty) => Err(AsterError::internal_error(
            "mfa totp used step insert produced empty result",
        )),
        Err(err) if matches!(err.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) => {
            Ok(false)
        }
        Err(err) => Err(AsterError::from(err)),
    }
}

/// 清理早于 `min_time_step` 的记录，避免表随验证无限增长。
pub async fn delete_steps_before<C: ConnectionTrait>(
    db: &C,
    factor_id: i64,
    min_time_step: i64,
) -> Result<()> {
    MfaTotpUsedStep::delete_many()
        .filter(mfa_totp_used_step::Column::FactorId.eq(factor_id))
        .filter(mfa_totp_used_step::Column::TimeStep.lt(min_time_step))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

pub async fn delete_all_for_factor<C: ConnectionTrait>(db: &C, factor_id: i64) -> Result<()> {
    MfaTotpUsedStep::delete_many()
        .filter(mfa_totp_used_step::Column::FactorId.eq(factor_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

pub async fn delete_all_for_factors<C: ConnectionTrait>(
    db: &C,
    factor_ids: Vec<i64>,
) -> Result<()> {
    if factor_ids.is_empty() {
        return Ok(());
    }
    MfaTotpUsedStep::delete_many()
        .filter(mfa_totp_used_step::Column::FactorId.is_in(factor_ids))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

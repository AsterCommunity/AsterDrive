//! 服务模块：`mail::outbox`。

use std::sync::Arc;

use chrono::{Duration, Utc};
use sea_orm::{ConnectionTrait, DatabaseConnection, Set};

use crate::config::RuntimeConfig;
use crate::db::repository::mail_outbox_repo;
use crate::entities::mail_outbox;
use crate::errors::Result;
use crate::runtime::MailRuntimeState;
use crate::services::{
    mail::audit,
    mail::sender::{self, MailSender},
    mail::template::{self, MailTemplatePayload},
};
use crate::types::MailOutboxStatus;

const MAIL_OUTBOX_BATCH_SIZE: u64 = 20;
const MAIL_OUTBOX_PROCESSING_STALE_SECS: i64 = 60;
const MAIL_OUTBOX_MAX_ATTEMPTS: i32 = 6;
const MAIL_OUTBOX_MAX_ERROR_LEN: usize = 1024;
const MAIL_OUTBOX_DRAIN_MAX_ROUNDS: usize = 32;

/// SMTP 已成功交付但 `mark_sent` DB 写失败时的重试退避（毫秒）。
/// 总预算约 7.6s，远小于 `MAIL_OUTBOX_PROCESSING_STALE_SECS`，
/// 给 DB 抖动留缓冲，降低"SMTP 成功 + row 残留 Processing → 另一个 worker 再发"的双发窗口。
const MARK_SENT_RETRY_DELAYS_MS: &[u64] = &[0, 100, 500, 2_000, 5_000];

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DispatchStats {
    pub claimed: usize,
    pub sent: usize,
    pub retried: usize,
    pub failed: usize,
}

impl DispatchStats {
    fn merge(&mut self, other: DispatchStats) {
        self.claimed += other.claimed;
        self.sent += other.sent;
        self.retried += other.retried;
        self.failed += other.failed;
    }
}

pub(crate) async fn enqueue<C: ConnectionTrait>(
    db: &C,
    to_address: &str,
    to_name: Option<&str>,
    payload: MailTemplatePayload,
) -> Result<mail_outbox::Model> {
    let now = Utc::now();
    mail_outbox_repo::create(
        db,
        mail_outbox::ActiveModel {
            template_code: Set(payload.template_code()),
            to_address: Set(to_address.to_string()),
            to_name: Set(to_name.map(str::to_string)),
            payload_json: Set(payload.to_stored()?),
            status: Set(MailOutboxStatus::Pending),
            attempt_count: Set(0),
            next_attempt_at: Set(now),
            processing_started_at: Set(None),
            sent_at: Set(None),
            last_error: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await
}

pub async fn dispatch_due(state: &impl MailRuntimeState) -> Result<DispatchStats> {
    dispatch_due_with(
        state.writer_db(),
        state.runtime_config(),
        state.mail_sender(),
    )
    .await
}

pub async fn dispatch_due_with(
    db: &DatabaseConnection,
    runtime_config: &Arc<RuntimeConfig>,
    mail_sender: &Arc<dyn MailSender>,
) -> Result<DispatchStats> {
    let now = Utc::now();
    let stale_before = now - Duration::seconds(MAIL_OUTBOX_PROCESSING_STALE_SECS);
    let due =
        mail_outbox_repo::list_claimable(db, now, stale_before, MAIL_OUTBOX_BATCH_SIZE).await?;
    let mut stats = DispatchStats::default();

    for row in due {
        let claimed_at = Utc::now();
        if !mail_outbox_repo::try_claim(db, row.id, claimed_at, stale_before).await? {
            continue;
        }

        stats.claimed += 1;
        let mut claimed_row = row;
        claimed_row.status = MailOutboxStatus::Processing;
        claimed_row.processing_started_at = Some(claimed_at);
        claimed_row.updated_at = claimed_at;

        match deliver_one(runtime_config, mail_sender, &claimed_row).await {
            Ok(subject) => {
                // SMTP 成功。mark_sent 必须尽一切努力落库——否则 row 会以 Processing 状态
                // 在 `MAIL_OUTBOX_PROCESSING_STALE_SECS` 后被另一个 worker 再次 claim，
                // 导致**收件人收到重复邮件**。退避重试把"瞬时 DB 抖动 → 双发"的概率
                // 降到最低；如果最终仍失败，日志标红让 ops 能追踪到。
                match mark_sent_with_retry(db, claimed_row.id).await {
                    Ok(true) => {
                        stats.sent += 1;
                        audit::log_send_with_db(
                            db,
                            runtime_config,
                            audit::MailAuditInput {
                                actor_user_id: 0,
                                ip_address: None,
                                user_agent: None,
                                to_address: &claimed_row.to_address,
                                to_name: claimed_row.to_name.as_deref(),
                                template_code: claimed_row.template_code.as_str(),
                                subject: Some(&subject),
                                outbox_id: Some(claimed_row.id),
                                attempt_count: Some(claimed_row.attempt_count + 1),
                                error: None,
                            },
                        )
                        .await;
                    }
                    Ok(false) => {
                        tracing::warn!(
                            mail_outbox_id = claimed_row.id,
                            template_code = %claimed_row.template_code.as_str(),
                            to = %claimed_row.to_address,
                            "mark_sent affected 0 rows after successful delivery; state will be rechecked"
                        );
                    }
                    Err(e) => {
                        tracing::error!(
                            mail_outbox_id = claimed_row.id,
                            template_code = %claimed_row.template_code.as_str(),
                            to = %claimed_row.to_address,
                            stale_secs = MAIL_OUTBOX_PROCESSING_STALE_SECS,
                            error = %e,
                            "CRITICAL: SMTP delivery succeeded but mark_sent failed after all retries; \
                             row remains Processing and may be re-claimed, causing duplicate delivery"
                        );
                    }
                }
            }
            Err(error) => {
                let attempt_count = claimed_row.attempt_count + 1;
                let error_message = truncate_error(&error.to_string());
                if attempt_count >= MAIL_OUTBOX_MAX_ATTEMPTS {
                    if mail_outbox_repo::mark_failed(
                        db,
                        claimed_row.id,
                        attempt_count,
                        Utc::now(),
                        &error_message,
                    )
                    .await?
                    {
                        stats.failed += 1;
                        audit::log_delivery_failed_with_db(
                            db,
                            runtime_config,
                            audit::MailAuditInput {
                                actor_user_id: 0,
                                ip_address: None,
                                user_agent: None,
                                to_address: &claimed_row.to_address,
                                to_name: claimed_row.to_name.as_deref(),
                                template_code: claimed_row.template_code.as_str(),
                                subject: None,
                                outbox_id: Some(claimed_row.id),
                                attempt_count: Some(attempt_count),
                                error: Some(&error_message),
                            },
                        )
                        .await;
                    }
                    tracing::warn!(
                        mail_outbox_id = claimed_row.id,
                        template_code = %claimed_row.template_code.as_str(),
                        to = %claimed_row.to_address,
                        attempt_count,
                        error = %error_message,
                        "mail outbox delivery permanently failed"
                    );
                } else {
                    let retry_at = Utc::now() + Duration::seconds(retry_delay_secs(attempt_count));
                    if mail_outbox_repo::mark_retry(
                        db,
                        claimed_row.id,
                        attempt_count,
                        retry_at,
                        &error_message,
                    )
                    .await?
                    {
                        stats.retried += 1;
                    }
                    tracing::warn!(
                        mail_outbox_id = claimed_row.id,
                        template_code = %claimed_row.template_code.as_str(),
                        to = %claimed_row.to_address,
                        attempt_count,
                        retry_at = %retry_at,
                        error = %error_message,
                        "mail outbox delivery failed; scheduled retry"
                    );
                }
            }
        }
    }

    Ok(stats)
}

pub async fn drain(state: &impl MailRuntimeState) -> Result<DispatchStats> {
    drain_with(
        state.writer_db(),
        state.runtime_config(),
        state.mail_sender(),
    )
    .await
}

pub async fn drain_with(
    db: &DatabaseConnection,
    runtime_config: &Arc<RuntimeConfig>,
    mail_sender: &Arc<dyn MailSender>,
) -> Result<DispatchStats> {
    let mut total = DispatchStats::default();

    for _ in 0..MAIL_OUTBOX_DRAIN_MAX_ROUNDS {
        let stats = dispatch_due_with(db, runtime_config, mail_sender).await?;
        let claimed = stats.claimed;
        total.merge(stats);
        if claimed == 0 {
            break;
        }
    }

    Ok(total)
}

async fn deliver_one(
    runtime_config: &RuntimeConfig,
    mail_sender: &Arc<dyn MailSender>,
    row: &mail_outbox::Model,
) -> Result<String> {
    let rendered = template::render(runtime_config, row.template_code, &row.payload_json)?;
    let subject = rendered.subject.clone();
    sender::send_rendered_with(
        runtime_config,
        mail_sender,
        sender::MailRecipient {
            address: row.to_address.clone(),
            display_name: row.to_name.clone(),
        },
        rendered,
    )
    .await?;
    Ok(subject)
}

fn retry_delay_secs(attempt_count: i32) -> i64 {
    match attempt_count {
        1 => 5,
        2 => 15,
        3 => 60,
        4 => 300,
        5 => 900,
        _ => 1800,
    }
}

/// SMTP 成功后持久化"已发送"状态；DB 失败时退避重试以压缩双发窗口。
///
/// 返回 `Ok(true)` = 标记成功（首行命中）；`Ok(false)` = 没有行被更新（可能已是 Sent）；
/// `Err(...)` = 最终仍失败，调用方应当打印 critical 日志。
async fn mark_sent_with_retry(db: &DatabaseConnection, id: i64) -> Result<bool> {
    let mut last_err: Option<crate::errors::AsterError> = None;
    for (i, delay_ms) in MARK_SENT_RETRY_DELAYS_MS.iter().enumerate() {
        if *delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(*delay_ms)).await;
        }
        match mail_outbox_repo::mark_sent(db, id, Utc::now()).await {
            Ok(updated) => return Ok(updated),
            Err(err) => {
                tracing::warn!(
                    mail_outbox_id = id,
                    attempt = i + 1,
                    "mark_sent failed, will retry: {err}"
                );
                last_err = Some(err);
            }
        }
    }
    Err(last_err.unwrap_or_else(|| {
        crate::errors::AsterError::internal_error(
            "mark_sent retries exhausted without error context",
        )
    }))
}

fn truncate_error(error: &str) -> String {
    error.chars().take(MAIL_OUTBOX_MAX_ERROR_LEN).collect()
}

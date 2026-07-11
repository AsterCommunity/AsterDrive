//! 服务模块：`mail::outbox`。

use std::sync::Arc;

use chrono::Utc;
use sea_orm::{ConnectionTrait, DatabaseConnection};

use crate::config::RuntimeConfig;
use crate::errors::Result;
use crate::runtime::MailRuntimeState;
use crate::services::{
    mail::audit,
    mail::sender::{self, MailSender},
    mail::template::{self, MailTemplatePayload},
};
use aster_forge_mail::{DispatchStats, MailOutboxDispatchConfig, MailOutboxRetryPolicy};

const MAIL_OUTBOX_BATCH_SIZE: u64 = 20;
const MAIL_OUTBOX_PROCESSING_STALE_SECS: i64 = 60;
const MAIL_OUTBOX_MAX_ATTEMPTS: i32 = 6;
const MAIL_OUTBOX_DRAIN_MAX_ROUNDS: usize = 32;
const MAIL_OUTBOX_DISPATCH_CONFIG: MailOutboxDispatchConfig = MailOutboxDispatchConfig::new(
    MAIL_OUTBOX_BATCH_SIZE,
    MAIL_OUTBOX_PROCESSING_STALE_SECS,
    MAIL_OUTBOX_DRAIN_MAX_ROUNDS,
    MailOutboxRetryPolicy::new(
        MAIL_OUTBOX_MAX_ATTEMPTS,
        aster_forge_mail::DEFAULT_ERROR_MAX_LEN,
    ),
);

pub(crate) async fn enqueue<C: ConnectionTrait>(
    db: &C,
    to_address: &str,
    to_name: Option<&str>,
    payload: MailTemplatePayload,
) -> Result<aster_forge_db::mail_outbox::Model> {
    let now = Utc::now();
    aster_forge_db::create_mail_outbox_row(
        db,
        aster_forge_db::MailOutboxCreate {
            template_code: payload.template_code(),
            to_address: to_address.to_string(),
            to_name: to_name.map(str::to_string),
            payload_json: payload.to_stored()?,
            next_attempt_at: now,
            now,
        },
    )
    .await
    .map_err(Into::into)
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
    let store = aster_forge_db::MailOutboxDbStore::new(db.clone());
    store
        .dispatch_due(
            &MAIL_OUTBOX_DISPATCH_CONFIG,
            |row| async move { deliver_one(runtime_config, mail_sender, &row).await },
            |context, attempt_count, subject| async move {
                audit::log_send_with_db(
                    db,
                    runtime_config,
                    audit::MailAuditInput {
                        actor_user_id: 0,
                        ip_address: None,
                        user_agent: None,
                        to_address: &context.to_address,
                        to_name: context.to_name.as_deref(),
                        template_code: &context.template_code,
                        subject: Some(&subject),
                        outbox_id: Some(context.id),
                        attempt_count: Some(attempt_count),
                        error: None,
                    },
                )
                .await;
            },
            |context, attempt_count, error_message| async move {
                audit::log_delivery_failed_with_db(
                    db,
                    runtime_config,
                    audit::MailAuditInput {
                        actor_user_id: 0,
                        ip_address: None,
                        user_agent: None,
                        to_address: &context.to_address,
                        to_name: context.to_name.as_deref(),
                        template_code: &context.template_code,
                        subject: None,
                        outbox_id: Some(context.id),
                        attempt_count: Some(attempt_count),
                        error: Some(&error_message),
                    },
                )
                .await;
            },
        )
        .await
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
    aster_forge_mail::drain_mail_outbox(&MAIL_OUTBOX_DISPATCH_CONFIG, || async move {
        dispatch_due_with(db, runtime_config, mail_sender).await
    })
    .await
}

async fn deliver_one(
    runtime_config: &RuntimeConfig,
    mail_sender: &Arc<dyn MailSender>,
    row: &aster_forge_db::mail_outbox::Model,
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

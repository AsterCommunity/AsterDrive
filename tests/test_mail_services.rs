mod common;

use std::any::Any;
use std::sync::{Arc, Mutex};

use aster_drive::config::{audit, mail, site_url};
use aster_drive::entities::audit_log;
use aster_drive::errors::{AsterError, Result};
use aster_drive::runtime::SharedRuntimeState;
use aster_drive::services::{
    mail::outbox,
    mail::sender::{self, MailMessage, MailSender},
    mail::template::RenderedMail,
};
use aster_drive::types::AuditAction;
use aster_forge_db::mail_outbox;
use aster_forge_mail::{MailOutboxStatus, MailTemplateCode, StoredMailPayload};
use async_trait::async_trait;
use chrono::{Duration, Utc};
use sea_orm::{ActiveModelTrait, EntityTrait, Set};

#[derive(Default)]
struct FailingMailSender {
    attempts: Mutex<usize>,
    message: String,
}

impl FailingMailSender {
    fn new(message: impl Into<String>) -> Arc<Self> {
        Arc::new(Self {
            attempts: Mutex::new(0),
            message: message.into(),
        })
    }

    fn attempts(&self) -> usize {
        *self.attempts.lock().expect("attempt counter lock")
    }
}

#[async_trait]
impl MailSender for FailingMailSender {
    async fn send(&self, _message: MailMessage) -> Result<()> {
        *self.attempts.lock().expect("attempt counter lock") += 1;
        Err(AsterError::mail_delivery_failed(self.message.clone()))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

fn apply_mail_config(state: &aster_drive::runtime::PrimaryAppState) {
    state.runtime_config.apply(common::system_config_model(
        mail::MAIL_FROM_ADDRESS_KEY,
        "noreply@example.com",
    ));
    state.runtime_config.apply(common::system_config_model(
        mail::MAIL_FROM_NAME_KEY,
        "Aster Test",
    ));
    state.runtime_config.apply(common::system_config_model(
        site_url::PUBLIC_SITE_URL_KEY,
        r#"["https://drive.example.com"]"#,
    ));
}

fn outbox_model(
    status: MailOutboxStatus,
    attempt_count: i32,
    next_attempt_at: chrono::DateTime<Utc>,
    payload_json: StoredMailPayload,
) -> mail_outbox::ActiveModel {
    let now = Utc::now();
    mail_outbox::ActiveModel {
        template_code: Set(MailTemplateCode::RegisterActivation),
        to_address: Set("user@example.com".to_string()),
        to_name: Set(Some("User".to_string())),
        payload_json: Set(payload_json),
        status: Set(status),
        attempt_count: Set(attempt_count),
        next_attempt_at: Set(next_attempt_at),
        processing_started_at: Set(None),
        sent_at: Set(None),
        last_error: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
}

async fn find_outbox_row(db: &sea_orm::DatabaseConnection, id: i64) -> mail_outbox::Model {
    mail_outbox::Entity::find_by_id(id)
        .one(db)
        .await
        .expect("mail outbox lookup should succeed")
        .expect("mail outbox row should exist")
}

async fn insert_outbox(
    db: &sea_orm::DatabaseConnection,
    model: mail_outbox::ActiveModel,
) -> std::result::Result<mail_outbox::Model, sea_orm::DbErr> {
    model.insert(db).await
}

async fn latest_mail_audit_entry(
    db: &sea_orm::DatabaseConnection,
    action: AuditAction,
) -> audit_log::Model {
    use sea_orm::{ColumnTrait, QueryFilter, QueryOrder};

    audit_log::Entity::find()
        .filter(audit_log::Column::Action.eq(action))
        .order_by_desc(audit_log::Column::Id)
        .one(db)
        .await
        .expect("mail audit lookup should succeed")
        .expect("mail audit entry should exist")
}

async fn mail_audit_count(db: &sea_orm::DatabaseConnection, action: AuditAction) -> u64 {
    use sea_orm::{ColumnTrait, PaginatorTrait, QueryFilter};

    audit_log::Entity::find()
        .filter(audit_log::Column::Action.eq(action))
        .count(db)
        .await
        .expect("mail audit count should succeed")
}

#[tokio::test]
async fn test_memory_sender_records_messages_and_send_rendered_uses_runtime_from_fields() {
    let state = common::setup().await;
    apply_mail_config(&state);
    let sender = sender::memory_sender();

    sender::send_rendered_with(
        &state.runtime_config,
        &sender,
        sender::MailRecipient {
            address: "target@example.com".to_string(),
            display_name: Some("Target User".to_string()),
        },
        RenderedMail {
            subject: "Subject".to_string(),
            text_body: "plain".to_string(),
            html_body: "<p>plain</p>".to_string(),
        },
    )
    .await
    .unwrap();

    let memory = sender::memory_sender_ref(&sender).unwrap();
    let messages = memory.messages();
    assert_eq!(messages.len(), 1);
    assert_eq!(memory.last_message(), messages.last().cloned());
    assert_eq!(messages[0].from.address, "noreply@example.com");
    assert_eq!(messages[0].from.display_name.as_deref(), Some("Aster Test"));
    assert_eq!(messages[0].to.address, "target@example.com");
    assert_eq!(messages[0].to.display_name.as_deref(), Some("Target User"));
    assert_eq!(messages[0].subject, "Subject");
}

#[tokio::test]
async fn test_send_rendered_state_wrapper_and_test_email_include_site_context() {
    let state = common::setup().await;
    apply_mail_config(&state);

    sender::send_rendered(
        &state,
        sender::MailRecipient {
            address: "first@example.com".to_string(),
            display_name: None,
        },
        RenderedMail {
            subject: "Wrapped".to_string(),
            text_body: "wrapped body".to_string(),
            html_body: "<p>wrapped body</p>".to_string(),
        },
    )
    .await
    .unwrap();
    sender::send_test_email(&state, "ops@example.com", Some("tester"))
        .await
        .unwrap();

    let memory = sender::memory_sender_ref(&state.mail_sender).unwrap();
    let messages = memory.messages();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].subject, "Wrapped");
    assert_eq!(messages[1].to.address, "ops@example.com");
    assert_eq!(messages[1].subject, "AsterDrive SMTP test");
    assert!(messages[1].text_body.contains("Triggered by: tester"));
    assert!(messages[1].text_body.contains("https://drive.example.com"));
    assert!(
        messages[1]
            .html_body
            .contains("<strong>Triggered by:</strong> tester")
    );
}

#[tokio::test]
async fn test_runtime_sender_rejects_missing_and_partial_smtp_configuration_before_network_io() {
    let state = common::setup().await;
    let sender = sender::runtime_sender(state.runtime_config.clone());
    let message = MailMessage {
        from: sender::MailRecipient {
            address: "noreply@example.com".to_string(),
            display_name: None,
        },
        to: sender::MailRecipient {
            address: "target@example.com".to_string(),
            display_name: None,
        },
        subject: "Subject".to_string(),
        text_body: "text".to_string(),
        html_body: "<p>text</p>".to_string(),
    };

    let error = sender.send(message.clone()).await.unwrap_err();
    assert_eq!(error.code(), "E008");
    assert!(error.message().contains("not configured"));

    state.runtime_config.apply(common::system_config_model(
        mail::MAIL_SMTP_HOST_KEY,
        "smtp.example.com",
    ));
    state.runtime_config.apply(common::system_config_model(
        mail::MAIL_FROM_ADDRESS_KEY,
        "noreply@example.com",
    ));
    state.runtime_config.apply(common::system_config_model(
        mail::MAIL_SMTP_USERNAME_KEY,
        "user",
    ));
    let error = sender.send(message).await.unwrap_err();
    assert_eq!(error.code(), "E008");
    assert!(error.message().contains("username and password"));
}

#[tokio::test]
async fn test_mail_outbox_dispatch_sends_due_message_and_clears_payload() {
    let state = common::setup().await;
    apply_mail_config(&state);
    let payload = aster_drive::services::mail::template::MailTemplatePayload::register_activation(
        "alice",
        "token-123",
        "AsterDrive",
    )
    .to_stored()
    .unwrap();
    let row = insert_outbox(
        state.writer_db(),
        outbox_model(MailOutboxStatus::Pending, 0, Utc::now(), payload),
    )
    .await
    .unwrap();

    let stats = outbox::dispatch_due(&state).await.unwrap();

    assert_eq!(stats.claimed, 1);
    assert_eq!(stats.sent, 1);
    assert_eq!(stats.retried, 0);
    assert_eq!(stats.failed, 0);
    let stored = find_outbox_row(state.writer_db(), row.id).await;
    assert_eq!(stored.status, MailOutboxStatus::Sent);
    assert_eq!(
        stored.payload_json.as_ref(),
        StoredMailPayload::CLEARED_JSON
    );
    assert!(stored.sent_at.is_some());

    let memory = sender::memory_sender_ref(&state.mail_sender).unwrap();
    let message = memory.last_message().unwrap();
    assert_eq!(message.to.address, "user@example.com");
    assert_eq!(message.to.display_name.as_deref(), Some("User"));
    assert!(message.text_body.contains("alice"));
    assert!(message.text_body.contains("https://drive.example.com"));

    let audit_entry = latest_mail_audit_entry(state.writer_db(), AuditAction::MailSend).await;
    assert_eq!(audit_entry.user_id, 0);
    assert_eq!(audit_entry.entity_type, "mail");
    assert_eq!(audit_entry.entity_id, Some(row.id));
    assert_eq!(audit_entry.entity_name.as_deref(), Some("mail"));
    let details: serde_json::Value =
        serde_json::from_str(audit_entry.details.as_deref().unwrap()).unwrap();
    assert_eq!(details["to_address"], "user@example.com");
    assert_eq!(details["template_code"], "register_activation");
    assert_eq!(details["to_name"], "User");
    assert_eq!(details["outbox_id"], row.id);
    assert_eq!(details["attempt_count"], 1);
}

#[tokio::test]
async fn test_mail_outbox_dispatch_skips_future_retry_rows() {
    let state = common::setup().await;
    apply_mail_config(&state);
    let payload = aster_drive::services::mail::template::MailTemplatePayload::register_activation(
        "alice",
        "token-123",
        "AsterDrive",
    )
    .to_stored()
    .unwrap();
    insert_outbox(
        state.writer_db(),
        outbox_model(
            MailOutboxStatus::Retry,
            1,
            Utc::now() + Duration::minutes(10),
            payload,
        ),
    )
    .await
    .unwrap();

    let stats = outbox::dispatch_due(&state).await.unwrap();

    assert_eq!(stats.claimed, 0);
    assert_eq!(stats.sent, 0);
    assert!(
        sender::memory_sender_ref(&state.mail_sender)
            .unwrap()
            .messages()
            .is_empty()
    );
}

#[tokio::test]
async fn test_mail_outbox_dispatch_retries_failed_delivery_with_truncated_error() {
    let state = common::setup().await;
    apply_mail_config(&state);
    let payload = aster_drive::services::mail::template::MailTemplatePayload::register_activation(
        "alice",
        "token-123",
        "AsterDrive",
    )
    .to_stored()
    .unwrap();
    let row = insert_outbox(
        state.writer_db(),
        outbox_model(MailOutboxStatus::Pending, 0, Utc::now(), payload),
    )
    .await
    .unwrap();
    let failing = FailingMailSender::new("x".repeat(1_200));
    let sender: Arc<dyn MailSender> = failing.clone();

    let stats = outbox::dispatch_due_with(state.writer_db(), &state.runtime_config, &sender)
        .await
        .unwrap();

    assert_eq!(stats.claimed, 1);
    assert_eq!(stats.retried, 1);
    assert_eq!(stats.failed, 0);
    assert_eq!(failing.attempts(), 1);
    let stored = find_outbox_row(state.writer_db(), row.id).await;
    assert_eq!(stored.status, MailOutboxStatus::Retry);
    assert_eq!(stored.attempt_count, 1);
    assert!(stored.next_attempt_at > Utc::now());
    assert_eq!(stored.last_error.as_deref().unwrap().chars().count(), 1024);
    assert_eq!(
        mail_audit_count(state.writer_db(), AuditAction::MailDeliveryFailed).await,
        0
    );
    assert_eq!(
        mail_audit_count(state.writer_db(), AuditAction::MailSend).await,
        0
    );
}

#[tokio::test]
async fn test_mail_outbox_dispatch_success_after_retries_records_current_attempt() {
    let state = common::setup().await;
    apply_mail_config(&state);
    let payload = aster_drive::services::mail::template::MailTemplatePayload::register_activation(
        "alice",
        "token-123",
        "AsterDrive",
    )
    .to_stored()
    .unwrap();
    let row = insert_outbox(
        state.writer_db(),
        outbox_model(MailOutboxStatus::Retry, 2, Utc::now(), payload),
    )
    .await
    .unwrap();

    let stats = outbox::dispatch_due(&state).await.unwrap();

    assert_eq!(stats.claimed, 1);
    assert_eq!(stats.sent, 1);
    let audit_entry = latest_mail_audit_entry(state.writer_db(), AuditAction::MailSend).await;
    assert_eq!(audit_entry.entity_id, Some(row.id));
    let details: serde_json::Value =
        serde_json::from_str(audit_entry.details.as_deref().unwrap()).unwrap();
    assert_eq!(details["attempt_count"], 3);
}

#[tokio::test]
async fn test_mail_outbox_dispatch_respects_mail_audit_action_scope() {
    let state = common::setup().await;
    apply_mail_config(&state);
    state.runtime_config.apply(common::system_config_model(
        audit::AUDIT_LOG_RECORDED_ACTIONS_KEY,
        r#"["mail_delivery_failed"]"#,
    ));
    let payload = aster_drive::services::mail::template::MailTemplatePayload::register_activation(
        "alice",
        "token-123",
        "AsterDrive",
    )
    .to_stored()
    .unwrap();
    insert_outbox(
        state.writer_db(),
        outbox_model(MailOutboxStatus::Pending, 0, Utc::now(), payload),
    )
    .await
    .unwrap();

    let stats = outbox::dispatch_due(&state).await.unwrap();

    assert_eq!(stats.sent, 1);
    assert_eq!(
        mail_audit_count(state.writer_db(), AuditAction::MailSend).await,
        0
    );
}

#[tokio::test]
async fn test_mail_outbox_dispatch_marks_final_failure_and_clears_payload() {
    let state = common::setup().await;
    apply_mail_config(&state);
    let payload = aster_drive::services::mail::template::MailTemplatePayload::register_activation(
        "alice",
        "token-123",
        "AsterDrive",
    )
    .to_stored()
    .unwrap();
    let row = insert_outbox(
        state.writer_db(),
        outbox_model(MailOutboxStatus::Pending, 5, Utc::now(), payload),
    )
    .await
    .unwrap();
    let failing = FailingMailSender::new("smtp unavailable");
    let sender: Arc<dyn MailSender> = failing.clone();

    let stats = outbox::dispatch_due_with(state.writer_db(), &state.runtime_config, &sender)
        .await
        .unwrap();

    assert_eq!(stats.claimed, 1);
    assert_eq!(stats.retried, 0);
    assert_eq!(stats.failed, 1);
    assert_eq!(failing.attempts(), 1);
    let stored = find_outbox_row(state.writer_db(), row.id).await;
    assert_eq!(stored.status, MailOutboxStatus::Failed);
    assert_eq!(stored.attempt_count, 6);
    assert_eq!(
        stored.payload_json.as_ref(),
        StoredMailPayload::CLEARED_JSON
    );
    assert_eq!(
        stored.last_error.as_deref(),
        Some("Mail Delivery Failed: smtp unavailable")
    );

    let audit_entry =
        latest_mail_audit_entry(state.writer_db(), AuditAction::MailDeliveryFailed).await;
    assert_eq!(audit_entry.user_id, 0);
    assert_eq!(audit_entry.entity_type, "mail");
    assert_eq!(audit_entry.entity_id, Some(row.id));
    let details: serde_json::Value =
        serde_json::from_str(audit_entry.details.as_deref().unwrap()).unwrap();
    assert_eq!(details["to_address"], "user@example.com");
    assert_eq!(details["template_code"], "register_activation");
    assert_eq!(details["attempt_count"], 6);
    assert_eq!(details["error"], "Mail Delivery Failed: smtp unavailable");
}

#[tokio::test]
async fn test_mail_outbox_final_failure_audit_error_is_truncated() {
    let state = common::setup().await;
    apply_mail_config(&state);
    let payload = aster_drive::services::mail::template::MailTemplatePayload::register_activation(
        "alice",
        "token-123",
        "AsterDrive",
    )
    .to_stored()
    .unwrap();
    let row = insert_outbox(
        state.writer_db(),
        outbox_model(MailOutboxStatus::Pending, 5, Utc::now(), payload),
    )
    .await
    .unwrap();
    let failing = FailingMailSender::new("x".repeat(1_200));
    let sender: Arc<dyn MailSender> = failing;

    let stats = outbox::dispatch_due_with(state.writer_db(), &state.runtime_config, &sender)
        .await
        .unwrap();

    assert_eq!(stats.failed, 1);
    let stored = find_outbox_row(state.writer_db(), row.id).await;
    assert_eq!(stored.last_error.as_deref().unwrap().chars().count(), 1024);

    let audit_entry =
        latest_mail_audit_entry(state.writer_db(), AuditAction::MailDeliveryFailed).await;
    let details: serde_json::Value =
        serde_json::from_str(audit_entry.details.as_deref().unwrap()).unwrap();
    assert_eq!(details["error"].as_str().unwrap().chars().count(), 1024);
}

#[tokio::test]
async fn test_mail_outbox_dispatch_reclaims_stale_processing_rows_and_drain_merges_stats() {
    let state = common::setup().await;
    apply_mail_config(&state);
    let payload = aster_drive::services::mail::template::MailTemplatePayload::register_activation(
        "alice",
        "token-123",
        "AsterDrive",
    )
    .to_stored()
    .unwrap();
    let mut model = outbox_model(MailOutboxStatus::Processing, 0, Utc::now(), payload.clone());
    model.processing_started_at = Set(Some(Utc::now() - Duration::seconds(120)));
    let stale = insert_outbox(state.writer_db(), model).await.unwrap();
    insert_outbox(
        state.writer_db(),
        outbox_model(MailOutboxStatus::Pending, 0, Utc::now(), payload),
    )
    .await
    .unwrap();

    let stats = outbox::drain(&state).await.unwrap();

    assert_eq!(stats.claimed, 2);
    assert_eq!(stats.sent, 2);
    assert_eq!(stats.retried, 0);
    assert_eq!(stats.failed, 0);
    assert_eq!(
        find_outbox_row(state.writer_db(), stale.id).await.status,
        MailOutboxStatus::Sent
    );
    assert_eq!(
        aster_forge_db::MailOutboxDbStore::new(state.writer_db().clone())
            .count_active()
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn test_mail_outbox_dispatch_invalid_payload_schedules_retry_without_sending() {
    let state = common::setup().await;
    apply_mail_config(&state);
    let row = insert_outbox(
        state.writer_db(),
        outbox_model(
            MailOutboxStatus::Pending,
            0,
            Utc::now(),
            StoredMailPayload("{\"bad\":true}".to_string()),
        ),
    )
    .await
    .unwrap();

    let stats = outbox::dispatch_due(&state).await.unwrap();

    assert_eq!(stats.claimed, 1);
    assert_eq!(stats.retried, 1);
    assert!(
        sender::memory_sender_ref(&state.mail_sender)
            .unwrap()
            .messages()
            .is_empty()
    );
    let stored = find_outbox_row(state.writer_db(), row.id).await;
    assert_eq!(stored.status, MailOutboxStatus::Retry);
    assert!(stored.last_error.unwrap().contains("failed to decode"));
}

#[tokio::test]
async fn test_mail_outbox_dispatch_does_not_reclaim_fresh_processing_rows() {
    let state = common::setup().await;
    apply_mail_config(&state);
    let payload = aster_drive::services::mail::template::MailTemplatePayload::register_activation(
        "alice",
        "token-123",
        "AsterDrive",
    )
    .to_stored()
    .unwrap();
    let mut model = outbox_model(MailOutboxStatus::Processing, 0, Utc::now(), payload);
    model.processing_started_at = Set(Some(Utc::now()));
    insert_outbox(state.writer_db(), model).await.unwrap();

    let stats = outbox::dispatch_due(&state).await.unwrap();

    assert_eq!(stats.claimed, 0);
    assert!(
        sender::memory_sender_ref(&state.mail_sender)
            .unwrap()
            .messages()
            .is_empty()
    );
}

#[tokio::test]
async fn test_mail_outbox_sent_and_failed_statuses_are_terminal() {
    assert!(MailOutboxStatus::Sent.is_terminal());
    assert!(MailOutboxStatus::Failed.is_terminal());
    assert!(!MailOutboxStatus::Pending.is_terminal());
    assert!(!MailOutboxStatus::Retry.is_terminal());
    assert!(!MailOutboxStatus::Processing.is_terminal());
}

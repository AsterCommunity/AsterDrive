use sea_orm::DatabaseConnection;

use crate::config::RuntimeConfig;
use crate::runtime::SharedRuntimeState;
use crate::services::audit_service::{self, AuditContext};

const MAIL_ENTITY_NAME: &str = "mail";

#[derive(Debug, Clone, Copy)]
pub struct MailAuditInput<'a> {
    pub actor_user_id: i64,
    pub to_address: &'a str,
    pub to_name: Option<&'a str>,
    pub template_code: &'a str,
    pub subject: Option<&'a str>,
    pub outbox_id: Option<i64>,
    pub attempt_count: Option<i32>,
    pub error: Option<&'a str>,
}

pub async fn log_send(state: &impl SharedRuntimeState, input: MailAuditInput<'_>) {
    let ctx = AuditContext {
        user_id: input.actor_user_id,
        ip_address: None,
        user_agent: None,
    };
    audit_service::log_with_details(
        state,
        &ctx,
        audit_service::AuditAction::MailSend,
        audit_service::AuditEntityType::Mail,
        input.outbox_id,
        Some(MAIL_ENTITY_NAME),
        || mail_details(input),
    )
    .await;
}

pub async fn log_send_with_db(
    db: &DatabaseConnection,
    runtime_config: &RuntimeConfig,
    input: MailAuditInput<'_>,
) {
    let ctx = AuditContext {
        user_id: input.actor_user_id,
        ip_address: None,
        user_agent: None,
    };
    audit_service::log_with_db_and_config(
        db,
        runtime_config,
        audit_service::AuditLogInput {
            ctx: &ctx,
            action: audit_service::AuditAction::MailSend,
            entity_type: audit_service::AuditEntityType::Mail,
            entity_id: input.outbox_id,
            entity_name: Some(MAIL_ENTITY_NAME),
        },
        || mail_details(input),
    )
    .await;
}

pub async fn log_delivery_failed_with_db(
    db: &DatabaseConnection,
    runtime_config: &RuntimeConfig,
    input: MailAuditInput<'_>,
) {
    let ctx = AuditContext {
        user_id: input.actor_user_id,
        ip_address: None,
        user_agent: None,
    };
    audit_service::log_with_db_and_config(
        db,
        runtime_config,
        audit_service::AuditLogInput {
            ctx: &ctx,
            action: audit_service::AuditAction::MailDeliveryFailed,
            entity_type: audit_service::AuditEntityType::Mail,
            entity_id: input.outbox_id,
            entity_name: Some(MAIL_ENTITY_NAME),
        },
        || mail_details(input),
    )
    .await;
}

fn mail_details(input: MailAuditInput<'_>) -> Option<serde_json::Value> {
    audit_service::details(audit_service::MailAuditDetails {
        to_address: input.to_address,
        template_code: input.template_code,
        to_name: input.to_name,
        subject: input.subject,
        outbox_id: input.outbox_id,
        attempt_count: input.attempt_count,
        error: input.error,
    })
}

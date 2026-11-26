//! 邮件发送与投递失败的审计日志记录。

use sea_orm::DatabaseConnection;

use crate::config::RuntimeConfig;
use crate::runtime::SharedRuntimeState;
use crate::services::ops::audit::{self, AuditContext};

const MAIL_ENTITY_NAME: &str = "mail";
const MAX_AUDIT_FIELD_LEN: usize = 1024;

/// Input snapshot for one mail audit entry.
///
/// `outbox_id` becomes the audit entity ID and `to_address`, `template_code`
/// and the remaining fields are stored as audit details, with `to_name`,
/// `subject` and `error` truncated to 1024 characters each. Outbox dispatch
/// records actor user ID `0` because no user action triggered the send.
#[derive(Debug, Clone, Copy)]
pub struct MailAuditInput<'a> {
    /// User ID attributed with the mail action; `0` for system-driven sends.
    pub actor_user_id: i64,
    /// Actor IP address carried into the audit context, when known.
    pub ip_address: Option<&'a str>,
    /// Actor user agent carried into the audit context, when known.
    pub user_agent: Option<&'a str>,
    /// Recipient email address the mail was sent to.
    pub to_address: &'a str,
    /// Recipient display name, when available.
    pub to_name: Option<&'a str>,
    /// Mail template code that produced the message.
    pub template_code: &'a str,
    /// Rendered subject line; `None` when delivery failed before it was known.
    pub subject: Option<&'a str>,
    /// Outbox row ID the mail was dispatched from, when it went through the queue.
    pub outbox_id: Option<i64>,
    /// Delivery attempt number for outbox-driven sends, when reported.
    pub attempt_count: Option<i32>,
    /// Delivery failure error message; only set for failure audits.
    pub error: Option<&'a str>,
}

/// Records a `MailSend` audit entry through the shared runtime state.
///
/// Used for direct sends triggered by a user action, such as MFA login codes
/// or admin test emails.
pub async fn log_send(state: &impl SharedRuntimeState, input: MailAuditInput<'_>) {
    let ctx = AuditContext {
        user_id: input.actor_user_id,
        ip_address: input.ip_address.map(str::to_string),
        user_agent: input.user_agent.map(str::to_string),
    };
    audit::log_with_details(
        state,
        &ctx,
        audit::AuditAction::MailSend,
        audit::AuditEntityType::Mail,
        input.outbox_id,
        Some(MAIL_ENTITY_NAME),
        || mail_details(input),
    )
    .await;
}

/// Records a `MailSend` audit entry on an explicit database connection.
///
/// Outbox dispatch uses this variant so the audit row is written with the
/// same connection and config the delivery ran against.
pub async fn log_send_with_db(
    db: &DatabaseConnection,
    runtime_config: &RuntimeConfig,
    input: MailAuditInput<'_>,
) {
    let ctx = AuditContext {
        user_id: input.actor_user_id,
        ip_address: input.ip_address.map(str::to_string),
        user_agent: input.user_agent.map(str::to_string),
    };
    audit::log_with_db_and_config(
        db,
        runtime_config,
        audit::AuditLogInput {
            ctx: &ctx,
            action: audit::AuditAction::MailSend,
            entity_type: audit::AuditEntityType::Mail,
            entity_id: input.outbox_id,
            entity_name: Some(MAIL_ENTITY_NAME),
        },
        || mail_details(input),
    )
    .await;
}

/// Records a `MailDeliveryFailed` audit entry on an explicit database connection.
///
/// Called by outbox dispatch after a delivery attempt exhausts its retry
/// policy; `input.error` carries the truncated provider error message.
pub async fn log_delivery_failed_with_db(
    db: &DatabaseConnection,
    runtime_config: &RuntimeConfig,
    input: MailAuditInput<'_>,
) {
    let ctx = AuditContext {
        user_id: input.actor_user_id,
        ip_address: input.ip_address.map(str::to_string),
        user_agent: input.user_agent.map(str::to_string),
    };
    audit::log_with_db_and_config(
        db,
        runtime_config,
        audit::AuditLogInput {
            ctx: &ctx,
            action: audit::AuditAction::MailDeliveryFailed,
            entity_type: audit::AuditEntityType::Mail,
            entity_id: input.outbox_id,
            entity_name: Some(MAIL_ENTITY_NAME),
        },
        || mail_details(input),
    )
    .await;
}

fn mail_details(input: MailAuditInput<'_>) -> Option<serde_json::Value> {
    let to_name = truncate_audit_field(input.to_name);
    let subject = truncate_audit_field(input.subject);
    let error = truncate_audit_field(input.error);

    audit::details(audit::MailAuditDetails {
        to_address: input.to_address,
        template_code: input.template_code,
        to_name: to_name.as_deref(),
        subject: subject.as_deref(),
        outbox_id: input.outbox_id,
        attempt_count: input.attempt_count,
        error: error.as_deref(),
    })
}

fn truncate_audit_field(value: Option<&str>) -> Option<String> {
    value.map(|raw| raw.chars().take(MAX_AUDIT_FIELD_LEN).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mail_details_truncates_variable_fields_without_splitting_utf8() {
        let long_value = "猫".repeat(MAX_AUDIT_FIELD_LEN + 1);
        let details = mail_details(MailAuditInput {
            actor_user_id: 1,
            ip_address: None,
            user_agent: None,
            to_address: "alice@example.com",
            to_name: Some(&long_value),
            template_code: "smtp_test",
            subject: Some(&long_value),
            outbox_id: None,
            attempt_count: None,
            error: Some(&long_value),
        })
        .expect("mail audit details should serialize");

        assert_eq!(
            details["to_name"]
                .as_str()
                .expect("to_name should be string")
                .chars()
                .count(),
            MAX_AUDIT_FIELD_LEN
        );
        assert_eq!(
            details["subject"]
                .as_str()
                .expect("subject should be string")
                .chars()
                .count(),
            MAX_AUDIT_FIELD_LEN
        );
        assert_eq!(
            details["error"]
                .as_str()
                .expect("error should be string")
                .chars()
                .count(),
            MAX_AUDIT_FIELD_LEN
        );
    }
}

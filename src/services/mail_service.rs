//! 服务模块：`mail_service`。

use std::any::Any;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use lettre::message::{Mailbox, MultiPart, SinglePart, header::ContentType};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use tokio::time::timeout;

use crate::config::RuntimeConfig;
use crate::config::{mail, site_url};
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::MailRuntimeState;
use crate::services::mail_template::RenderedMail;
use crate::utils::id;

const SMTP_SEND_TIMEOUT_SECS: u64 = 15;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailRecipient {
    pub address: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailMessage {
    pub from: MailRecipient,
    pub to: MailRecipient,
    pub subject: String,
    pub text_body: String,
    pub html_body: String,
}

#[async_trait]
pub trait MailSender: Send + Sync {
    async fn send(&self, message: MailMessage) -> Result<()>;
    fn as_any(&self) -> &dyn Any;
}

pub fn runtime_sender(runtime_config: Arc<RuntimeConfig>) -> Arc<dyn MailSender> {
    Arc::new(RuntimeMailSender { runtime_config })
}

pub fn memory_sender() -> Arc<dyn MailSender> {
    Arc::new(MemoryMailSender::default())
}

pub fn memory_sender_ref(sender: &Arc<dyn MailSender>) -> Option<&MemoryMailSender> {
    sender.as_ref().as_any().downcast_ref::<MemoryMailSender>()
}

#[derive(Default)]
pub struct MemoryMailSender {
    outbox: Mutex<Vec<MailMessage>>,
}

impl MemoryMailSender {
    pub fn messages(&self) -> Vec<MailMessage> {
        self.outbox
            .lock()
            .expect("memory mail sender poisoned")
            .clone()
    }

    pub fn last_message(&self) -> Option<MailMessage> {
        self.outbox
            .lock()
            .expect("memory mail sender poisoned")
            .last()
            .cloned()
    }
}

#[async_trait]
impl MailSender for MemoryMailSender {
    async fn send(&self, message: MailMessage) -> Result<()> {
        self.outbox
            .lock()
            .expect("memory mail sender poisoned")
            .push(message);
        Ok(())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

struct RuntimeMailSender {
    runtime_config: Arc<RuntimeConfig>,
}

#[async_trait]
impl MailSender for RuntimeMailSender {
    async fn send(&self, message: MailMessage) -> Result<()> {
        let settings = mail::RuntimeMailSettings::from_runtime_config(&self.runtime_config);
        if !settings.is_configured() {
            return Err(AsterError::mail_not_configured(
                "mail service is not configured",
            ));
        }
        if !settings.is_ready_for_delivery() {
            return Err(AsterError::mail_not_configured(
                "mail SMTP username and password must both be set or both be empty",
            ));
        }

        let to_address = message.to.address.clone();
        let subject = message.subject.clone();
        tracing::debug!(
            smtp_host = %settings.smtp_host,
            smtp_port = settings.smtp_port,
            encryption_enabled = settings.encryption_enabled,
            to = %to_address,
            subject = %subject,
            timeout_secs = SMTP_SEND_TIMEOUT_SECS,
            "mail: preparing runtime SMTP delivery"
        );

        let email = build_lettre_message(message)?;
        let mailer = build_transport(&settings)?;
        match timeout(
            Duration::from_secs(SMTP_SEND_TIMEOUT_SECS),
            mailer.send(email),
        )
        .await
        {
            Ok(Ok(_)) => {
                tracing::debug!(
                    smtp_host = %settings.smtp_host,
                    smtp_port = settings.smtp_port,
                    to = %to_address,
                    subject = %subject,
                    timeout_secs = SMTP_SEND_TIMEOUT_SECS,
                    "mail: SMTP delivery completed"
                );
                Ok(())
            }
            Ok(Err(error)) => {
                tracing::debug!(
                    smtp_host = %settings.smtp_host,
                    smtp_port = settings.smtp_port,
                    to = %to_address,
                    subject = %subject,
                    error = %error,
                    timeout_secs = SMTP_SEND_TIMEOUT_SECS,
                    "mail: SMTP delivery failed"
                );
                Err(AsterError::mail_delivery_failed(error.to_string()))
            }
            Err(_) => {
                tracing::debug!(
                    smtp_host = %settings.smtp_host,
                    smtp_port = settings.smtp_port,
                    to = %to_address,
                    subject = %subject,
                    timeout_secs = SMTP_SEND_TIMEOUT_SECS,
                    "mail: SMTP delivery timed out"
                );
                Err(AsterError::mail_delivery_failed(format!(
                    "mail delivery timed out after {} seconds",
                    SMTP_SEND_TIMEOUT_SECS
                )))
            }
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub async fn send_rendered(
    state: &impl MailRuntimeState,
    to: MailRecipient,
    rendered: RenderedMail,
) -> Result<()> {
    send_rendered_with(state.runtime_config(), state.mail_sender(), to, rendered).await
}

pub async fn send_rendered_with(
    runtime_config: &RuntimeConfig,
    mail_sender: &Arc<dyn MailSender>,
    to: MailRecipient,
    rendered: RenderedMail,
) -> Result<()> {
    let settings = mail::RuntimeMailSettings::from_runtime_config(runtime_config);
    let from = MailRecipient {
        address: settings.from_address,
        display_name: (!settings.from_name.is_empty()).then_some(settings.from_name),
    };
    tracing::debug!(
        from = %from.address,
        to = %to.address,
        subject = %rendered.subject,
        "mail: dispatching rendered message through configured sender"
    );

    mail_sender
        .send(MailMessage {
            from,
            to,
            subject: rendered.subject,
            text_body: rendered.text_body,
            html_body: rendered.html_body,
        })
        .await
}

pub async fn send_test_email(
    state: &impl MailRuntimeState,
    email: &str,
    triggered_by: Option<&str>,
) -> Result<()> {
    let timestamp = Utc::now().to_rfc3339();
    let site_url = site_url::public_site_url(state.runtime_config())
        .unwrap_or_else(|| "(not configured)".to_string());
    let triggered_by = triggered_by.unwrap_or("admin");
    tracing::debug!(
        to = %email,
        triggered_by = %triggered_by,
        "mail: building test email"
    );

    send_rendered(
        state,
        MailRecipient {
            address: email.to_string(),
            display_name: None,
        },
        RenderedMail {
            subject: "AsterDrive SMTP test".to_string(),
            text_body: format!(
                "This is a test email from AsterDrive.\n\nTriggered by: {triggered_by}\nSent at (UTC): {timestamp}\nPublic site URL: {site_url}\n\nIf you received this email, your SMTP settings are working."
            ),
            html_body: format!(
                "<p>This is a test email from AsterDrive.</p><p><strong>Triggered by:</strong> {triggered_by}<br /><strong>Sent at (UTC):</strong> {timestamp}<br /><strong>Public site URL:</strong> {site_url}</p><p>If you received this email, your SMTP settings are working.</p>"
            ),
        },
    )
    .await
}

pub fn build_verification_token() -> String {
    format!("cv_{}", id::new_short_token())
}

fn build_transport(
    settings: &mail::RuntimeMailSettings,
) -> Result<AsyncSmtpTransport<Tokio1Executor>> {
    tracing::debug!(
        smtp_host = %settings.smtp_host,
        smtp_port = settings.smtp_port,
        encryption_enabled = settings.encryption_enabled,
        auth_enabled = !settings.smtp_username.is_empty(),
        "mail: building SMTP transport"
    );
    let mut transport = if settings.encryption_enabled {
        if settings.smtp_port == 465 {
            AsyncSmtpTransport::<Tokio1Executor>::relay(&settings.smtp_host)
                .map_aster_err(AsterError::config_error)?
                .port(settings.smtp_port)
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&settings.smtp_host)
                .map_aster_err(AsterError::config_error)?
                .port(settings.smtp_port)
        }
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&settings.smtp_host)
            .port(settings.smtp_port)
    };

    if !settings.smtp_username.is_empty() {
        transport = transport.credentials(Credentials::new(
            settings.smtp_username.clone(),
            settings.smtp_password.clone(),
        ));
    }

    Ok(transport.build())
}

fn build_lettre_message(message: MailMessage) -> Result<Message> {
    let from = mailbox(message.from)?;
    let to = mailbox(message.to)?;

    Message::builder()
        .from(from)
        .to(to)
        .subject(message.subject)
        .multipart(
            MultiPart::alternative()
                .singlepart(SinglePart::plain(message.text_body))
                .singlepart(
                    SinglePart::builder()
                        .header(ContentType::TEXT_HTML)
                        .body(message.html_body),
                ),
        )
        .map_aster_err(AsterError::config_error)
}

fn mailbox(recipient: MailRecipient) -> Result<Mailbox> {
    let address = recipient
        .address
        .parse()
        .map_aster_err(AsterError::validation_error)?;
    Ok(Mailbox::new(recipient.display_name, address))
}

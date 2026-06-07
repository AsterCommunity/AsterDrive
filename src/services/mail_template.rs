//! 服务模块：`mail_template`。

use serde::{Deserialize, Serialize, de::DeserializeOwned};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::config::{RuntimeConfig, branding, mail, site_url};
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::types::{MailTemplateCode, StoredMailPayload};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedMail {
    pub subject: String,
    pub text_body: String,
    pub html_body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct TemplateVariableItem {
    pub token: String,
    pub label_i18n_key: String,
    pub description_i18n_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct TemplateVariableGroup {
    pub category: String,
    pub template_code: String,
    pub label_i18n_key: String,
    pub variables: Vec<TemplateVariableItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisterActivationPayload {
    pub username: String,
    pub token: String,
    #[serde(default = "default_site_name")]
    pub site_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContactChangeConfirmationPayload {
    pub username: String,
    pub token: String,
    #[serde(default = "default_site_name")]
    pub site_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PasswordResetPayload {
    pub username: String,
    pub token: String,
    #[serde(default = "default_site_name")]
    pub site_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PasswordResetNoticePayload {
    pub username: String,
    #[serde(default = "default_site_name")]
    pub site_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContactChangeNoticePayload {
    pub username: String,
    pub previous_email: String,
    pub new_email: String,
    #[serde(default = "default_site_name")]
    pub site_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalAuthEmailVerificationPayload {
    pub email: String,
    pub token: String,
    #[serde(default = "default_external_auth_provider_name")]
    pub provider_name: String,
    #[serde(default = "default_site_name")]
    pub site_name: String,
    #[serde(default = "default_external_auth_expires_in")]
    pub expires_in: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoginEmailCodePayload {
    pub username: String,
    pub code: String,
    #[serde(default = "default_site_name")]
    pub site_name: String,
    #[serde(default = "default_login_email_code_expires_in")]
    pub expires_in: String,
    #[serde(default = "default_mail_template_lang")]
    pub lang: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserInvitationPayload {
    pub email: String,
    pub invitation_url: String,
    #[serde(default = "default_site_name")]
    pub site_name: String,
    #[serde(default = "default_user_invitation_expires_in")]
    pub expires_in: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MailTemplatePayload {
    RegisterActivation(RegisterActivationPayload),
    ContactChangeConfirmation(ContactChangeConfirmationPayload),
    PasswordReset(PasswordResetPayload),
    PasswordResetNotice(PasswordResetNoticePayload),
    ContactChangeNotice(ContactChangeNoticePayload),
    ExternalAuthEmailVerification(ExternalAuthEmailVerificationPayload),
    LoginEmailCode(LoginEmailCodePayload),
    UserInvitation(UserInvitationPayload),
}

impl MailTemplatePayload {
    pub fn register_activation(username: &str, token: &str, site_name: &str) -> Self {
        Self::RegisterActivation(RegisterActivationPayload {
            username: username.to_string(),
            token: token.to_string(),
            site_name: site_name.to_string(),
        })
    }

    pub fn contact_change_confirmation(username: &str, token: &str, site_name: &str) -> Self {
        Self::ContactChangeConfirmation(ContactChangeConfirmationPayload {
            username: username.to_string(),
            token: token.to_string(),
            site_name: site_name.to_string(),
        })
    }

    pub fn password_reset(username: &str, token: &str, site_name: &str) -> Self {
        Self::PasswordReset(PasswordResetPayload {
            username: username.to_string(),
            token: token.to_string(),
            site_name: site_name.to_string(),
        })
    }

    pub fn password_reset_notice(username: &str, site_name: &str) -> Self {
        Self::PasswordResetNotice(PasswordResetNoticePayload {
            username: username.to_string(),
            site_name: site_name.to_string(),
        })
    }

    pub fn contact_change_notice(
        username: &str,
        previous_email: &str,
        new_email: &str,
        site_name: &str,
    ) -> Self {
        Self::ContactChangeNotice(ContactChangeNoticePayload {
            username: username.to_string(),
            previous_email: previous_email.to_string(),
            new_email: new_email.to_string(),
            site_name: site_name.to_string(),
        })
    }

    pub fn external_auth_email_verification(
        email: &str,
        token: &str,
        provider_name: &str,
        site_name: &str,
        expires_in: &str,
    ) -> Self {
        Self::ExternalAuthEmailVerification(ExternalAuthEmailVerificationPayload {
            email: email.to_string(),
            token: token.to_string(),
            provider_name: provider_name.to_string(),
            site_name: site_name.to_string(),
            expires_in: expires_in.to_string(),
        })
    }

    pub fn login_email_code(username: &str, code: &str, site_name: &str, expires_in: &str) -> Self {
        Self::LoginEmailCode(LoginEmailCodePayload {
            username: username.to_string(),
            code: code.to_string(),
            site_name: site_name.to_string(),
            expires_in: expires_in.to_string(),
            lang: default_mail_template_lang(),
        })
    }

    pub fn user_invitation(
        email: &str,
        invitation_url: &str,
        site_name: &str,
        expires_in: &str,
    ) -> Self {
        Self::UserInvitation(UserInvitationPayload {
            email: email.to_string(),
            invitation_url: invitation_url.to_string(),
            site_name: site_name.to_string(),
            expires_in: expires_in.to_string(),
        })
    }

    pub fn template_code(&self) -> MailTemplateCode {
        match self {
            Self::RegisterActivation(_) => MailTemplateCode::RegisterActivation,
            Self::ContactChangeConfirmation(_) => MailTemplateCode::ContactChangeConfirmation,
            Self::PasswordReset(_) => MailTemplateCode::PasswordReset,
            Self::PasswordResetNotice(_) => MailTemplateCode::PasswordResetNotice,
            Self::ContactChangeNotice(_) => MailTemplateCode::ContactChangeNotice,
            Self::ExternalAuthEmailVerification(_) => {
                MailTemplateCode::ExternalAuthEmailVerification
            }
            Self::LoginEmailCode(_) => MailTemplateCode::LoginEmailCode,
            Self::UserInvitation(_) => MailTemplateCode::UserInvitation,
        }
    }

    pub fn to_stored(&self) -> Result<StoredMailPayload> {
        match self {
            Self::RegisterActivation(payload) => serialize_payload(payload).map(StoredMailPayload),
            Self::ContactChangeConfirmation(payload) => {
                serialize_payload(payload).map(StoredMailPayload)
            }
            Self::PasswordReset(payload) => serialize_payload(payload).map(StoredMailPayload),
            Self::PasswordResetNotice(payload) => serialize_payload(payload).map(StoredMailPayload),
            Self::ContactChangeNotice(payload) => serialize_payload(payload).map(StoredMailPayload),
            Self::ExternalAuthEmailVerification(payload) => {
                serialize_payload(payload).map(StoredMailPayload)
            }
            Self::LoginEmailCode(payload) => serialize_payload(payload).map(StoredMailPayload),
            Self::UserInvitation(payload) => serialize_payload(payload).map(StoredMailPayload),
        }
    }

    pub fn from_stored(
        template_code: MailTemplateCode,
        payload: &StoredMailPayload,
    ) -> Result<Self> {
        match template_code {
            MailTemplateCode::RegisterActivation => Ok(Self::RegisterActivation(
                deserialize_payload(template_code, payload.as_ref())?,
            )),
            MailTemplateCode::ContactChangeConfirmation => Ok(Self::ContactChangeConfirmation(
                deserialize_payload(template_code, payload.as_ref())?,
            )),
            MailTemplateCode::PasswordReset => Ok(Self::PasswordReset(deserialize_payload(
                template_code,
                payload.as_ref(),
            )?)),
            MailTemplateCode::PasswordResetNotice => Ok(Self::PasswordResetNotice(
                deserialize_payload(template_code, payload.as_ref())?,
            )),
            MailTemplateCode::ContactChangeNotice => Ok(Self::ContactChangeNotice(
                deserialize_payload(template_code, payload.as_ref())?,
            )),
            MailTemplateCode::ExternalAuthEmailVerification => {
                Ok(Self::ExternalAuthEmailVerification(deserialize_payload(
                    template_code,
                    payload.as_ref(),
                )?))
            }
            MailTemplateCode::LoginEmailCode => Ok(Self::LoginEmailCode(deserialize_payload(
                template_code,
                payload.as_ref(),
            )?)),
            MailTemplateCode::UserInvitation => Ok(Self::UserInvitation(deserialize_payload(
                template_code,
                payload.as_ref(),
            )?)),
        }
    }
}

pub fn list_template_variable_groups() -> Vec<TemplateVariableGroup> {
    vec![
        template_variable_group(
            MailTemplateCode::RegisterActivation,
            &[
                placeholder_spec(
                    "username",
                    "settings_template_variable_username_label",
                    "settings_template_variable_username_desc",
                ),
                placeholder_spec(
                    "verification_url",
                    "settings_template_variable_verification_url_label",
                    "settings_template_variable_verification_url_desc",
                ),
                placeholder_spec(
                    "site_name",
                    "settings_template_variable_site_name_label",
                    "settings_template_variable_site_name_desc",
                ),
            ],
        ),
        template_variable_group(
            MailTemplateCode::ContactChangeConfirmation,
            &[
                placeholder_spec(
                    "username",
                    "settings_template_variable_username_label",
                    "settings_template_variable_username_desc",
                ),
                placeholder_spec(
                    "verification_url",
                    "settings_template_variable_verification_url_label",
                    "settings_template_variable_verification_url_desc",
                ),
                placeholder_spec(
                    "site_name",
                    "settings_template_variable_site_name_label",
                    "settings_template_variable_site_name_desc",
                ),
            ],
        ),
        template_variable_group(
            MailTemplateCode::PasswordReset,
            &[
                placeholder_spec(
                    "username",
                    "settings_template_variable_username_label",
                    "settings_template_variable_username_desc",
                ),
                placeholder_spec(
                    "reset_url",
                    "settings_template_variable_reset_url_label",
                    "settings_template_variable_reset_url_desc",
                ),
                placeholder_spec(
                    "site_name",
                    "settings_template_variable_site_name_label",
                    "settings_template_variable_site_name_desc",
                ),
            ],
        ),
        template_variable_group(
            MailTemplateCode::PasswordResetNotice,
            &[
                placeholder_spec(
                    "username",
                    "settings_template_variable_username_label",
                    "settings_template_variable_username_desc",
                ),
                placeholder_spec(
                    "site_name",
                    "settings_template_variable_site_name_label",
                    "settings_template_variable_site_name_desc",
                ),
            ],
        ),
        template_variable_group(
            MailTemplateCode::ContactChangeNotice,
            &[
                placeholder_spec(
                    "username",
                    "settings_template_variable_username_label",
                    "settings_template_variable_username_desc",
                ),
                placeholder_spec(
                    "previous_email",
                    "settings_template_variable_previous_email_label",
                    "settings_template_variable_previous_email_desc",
                ),
                placeholder_spec(
                    "new_email",
                    "settings_template_variable_new_email_label",
                    "settings_template_variable_new_email_desc",
                ),
                placeholder_spec(
                    "site_name",
                    "settings_template_variable_site_name_label",
                    "settings_template_variable_site_name_desc",
                ),
            ],
        ),
        template_variable_group(
            MailTemplateCode::ExternalAuthEmailVerification,
            &[
                placeholder_spec(
                    "email",
                    "settings_template_variable_email_label",
                    "settings_template_variable_email_desc",
                ),
                placeholder_spec(
                    "verification_url",
                    "settings_template_variable_verification_url_label",
                    "settings_template_variable_verification_url_desc",
                ),
                placeholder_spec(
                    "provider_name",
                    "settings_template_variable_provider_name_label",
                    "settings_template_variable_provider_name_desc",
                ),
                placeholder_spec(
                    "site_name",
                    "settings_template_variable_site_name_label",
                    "settings_template_variable_site_name_desc",
                ),
                placeholder_spec(
                    "expires_in",
                    "settings_template_variable_expires_in_label",
                    "settings_template_variable_expires_in_desc",
                ),
            ],
        ),
        template_variable_group(
            MailTemplateCode::LoginEmailCode,
            &[
                placeholder_spec(
                    "username",
                    "settings_template_variable_username_label",
                    "settings_template_variable_username_desc",
                ),
                placeholder_spec(
                    "code",
                    "settings_template_variable_code_label",
                    "settings_template_variable_code_desc",
                ),
                placeholder_spec(
                    "site_name",
                    "settings_template_variable_site_name_label",
                    "settings_template_variable_site_name_desc",
                ),
                placeholder_spec(
                    "expires_in",
                    "settings_template_variable_expires_in_label",
                    "settings_template_variable_expires_in_desc",
                ),
                placeholder_spec(
                    "lang",
                    "settings_template_variable_lang_label",
                    "settings_template_variable_lang_desc",
                ),
            ],
        ),
        template_variable_group(
            MailTemplateCode::UserInvitation,
            &[
                placeholder_spec(
                    "email",
                    "settings_template_variable_email_label",
                    "settings_template_variable_email_desc",
                ),
                placeholder_spec(
                    "invitation_url",
                    "settings_template_variable_invitation_url_label",
                    "settings_template_variable_invitation_url_desc",
                ),
                placeholder_spec(
                    "site_name",
                    "settings_template_variable_site_name_label",
                    "settings_template_variable_site_name_desc",
                ),
                placeholder_spec(
                    "expires_in",
                    "settings_template_variable_expires_in_label",
                    "settings_template_variable_expires_in_desc",
                ),
            ],
        ),
    ]
}

pub fn render(
    runtime_config: &RuntimeConfig,
    template_code: MailTemplateCode,
    payload: &StoredMailPayload,
) -> Result<RenderedMail> {
    let placeholders = match MailTemplatePayload::from_stored(template_code, payload)? {
        MailTemplatePayload::RegisterActivation(payload) => {
            let verification_url = verification_link(runtime_config, &payload.token);
            PlaceholderSet {
                text_values: vec![
                    ("username", payload.username.clone()),
                    ("verification_url", verification_url.clone()),
                    ("site_name", payload.site_name.clone()),
                ],
                html_values: vec![
                    ("username", escape_html(&payload.username)),
                    ("verification_url", escape_html(&verification_url)),
                    ("site_name", escape_html(&payload.site_name)),
                ],
            }
        }
        MailTemplatePayload::ContactChangeConfirmation(payload) => {
            let verification_url = verification_link(runtime_config, &payload.token);
            PlaceholderSet {
                text_values: vec![
                    ("username", payload.username.clone()),
                    ("verification_url", verification_url.clone()),
                    ("site_name", payload.site_name.clone()),
                ],
                html_values: vec![
                    ("username", escape_html(&payload.username)),
                    ("verification_url", escape_html(&verification_url)),
                    ("site_name", escape_html(&payload.site_name)),
                ],
            }
        }
        MailTemplatePayload::PasswordReset(payload) => {
            let reset_url = password_reset_link(runtime_config, &payload.token);
            PlaceholderSet {
                text_values: vec![
                    ("username", payload.username.clone()),
                    ("reset_url", reset_url.clone()),
                    ("site_name", payload.site_name.clone()),
                ],
                html_values: vec![
                    ("username", escape_html(&payload.username)),
                    ("reset_url", escape_html(&reset_url)),
                    ("site_name", escape_html(&payload.site_name)),
                ],
            }
        }
        MailTemplatePayload::PasswordResetNotice(payload) => PlaceholderSet {
            text_values: vec![
                ("username", payload.username.clone()),
                ("site_name", payload.site_name.clone()),
            ],
            html_values: vec![
                ("username", escape_html(&payload.username)),
                ("site_name", escape_html(&payload.site_name)),
            ],
        },
        MailTemplatePayload::ContactChangeNotice(payload) => PlaceholderSet {
            text_values: vec![
                ("username", payload.username.clone()),
                ("previous_email", payload.previous_email.clone()),
                ("new_email", payload.new_email.clone()),
                ("site_name", payload.site_name.clone()),
            ],
            html_values: vec![
                ("username", escape_html(&payload.username)),
                ("previous_email", escape_html(&payload.previous_email)),
                ("new_email", escape_html(&payload.new_email)),
                ("site_name", escape_html(&payload.site_name)),
            ],
        },
        MailTemplatePayload::ExternalAuthEmailVerification(payload) => {
            let verification_url =
                external_auth_email_verification_link(runtime_config, &payload.token);
            PlaceholderSet {
                text_values: vec![
                    ("email", payload.email.clone()),
                    ("verification_url", verification_url.clone()),
                    ("provider_name", payload.provider_name.clone()),
                    ("site_name", payload.site_name.clone()),
                    ("expires_in", payload.expires_in.clone()),
                ],
                html_values: vec![
                    ("email", escape_html(&payload.email)),
                    ("verification_url", escape_html(&verification_url)),
                    ("provider_name", escape_html(&payload.provider_name)),
                    ("site_name", escape_html(&payload.site_name)),
                    ("expires_in", escape_html(&payload.expires_in)),
                ],
            }
        }
        MailTemplatePayload::LoginEmailCode(payload) => PlaceholderSet {
            text_values: vec![
                ("username", payload.username.clone()),
                ("code", payload.code.clone()),
                ("site_name", payload.site_name.clone()),
                ("expires_in", payload.expires_in.clone()),
                ("lang", normalize_mail_template_lang(&payload.lang)),
            ],
            html_values: vec![
                ("username", escape_html(&payload.username)),
                ("code", escape_html(&payload.code)),
                ("site_name", escape_html(&payload.site_name)),
                ("expires_in", escape_html(&payload.expires_in)),
                (
                    "lang",
                    escape_html(&normalize_mail_template_lang(&payload.lang)),
                ),
            ],
        },
        MailTemplatePayload::UserInvitation(payload) => PlaceholderSet {
            text_values: vec![
                ("email", payload.email.clone()),
                ("invitation_url", payload.invitation_url.clone()),
                ("site_name", payload.site_name.clone()),
                ("expires_in", payload.expires_in.clone()),
            ],
            html_values: vec![
                ("email", escape_html(&payload.email)),
                ("invitation_url", escape_html(&payload.invitation_url)),
                ("site_name", escape_html(&payload.site_name)),
                ("expires_in", escape_html(&payload.expires_in)),
            ],
        },
    };

    let subject = render_placeholders(
        mail::template_subject(runtime_config, template_code),
        &placeholders.text_values,
    );
    let html_body = render_placeholders(
        mail::template_html(runtime_config, template_code),
        &placeholders.html_values,
    );
    let text_body = html_to_text(&html_body);

    Ok(RenderedMail {
        subject,
        text_body,
        html_body,
    })
}

fn serialize_payload<T: Serialize>(payload: &T) -> Result<String> {
    serde_json::to_string(payload).map_aster_err_ctx(
        "failed to serialize mail payload",
        AsterError::internal_error,
    )
}

fn default_external_auth_provider_name() -> String {
    "single sign-on provider".to_string()
}

fn default_site_name() -> String {
    branding::DEFAULT_BRANDING_TITLE.to_string()
}

fn default_external_auth_expires_in() -> String {
    "30 minutes".to_string()
}

fn default_login_email_code_expires_in() -> String {
    "10 minutes".to_string()
}

fn default_user_invitation_expires_in() -> String {
    "72 hours".to_string()
}

fn default_mail_template_lang() -> String {
    "en".to_string()
}

fn normalize_mail_template_lang(value: &str) -> String {
    let normalized = value.trim();
    if normalized.is_empty()
        || !normalized
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
    {
        return default_mail_template_lang();
    }
    normalized.to_string()
}

fn deserialize_payload<T: DeserializeOwned>(
    template_code: MailTemplateCode,
    payload_json: &str,
) -> Result<T> {
    serde_json::from_str(payload_json).map_aster_err_ctx(
        &format!("failed to decode {} mail payload", template_code.as_str()),
        AsterError::internal_error,
    )
}

fn verification_link(runtime_config: &RuntimeConfig, token: &str) -> String {
    site_url::public_app_url_or_path(
        runtime_config,
        &format!(
            "/api/v1/auth/contact-verification/confirm?token={}",
            urlencoding::encode(token)
        ),
    )
}

fn password_reset_link(runtime_config: &RuntimeConfig, token: &str) -> String {
    site_url::public_app_url_or_path(
        runtime_config,
        &format!("/reset-password?token={}", urlencoding::encode(token)),
    )
}

fn external_auth_email_verification_link(runtime_config: &RuntimeConfig, token: &str) -> String {
    site_url::public_app_url_or_path(
        runtime_config,
        &format!(
            "/api/v1/auth/external-auth/email-verification/confirm?token={}",
            urlencoding::encode(token)
        ),
    )
}

struct PlaceholderSpec {
    key: &'static str,
    label_i18n_key: &'static str,
    description_i18n_key: &'static str,
}

const fn placeholder_spec(
    key: &'static str,
    label_i18n_key: &'static str,
    description_i18n_key: &'static str,
) -> PlaceholderSpec {
    PlaceholderSpec {
        key,
        label_i18n_key,
        description_i18n_key,
    }
}

fn template_variable_group(
    template_code: MailTemplateCode,
    variables: &[PlaceholderSpec],
) -> TemplateVariableGroup {
    TemplateVariableGroup {
        category: crate::config::definitions::CONFIG_CATEGORY_MAIL_TEMPLATE.to_string(),
        template_code: template_code.as_str().to_string(),
        label_i18n_key: format!("settings_mail_template_group_{}", template_code.as_str()),
        variables: variables
            .iter()
            .map(|variable| TemplateVariableItem {
                token: format!("{{{{{}}}}}", variable.key),
                label_i18n_key: variable.label_i18n_key.to_string(),
                description_i18n_key: variable.description_i18n_key.to_string(),
            })
            .collect(),
    }
}

fn render_placeholders(mut template: String, values: &[(&'static str, String)]) -> String {
    for (key, value) in values {
        let placeholder = format!("{{{{{key}}}}}");
        template = template.replace(&placeholder, value);
    }
    template
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn html_to_text(html: &str) -> String {
    let mut output = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut tag = String::new();
    let mut ignored_tags = Vec::new();

    for ch in html.chars() {
        if in_tag {
            if ch == '>' {
                if let Some(parsed_tag) = parse_tag(&tag) {
                    if ignored_tags.is_empty() {
                        apply_tag_to_text(&mut output, &parsed_tag);
                    }
                    update_ignored_tags(&mut ignored_tags, &parsed_tag);
                }
                tag.clear();
                in_tag = false;
            } else {
                tag.push(ch);
            }
            continue;
        }

        if ch == '<' {
            in_tag = true;
            continue;
        }

        if ignored_tags.is_empty() {
            output.push(ch);
        }
    }

    let decoded = decode_html_entities(&output);
    normalize_text_fallback(&decoded)
}

fn apply_tag_to_text(output: &mut String, tag: &ParsedTag) {
    if tag.is_closing {
        return;
    }

    if tag.name == "li" && !output.ends_with("- ") {
        if !output.is_empty() && !output.ends_with('\n') {
            output.push('\n');
        }
        output.push_str("- ");
        return;
    }

    let needs_newline = matches!(
        tag.name.as_str(),
        "p" | "div"
            | "section"
            | "article"
            | "header"
            | "footer"
            | "tr"
            | "table"
            | "br"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
    );

    if needs_newline && !output.is_empty() && !output.ends_with('\n') {
        output.push('\n');
    }
}

fn parse_tag(tag: &str) -> Option<ParsedTag> {
    let trimmed = tag.trim();
    if trimmed.is_empty() || trimmed.starts_with('!') || trimmed.starts_with('?') {
        return None;
    }

    let is_closing = trimmed.starts_with('/');
    let content = if is_closing { &trimmed[1..] } else { trimmed };
    let is_self_closing = content.ends_with('/');
    let name = content
        .trim_end_matches('/')
        .split_whitespace()
        .next()?
        .to_ascii_lowercase();

    Some(ParsedTag {
        name,
        is_closing,
        is_self_closing,
    })
}

fn update_ignored_tags(ignored_tags: &mut Vec<String>, tag: &ParsedTag) {
    if !is_ignored_text_tag(&tag.name) || tag.is_self_closing {
        return;
    }

    if tag.is_closing {
        if ignored_tags.last().is_some_and(|name| name == &tag.name) {
            ignored_tags.pop();
        }
        return;
    }

    ignored_tags.push(tag.name.clone());
}

fn is_ignored_text_tag(name: &str) -> bool {
    matches!(name, "head" | "script" | "style" | "title")
}

fn decode_html_entities(value: &str) -> String {
    value
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

fn normalize_text_fallback(value: &str) -> String {
    let mut normalized = String::new();
    let mut last_blank = true;

    for line in value.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !last_blank {
                normalized.push('\n');
            }
            last_blank = true;
            continue;
        }

        if !normalized.is_empty() && !normalized.ends_with('\n') {
            normalized.push('\n');
        }
        normalized.push_str(trimmed);
        last_blank = false;
    }

    normalized.trim().to_string()
}

struct PlaceholderSet {
    text_values: Vec<(&'static str, String)>,
    html_values: Vec<(&'static str, String)>,
}

struct ParsedTag {
    name: String,
    is_closing: bool,
    is_self_closing: bool,
}

#[cfg(test)]
mod tests {
    use super::{MailTemplateCode, MailTemplatePayload, list_template_variable_groups, render};
    use crate::config::RuntimeConfig;
    use crate::config::definitions::CONFIG_CATEGORY_MAIL_TEMPLATE;
    use crate::entities::system_config;
    use chrono::Utc;

    fn config_model(key: &str, value: &str) -> system_config::Model {
        system_config::Model {
            id: 1,
            key: key.to_string(),
            value: value.to_string(),
            value_type: crate::types::SystemConfigValueType::Multiline,
            requires_restart: false,
            is_sensitive: false,
            source: crate::types::SystemConfigSource::System,
            visibility: crate::types::SystemConfigVisibility::Private,
            namespace: String::new(),
            category: CONFIG_CATEGORY_MAIL_TEMPLATE.to_string(),
            description: "test".to_string(),
            updated_at: Utc::now(),
            updated_by: None,
        }
    }

    #[test]
    fn render_register_activation_builds_link_and_escapes_html() {
        let runtime_config = RuntimeConfig::new();
        let payload = MailTemplatePayload::register_activation("A&B", "token-123", "Drive & Files");
        let stored = payload.to_stored().unwrap();
        let rendered = render(
            &runtime_config,
            MailTemplateCode::RegisterActivation,
            &stored,
        )
        .unwrap();

        assert!(rendered.text_body.contains("token=token-123"));
        assert!(rendered.html_body.starts_with("<!doctype html>"));
        assert!(rendered.html_body.contains("A&amp;B"));
        assert!(rendered.html_body.contains("Drive &amp; Files"));
        assert!(rendered.subject.contains("Drive & Files"));
    }

    #[test]
    fn render_external_auth_email_verification_builds_link_and_escapes_html() {
        let runtime_config = RuntimeConfig::new();
        let payload = MailTemplatePayload::external_auth_email_verification(
            "oidc+user@example.com",
            "token-123",
            "Acme <SSO>",
            "Drive & Files",
            "30 minutes",
        );
        let stored = payload.to_stored().unwrap();
        let rendered = render(
            &runtime_config,
            MailTemplateCode::ExternalAuthEmailVerification,
            &stored,
        )
        .unwrap();

        assert!(
            rendered
                .text_body
                .contains("/api/v1/auth/external-auth/email-verification/confirm?token=token-123",)
        );
        assert!(rendered.html_body.contains("oidc+user@example.com"));
        assert!(rendered.html_body.contains("Acme &lt;SSO&gt;"));
        assert!(rendered.html_body.contains("Drive &amp; Files"));
        assert!(rendered.html_body.contains("30 minutes"));
        assert!(rendered.subject.contains("Drive & Files"));
        assert!(rendered.text_body.contains("Acme <SSO>"));
    }

    #[test]
    fn external_auth_email_verification_variables_exclude_username() {
        let groups = list_template_variable_groups();
        let group = groups
            .iter()
            .find(|group| group.template_code == "external_auth_email_verification")
            .expect("external auth email verification variable group should exist");
        let tokens = group
            .variables
            .iter()
            .map(|variable| variable.token.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            tokens,
            vec![
                "{{email}}",
                "{{verification_url}}",
                "{{provider_name}}",
                "{{site_name}}",
                "{{expires_in}}",
            ]
        );
        assert!(!tokens.contains(&"{{username}}"));
    }

    #[test]
    fn render_login_email_code_sets_default_html_lang() {
        let runtime_config = RuntimeConfig::new();
        let payload = MailTemplatePayload::login_email_code(
            "Alice",
            "12345678",
            "Drive & Files",
            "5 minutes",
        );
        let stored = payload.to_stored().unwrap();
        let rendered = render(&runtime_config, MailTemplateCode::LoginEmailCode, &stored).unwrap();

        assert!(rendered.html_body.contains("<html lang=\"en\">"));
        assert!(rendered.html_body.contains("12345678"));
    }

    #[test]
    fn all_mail_template_variable_groups_include_site_name() {
        for group in list_template_variable_groups() {
            assert!(
                group
                    .variables
                    .iter()
                    .any(|variable| variable.token == "{{site_name}}"),
                "{} should expose site_name",
                group.template_code
            );
        }
    }

    #[test]
    fn stored_mail_payload_round_trips_with_template_code() {
        let payload = MailTemplatePayload::contact_change_notice(
            "Alice",
            "old@example.com",
            "new@example.com",
            "AsterDrive",
        );
        let stored = payload.to_stored().unwrap();

        let decoded =
            MailTemplatePayload::from_stored(MailTemplateCode::ContactChangeNotice, &stored)
                .unwrap();

        assert_eq!(decoded, payload);
    }

    #[test]
    fn html_to_text_generates_multiline_fallback() {
        let html = "<p>Hello &amp; welcome</p><p><a href=\"https://example.com\">https://example.com</a></p>";

        assert_eq!(
            super::html_to_text(html),
            "Hello & welcome\nhttps://example.com"
        );
    }

    #[test]
    fn html_to_text_ignores_head_content() {
        let html = "<!doctype html><html><head><title>Ignore me</title><style>.note { color: red; }</style></head><body><p>Hello</p></body></html>";

        assert_eq!(super::html_to_text(html), "Hello");
    }

    #[test]
    fn render_keeps_existing_full_html_documents() {
        let runtime_config = RuntimeConfig::new();
        runtime_config.apply(config_model(
            crate::config::mail::MAIL_TEMPLATE_PASSWORD_RESET_HTML_KEY,
            "<!doctype html><html><body><p>Hello {{username}}</p></body></html>",
        ));

        let payload = MailTemplatePayload::password_reset("Alice", "token-123", "AsterDrive");
        let stored = payload.to_stored().unwrap();
        let rendered = render(&runtime_config, MailTemplateCode::PasswordReset, &stored).unwrap();

        assert_eq!(rendered.html_body.matches("<html").count(), 1);
        assert!(rendered.html_body.contains("<p>Hello Alice</p>"));
    }
}

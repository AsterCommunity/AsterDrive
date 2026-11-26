//! 服务模块：`mail::template`。

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::config::{RuntimeConfig, branding, mail, site_url};
use crate::errors::{AsterError, MapAsterErr, Result};
use aster_forge_mail::{
    MailTemplateCode, MailTemplateDefinition, MailTemplateRegistry, RenderedMail,
    StoredMailPayload, TemplatePlaceholderSet, TemplateVariableSpec, escape_html, render_template,
};

/// Template variables for the register activation mail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisterActivationPayload {
    /// Display name of the account awaiting activation.
    pub username: String,
    /// Activation token embedded in the verification URL.
    pub token: String,
    /// Branding title shown in the mail body, defaulted for old stored rows.
    #[serde(default = "default_site_name")]
    pub site_name: String,
}

/// Template variables for the contact-change confirmation mail sent to the new address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContactChangeConfirmationPayload {
    /// Display name of the user changing their contact address.
    pub username: String,
    /// Confirmation token embedded in the verification URL.
    pub token: String,
    /// Branding title shown in the mail body, defaulted for old stored rows.
    #[serde(default = "default_site_name")]
    pub site_name: String,
}

/// Template variables for the password reset request mail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PasswordResetPayload {
    /// Display name of the user who requested the reset.
    pub username: String,
    /// Reset token embedded in the reset URL.
    pub token: String,
    /// Branding title shown in the mail body, defaulted for old stored rows.
    #[serde(default = "default_site_name")]
    pub site_name: String,
}

/// Template variables for the notice sent after a password reset succeeded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PasswordResetNoticePayload {
    /// Display name of the user whose password was changed.
    pub username: String,
    /// Branding title shown in the mail body, defaulted for old stored rows.
    #[serde(default = "default_site_name")]
    pub site_name: String,
}

/// Template variables for the contact-change notice sent to the previous address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContactChangeNoticePayload {
    /// Display name of the user who changed their contact address.
    pub username: String,
    /// Address the mail is sent to, now replaced by the new address.
    pub previous_email: String,
    /// Address that became active after the confirmation.
    pub new_email: String,
    /// Branding title shown in the mail body, defaulted for old stored rows.
    #[serde(default = "default_site_name")]
    pub site_name: String,
}

/// Template variables for the external-auth email verification mail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalAuthEmailVerificationPayload {
    /// Address being verified for the pending external-auth login.
    pub email: String,
    /// Verification token embedded in the verification URL.
    pub token: String,
    /// Display name of the external auth provider, e.g. an OIDC issuer.
    #[serde(default = "default_external_auth_provider_name")]
    pub provider_name: String,
    /// Branding title shown in the mail body, defaulted for old stored rows.
    #[serde(default = "default_site_name")]
    pub site_name: String,
    /// Human-readable validity window of the link, e.g. `"30 minutes"`.
    #[serde(default = "default_external_auth_expires_in")]
    pub expires_in: String,
}

/// Template variables for the one-time login code mail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoginEmailCodePayload {
    /// Display name of the user requesting the login code.
    pub username: String,
    /// One-time login code the recipient must enter to finish signing in.
    pub code: String,
    /// Branding title shown in the mail body, defaulted for old stored rows.
    #[serde(default = "default_site_name")]
    pub site_name: String,
    /// Human-readable validity window of the code, e.g. `"10 minutes"`.
    #[serde(default = "default_login_email_code_expires_in")]
    pub expires_in: String,
    /// BCP 47 language tag for the HTML mail, e.g. `"en"`; falls back to
    /// `"en"` when missing or not alphanumeric-dash.
    #[serde(default = "default_mail_template_lang")]
    pub lang: String,
}

/// Template variables for the user invitation mail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserInvitationPayload {
    /// Address the invitation is sent to.
    pub email: String,
    /// Pre-authorized URL the invitee opens to accept the invitation.
    pub invitation_url: String,
    /// Branding title shown in the mail body, defaulted for old stored rows.
    #[serde(default = "default_site_name")]
    pub site_name: String,
    /// Human-readable validity window of the invitation, e.g. `"72 hours"`.
    #[serde(default = "default_user_invitation_expires_in")]
    pub expires_in: String,
}

/// Typed payload for one of the built-in mail templates, stored as JSON in
/// the outbox and turned into placeholder sets at delivery time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MailTemplatePayload {
    /// Payload for the register activation mail.
    RegisterActivation(RegisterActivationPayload),
    /// Payload for the contact-change confirmation mail.
    ContactChangeConfirmation(ContactChangeConfirmationPayload),
    /// Payload for the password reset mail.
    PasswordReset(PasswordResetPayload),
    /// Payload for the post-reset security notice mail.
    PasswordResetNotice(PasswordResetNoticePayload),
    /// Payload for the contact-change notice mail.
    ContactChangeNotice(ContactChangeNoticePayload),
    /// Payload for the external-auth email verification mail.
    ExternalAuthEmailVerification(ExternalAuthEmailVerificationPayload),
    /// Payload for the login email code mail.
    LoginEmailCode(LoginEmailCodePayload),
    /// Payload for the user invitation mail.
    UserInvitation(UserInvitationPayload),
}

impl MailTemplatePayload {
    /// Builds the payload for the register activation mail; `token` is
    /// exchanged at the contact-verification confirm endpoint.
    pub fn register_activation(username: &str, token: &str, site_name: &str) -> Self {
        Self::RegisterActivation(RegisterActivationPayload {
            username: username.to_string(),
            token: token.to_string(),
            site_name: site_name.to_string(),
        })
    }

    /// Builds the payload for the contact-change confirmation mail sent to
    /// the new address; `token` is exchanged at the confirm endpoint.
    pub fn contact_change_confirmation(username: &str, token: &str, site_name: &str) -> Self {
        Self::ContactChangeConfirmation(ContactChangeConfirmationPayload {
            username: username.to_string(),
            token: token.to_string(),
            site_name: site_name.to_string(),
        })
    }

    /// Builds the payload for the password reset mail; `token` is exchanged
    /// at the reset endpoint.
    pub fn password_reset(username: &str, token: &str, site_name: &str) -> Self {
        Self::PasswordReset(PasswordResetPayload {
            username: username.to_string(),
            token: token.to_string(),
            site_name: site_name.to_string(),
        })
    }

    /// Builds the payload for the security notice sent after a password
    /// reset has been applied; carries no token or link.
    pub fn password_reset_notice(username: &str, site_name: &str) -> Self {
        Self::PasswordResetNotice(PasswordResetNoticePayload {
            username: username.to_string(),
            site_name: site_name.to_string(),
        })
    }

    /// Builds the payload for the notice sent to the previous address once
    /// the contact change has been confirmed.
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

    /// Builds the payload for the external-auth email verification mail;
    /// `expires_in` is a pre-formatted human-readable duration.
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

    /// Builds the payload for the login email code mail; the template
    /// language stays at its default so rendering normalizes it.
    pub fn login_email_code(username: &str, code: &str, site_name: &str, expires_in: &str) -> Self {
        Self::LoginEmailCode(LoginEmailCodePayload {
            username: username.to_string(),
            code: code.to_string(),
            site_name: site_name.to_string(),
            expires_in: expires_in.to_string(),
            lang: default_mail_template_lang(),
        })
    }

    /// Builds the payload for the user invitation mail; `invitation_url` is
    /// the pre-authorized acceptance link built by the caller.
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

    /// Returns the template code this payload renders under.
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

    /// Serializes this payload into the JSON stored on the outbox row.
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

    /// Deserializes the JSON stored on an outbox row into the payload
    /// matching `template_code`; fails when the stored shape does not fit.
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

const USERNAME_VARIABLE: TemplateVariableSpec = TemplateVariableSpec::new(
    "username",
    "settings_template_variable_username_label",
    "settings_template_variable_username_desc",
);
const EMAIL_VARIABLE: TemplateVariableSpec = TemplateVariableSpec::new(
    "email",
    "settings_template_variable_email_label",
    "settings_template_variable_email_desc",
);
const VERIFICATION_URL_VARIABLE: TemplateVariableSpec = TemplateVariableSpec::new(
    "verification_url",
    "settings_template_variable_verification_url_label",
    "settings_template_variable_verification_url_desc",
);
const RESET_URL_VARIABLE: TemplateVariableSpec = TemplateVariableSpec::new(
    "reset_url",
    "settings_template_variable_reset_url_label",
    "settings_template_variable_reset_url_desc",
);
const SITE_NAME_VARIABLE: TemplateVariableSpec = TemplateVariableSpec::new(
    "site_name",
    "settings_template_variable_site_name_label",
    "settings_template_variable_site_name_desc",
);
const PREVIOUS_EMAIL_VARIABLE: TemplateVariableSpec = TemplateVariableSpec::new(
    "previous_email",
    "settings_template_variable_previous_email_label",
    "settings_template_variable_previous_email_desc",
);
const NEW_EMAIL_VARIABLE: TemplateVariableSpec = TemplateVariableSpec::new(
    "new_email",
    "settings_template_variable_new_email_label",
    "settings_template_variable_new_email_desc",
);
const PROVIDER_NAME_VARIABLE: TemplateVariableSpec = TemplateVariableSpec::new(
    "provider_name",
    "settings_template_variable_provider_name_label",
    "settings_template_variable_provider_name_desc",
);
const EXPIRES_IN_VARIABLE: TemplateVariableSpec = TemplateVariableSpec::new(
    "expires_in",
    "settings_template_variable_expires_in_label",
    "settings_template_variable_expires_in_desc",
);
const CODE_VARIABLE: TemplateVariableSpec = TemplateVariableSpec::new(
    "code",
    "settings_template_variable_code_label",
    "settings_template_variable_code_desc",
);
const LANG_VARIABLE: TemplateVariableSpec = TemplateVariableSpec::new(
    "lang",
    "settings_template_variable_lang_label",
    "settings_template_variable_lang_desc",
);
const INVITATION_URL_VARIABLE: TemplateVariableSpec = TemplateVariableSpec::new(
    "invitation_url",
    "settings_template_variable_invitation_url_label",
    "settings_template_variable_invitation_url_desc",
);

const REGISTER_ACTIVATION_VARIABLES: &[TemplateVariableSpec] = &[
    USERNAME_VARIABLE,
    VERIFICATION_URL_VARIABLE,
    SITE_NAME_VARIABLE,
];
const CONTACT_CHANGE_CONFIRMATION_VARIABLES: &[TemplateVariableSpec] = &[
    USERNAME_VARIABLE,
    VERIFICATION_URL_VARIABLE,
    SITE_NAME_VARIABLE,
];
const PASSWORD_RESET_VARIABLES: &[TemplateVariableSpec] =
    &[USERNAME_VARIABLE, RESET_URL_VARIABLE, SITE_NAME_VARIABLE];
const PASSWORD_RESET_NOTICE_VARIABLES: &[TemplateVariableSpec] =
    &[USERNAME_VARIABLE, SITE_NAME_VARIABLE];
const CONTACT_CHANGE_NOTICE_VARIABLES: &[TemplateVariableSpec] = &[
    USERNAME_VARIABLE,
    PREVIOUS_EMAIL_VARIABLE,
    NEW_EMAIL_VARIABLE,
    SITE_NAME_VARIABLE,
];
const EXTERNAL_AUTH_EMAIL_VERIFICATION_VARIABLES: &[TemplateVariableSpec] = &[
    EMAIL_VARIABLE,
    VERIFICATION_URL_VARIABLE,
    PROVIDER_NAME_VARIABLE,
    SITE_NAME_VARIABLE,
    EXPIRES_IN_VARIABLE,
];
const LOGIN_EMAIL_CODE_VARIABLES: &[TemplateVariableSpec] = &[
    USERNAME_VARIABLE,
    CODE_VARIABLE,
    SITE_NAME_VARIABLE,
    EXPIRES_IN_VARIABLE,
    LANG_VARIABLE,
];
const USER_INVITATION_VARIABLES: &[TemplateVariableSpec] = &[
    EMAIL_VARIABLE,
    INVITATION_URL_VARIABLE,
    SITE_NAME_VARIABLE,
    EXPIRES_IN_VARIABLE,
];

const MAIL_TEMPLATE_DEFINITIONS: &[MailTemplateDefinition] = &[
    MailTemplateDefinition::new(
        MailTemplateCode::RegisterActivation.as_str(),
        crate::config::definitions::CONFIG_CATEGORY_MAIL_TEMPLATE,
        "settings_mail_template_group_register_activation",
        REGISTER_ACTIVATION_VARIABLES,
    ),
    MailTemplateDefinition::new(
        MailTemplateCode::ContactChangeConfirmation.as_str(),
        crate::config::definitions::CONFIG_CATEGORY_MAIL_TEMPLATE,
        "settings_mail_template_group_contact_change_confirmation",
        CONTACT_CHANGE_CONFIRMATION_VARIABLES,
    ),
    MailTemplateDefinition::new(
        MailTemplateCode::PasswordReset.as_str(),
        crate::config::definitions::CONFIG_CATEGORY_MAIL_TEMPLATE,
        "settings_mail_template_group_password_reset",
        PASSWORD_RESET_VARIABLES,
    ),
    MailTemplateDefinition::new(
        MailTemplateCode::PasswordResetNotice.as_str(),
        crate::config::definitions::CONFIG_CATEGORY_MAIL_TEMPLATE,
        "settings_mail_template_group_password_reset_notice",
        PASSWORD_RESET_NOTICE_VARIABLES,
    ),
    MailTemplateDefinition::new(
        MailTemplateCode::ContactChangeNotice.as_str(),
        crate::config::definitions::CONFIG_CATEGORY_MAIL_TEMPLATE,
        "settings_mail_template_group_contact_change_notice",
        CONTACT_CHANGE_NOTICE_VARIABLES,
    ),
    MailTemplateDefinition::new(
        MailTemplateCode::ExternalAuthEmailVerification.as_str(),
        crate::config::definitions::CONFIG_CATEGORY_MAIL_TEMPLATE,
        "settings_mail_template_group_external_auth_email_verification",
        EXTERNAL_AUTH_EMAIL_VERIFICATION_VARIABLES,
    ),
    MailTemplateDefinition::new(
        MailTemplateCode::LoginEmailCode.as_str(),
        crate::config::definitions::CONFIG_CATEGORY_MAIL_TEMPLATE,
        "settings_mail_template_group_login_email_code",
        LOGIN_EMAIL_CODE_VARIABLES,
    ),
    MailTemplateDefinition::new(
        MailTemplateCode::UserInvitation.as_str(),
        crate::config::definitions::CONFIG_CATEGORY_MAIL_TEMPLATE,
        "settings_mail_template_group_user_invitation",
        USER_INVITATION_VARIABLES,
    ),
];

/// Static registry describing every built-in mail template and its variables.
///
/// Backs the admin template editing UI and the rendering config keys; the
/// variable groups are asserted complete and valid at startup.
pub const MAIL_TEMPLATE_REGISTRY: MailTemplateRegistry =
    MailTemplateRegistry::new(MAIL_TEMPLATE_DEFINITIONS);

/// Validates that every template in the registry declares unique codes and
/// known variables.
///
/// Invoked at startup so a bad built-in template definition fails fast
/// instead of producing broken admin settings or renders.
pub fn validate_template_registry() -> Result<()> {
    MAIL_TEMPLATE_REGISTRY.validate().map_err(|error| {
        AsterError::internal_error(format!("invalid mail template registry: {error}"))
    })
}

/// Renders the subject and HTML body for `template_code` from a stored payload.
///
/// Decodes the payload, builds raw and HTML-escaped placeholder sets, and
/// fills in the subject and HTML templates configured in runtime config; the
/// plain-text body is derived from the rendered HTML. Link variables such as
/// `verification_url` and `reset_url` are built here from the token embedded
/// in the payload.
pub fn render(
    runtime_config: &RuntimeConfig,
    template_code: MailTemplateCode,
    payload: &StoredMailPayload,
) -> Result<RenderedMail> {
    let placeholders = match MailTemplatePayload::from_stored(template_code, payload)? {
        MailTemplatePayload::RegisterActivation(payload) => {
            let verification_url = verification_link(runtime_config, &payload.token);
            TemplatePlaceholderSet::new(
                vec![
                    ("username", payload.username.clone()),
                    ("verification_url", verification_url.clone()),
                    ("site_name", payload.site_name.clone()),
                ],
                vec![
                    ("username", escape_html(&payload.username)),
                    ("verification_url", escape_html(&verification_url)),
                    ("site_name", escape_html(&payload.site_name)),
                ],
            )
        }
        MailTemplatePayload::ContactChangeConfirmation(payload) => {
            let verification_url = verification_link(runtime_config, &payload.token);
            TemplatePlaceholderSet::new(
                vec![
                    ("username", payload.username.clone()),
                    ("verification_url", verification_url.clone()),
                    ("site_name", payload.site_name.clone()),
                ],
                vec![
                    ("username", escape_html(&payload.username)),
                    ("verification_url", escape_html(&verification_url)),
                    ("site_name", escape_html(&payload.site_name)),
                ],
            )
        }
        MailTemplatePayload::PasswordReset(payload) => {
            let reset_url = password_reset_link(runtime_config, &payload.token);
            TemplatePlaceholderSet::new(
                vec![
                    ("username", payload.username.clone()),
                    ("reset_url", reset_url.clone()),
                    ("site_name", payload.site_name.clone()),
                ],
                vec![
                    ("username", escape_html(&payload.username)),
                    ("reset_url", escape_html(&reset_url)),
                    ("site_name", escape_html(&payload.site_name)),
                ],
            )
        }
        MailTemplatePayload::PasswordResetNotice(payload) => TemplatePlaceholderSet::new(
            vec![
                ("username", payload.username.clone()),
                ("site_name", payload.site_name.clone()),
            ],
            vec![
                ("username", escape_html(&payload.username)),
                ("site_name", escape_html(&payload.site_name)),
            ],
        ),
        MailTemplatePayload::ContactChangeNotice(payload) => TemplatePlaceholderSet::new(
            vec![
                ("username", payload.username.clone()),
                ("previous_email", payload.previous_email.clone()),
                ("new_email", payload.new_email.clone()),
                ("site_name", payload.site_name.clone()),
            ],
            vec![
                ("username", escape_html(&payload.username)),
                ("previous_email", escape_html(&payload.previous_email)),
                ("new_email", escape_html(&payload.new_email)),
                ("site_name", escape_html(&payload.site_name)),
            ],
        ),
        MailTemplatePayload::ExternalAuthEmailVerification(payload) => {
            let verification_url =
                external_auth_email_verification_link(runtime_config, &payload.token);
            TemplatePlaceholderSet::new(
                vec![
                    ("email", payload.email.clone()),
                    ("verification_url", verification_url.clone()),
                    ("provider_name", payload.provider_name.clone()),
                    ("site_name", payload.site_name.clone()),
                    ("expires_in", payload.expires_in.clone()),
                ],
                vec![
                    ("email", escape_html(&payload.email)),
                    ("verification_url", escape_html(&verification_url)),
                    ("provider_name", escape_html(&payload.provider_name)),
                    ("site_name", escape_html(&payload.site_name)),
                    ("expires_in", escape_html(&payload.expires_in)),
                ],
            )
        }
        MailTemplatePayload::LoginEmailCode(payload) => TemplatePlaceholderSet::new(
            vec![
                ("username", payload.username.clone()),
                ("code", payload.code.clone()),
                ("site_name", payload.site_name.clone()),
                ("expires_in", payload.expires_in.clone()),
                ("lang", normalize_mail_template_lang(&payload.lang)),
            ],
            vec![
                ("username", escape_html(&payload.username)),
                ("code", escape_html(&payload.code)),
                ("site_name", escape_html(&payload.site_name)),
                ("expires_in", escape_html(&payload.expires_in)),
                (
                    "lang",
                    escape_html(&normalize_mail_template_lang(&payload.lang)),
                ),
            ],
        ),
        MailTemplatePayload::UserInvitation(payload) => TemplatePlaceholderSet::new(
            vec![
                ("email", payload.email.clone()),
                ("invitation_url", payload.invitation_url.clone()),
                ("site_name", payload.site_name.clone()),
                ("expires_in", payload.expires_in.clone()),
            ],
            vec![
                ("email", escape_html(&payload.email)),
                ("invitation_url", escape_html(&payload.invitation_url)),
                ("site_name", escape_html(&payload.site_name)),
                ("expires_in", escape_html(&payload.expires_in)),
            ],
        ),
    };

    Ok(render_template(
        &mail::template_subject(runtime_config, template_code),
        &mail::template_html(runtime_config, template_code),
        &placeholders,
    ))
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

#[cfg(test)]
mod tests {
    use super::{
        MAIL_TEMPLATE_REGISTRY, MailTemplateCode, MailTemplatePayload, render,
        validate_template_registry,
    };
    use crate::config::RuntimeConfig;
    use crate::config::definitions::CONFIG_CATEGORY_MAIL_TEMPLATE;
    use aster_forge_db::system_config;
    use chrono::Utc;

    fn config_model(key: &str, value: &str) -> system_config::Model {
        system_config::Model {
            id: 1,
            key: key.to_string(),
            value: value.to_string(),
            value_type: aster_forge_config::ConfigValueType::Multiline,
            requires_restart: false,
            is_sensitive: false,
            source: aster_forge_config::ConfigSource::System,
            visibility: aster_forge_config::ConfigVisibility::Private,
            namespace: String::new(),
            category: CONFIG_CATEGORY_MAIL_TEMPLATE.to_string(),
            description: "test".to_string(),
            updated_at: Utc::now(),
            updated_by: None,
        }
    }

    #[test]
    fn template_registry_is_valid_and_complete() {
        validate_template_registry().unwrap();

        let codes = MAIL_TEMPLATE_REGISTRY
            .variable_groups()
            .into_iter()
            .map(|group| group.template_code)
            .collect::<Vec<_>>();

        assert_eq!(
            codes,
            vec![
                MailTemplateCode::RegisterActivation.as_str(),
                MailTemplateCode::ContactChangeConfirmation.as_str(),
                MailTemplateCode::PasswordReset.as_str(),
                MailTemplateCode::PasswordResetNotice.as_str(),
                MailTemplateCode::ContactChangeNotice.as_str(),
                MailTemplateCode::ExternalAuthEmailVerification.as_str(),
                MailTemplateCode::LoginEmailCode.as_str(),
                MailTemplateCode::UserInvitation.as_str(),
            ]
        );
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
        let groups = MAIL_TEMPLATE_REGISTRY.variable_groups();
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
        for group in MAIL_TEMPLATE_REGISTRY.variable_groups() {
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
            aster_forge_mail::html_to_text(html),
            "Hello & welcome\nhttps://example.com"
        );
    }

    #[test]
    fn html_to_text_ignores_head_content() {
        let html = "<!doctype html><html><head><title>Ignore me</title><style>.note { color: red; }</style></head><body><p>Hello</p></body></html>";

        assert_eq!(aster_forge_mail::html_to_text(html), "Hello");
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

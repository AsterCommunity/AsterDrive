//! `auth` API DTO 定义。

use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::{IntoParams, ToSchema};

use crate::errors::{AsterError, Result};
use crate::services::user::account::{MeResponseField, MeResponseFields};

/// Registration request for new users.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RegisterReq {
    pub username: String,
    pub email: String,
    pub password: String,
}

/// Accept a user invitation by choosing account credentials.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AcceptUserInvitationReq {
    pub username: String,
    pub password: String,
}

/// Resend registration activation email.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ResendRegisterActivationReq {
    pub identifier: String,
}

/// Response for the `/auth/check` endpoint.
#[derive(serde::Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct CheckResp {
    pub has_users: bool,
    pub allow_user_registration: bool,
    pub passkey_login_enabled: bool,
}

/// Initial system setup (first admin account).
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct SetupReq {
    pub username: String,
    pub email: String,
    pub password: String,
}

/// Standard login credentials.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct LoginReq {
    pub identifier: String,
    pub password: String,
}

/// Query parameters for email contact verification confirmation.
#[derive(Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(IntoParams, ToSchema)
)]
pub struct ContactVerificationConfirmQuery {
    pub token: Option<String>,
}

/// Response body for token issuance (login / refresh / password change).
#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AuthTokenResp {
    pub expires_in: u64,
}

#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum LoginResponse {
    Authenticated {
        expires_in: u64,
    },
    PasswordChangeRequired {
        expires_in: u64,
    },
    MfaRequired {
        flow_token: String,
        expires_in: u64,
        methods: Vec<crate::types::MfaMethod>,
    },
}

/// Generic message-only response (used after email operations).
#[derive(serde::Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ActionMessageResp {
    pub message: String,
}

/// Update the user's avatar source.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct UpdateAvatarSourceReq {
    pub source: crate::types::AvatarSource,
}

/// Update display name in user profile.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct UpdateProfileReq {
    pub display_name: Option<String>,
}

/// Change the authenticated user's password.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ChangePasswordReq {
    pub current_password: String,
    pub new_password: String,
}

/// Query parameters for `/auth/me`.
#[derive(Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(IntoParams, ToSchema)
)]
pub struct MeQuery {
    /// Comma-separated field groups to include: profile, preferences, quota, session.
    pub fields: Option<String>,
}

impl MeQuery {
    pub fn selected_fields(&self) -> Result<Option<MeResponseFields>> {
        let Some(fields) = self.fields.as_deref() else {
            return Ok(None);
        };

        let fields = fields.trim();
        if fields.is_empty() {
            return Ok(None);
        }

        let mut selected = Vec::new();
        for raw_field in fields.split(',') {
            let field = raw_field.trim().to_ascii_lowercase();
            if field.is_empty() {
                continue;
            }
            selected.push(match field.as_str() {
                "profile" => MeResponseField::Profile,
                "preferences" => MeResponseField::Preferences,
                "quota" => MeResponseField::Quota,
                "session" => MeResponseField::Session,
                _ => {
                    return Err(AsterError::validation_error(format!(
                        "invalid auth/me field '{raw_field}'"
                    )));
                }
            });
        }

        if selected.is_empty() {
            Ok(None)
        } else {
            Ok(Some(MeResponseFields::from_fields(selected)))
        }
    }
}

/// Request a password reset email.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PasswordResetRequestReq {
    pub email: String,
}

/// Confirm a password reset with the token from the email.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PasswordResetConfirmReq {
    pub token: String,
    pub new_password: String,
}

/// Request an email address change.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RequestEmailChangeReq {
    pub new_email: String,
}

/// Start registering a passkey for the authenticated user.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PasskeyRegisterStartReq {
    pub name: Option<String>,
}

/// Finish registering a passkey for the authenticated user.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PasskeyRegisterFinishReq {
    pub flow_id: String,
    pub credential: serde_json::Value,
    pub name: Option<String>,
}

/// Rename an existing passkey.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PatchPasskeyReq {
    pub name: String,
}

/// Start a passkey login challenge.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PasskeyLoginStartReq {
    pub identifier: Option<String>,
    pub conditional: Option<bool>,
}

/// Finish a passkey login challenge.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PasskeyLoginFinishReq {
    pub flow_id: String,
    pub credential: serde_json::Value,
}

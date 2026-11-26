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
    /// Desired username for the new account.
    pub username: String,
    /// Email address that receives the activation email.
    pub email: String,
    /// Account password in plaintext; stored server-side only as a hash.
    pub password: String,
}

/// Accept a user invitation by choosing account credentials.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AcceptUserInvitationReq {
    /// Username to claim for the invited account.
    pub username: String,
    /// Account password in plaintext; stored server-side only as a hash.
    pub password: String,
}

/// Resend registration activation email.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ResendRegisterActivationReq {
    /// Username or email address of the pending registration.
    pub identifier: String,
}

/// Response for the `/auth/check` endpoint.
#[derive(serde::Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct CheckResp {
    /// Current phase of the initial system setup.
    pub setup_state: crate::services::system_setup::SystemSetupState,
    /// Whether any user account already exists.
    pub has_users: bool,
    /// Whether open registration is currently allowed.
    pub allow_user_registration: bool,
    /// Whether password-less passkey login is enabled.
    pub passkey_login_enabled: bool,
    /// Whether password-based login is enabled.
    pub password_login_enabled: bool,
}

/// Initial system setup (first admin account).
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct SetupReq {
    /// Username for the first administrator account.
    pub username: String,
    /// Email address for the first administrator account.
    pub email: String,
    /// Account password in plaintext; stored server-side only as a hash.
    pub password: String,
}

/// Standard login credentials.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct LoginReq {
    /// Username or email address; values containing `@` are treated as email.
    pub identifier: String,
    /// Account password in plaintext.
    pub password: String,
}

/// Query parameters for email contact verification confirmation.
#[derive(Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(IntoParams, ToSchema)
)]
pub struct ContactVerificationConfirmQuery {
    /// One-time verification token from the emailed link; empty or absent
    /// redirects to the frontend fallback page instead of confirming.
    pub token: Option<String>,
}

/// Response body for token issuance (login / refresh / password change).
#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AuthTokenResp {
    /// Access token lifetime in seconds.
    pub expires_in: u64,
}

/// Login outcome for the password login endpoint, tagged by `status`.
#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum LoginResponse {
    /// Credentials accepted and a session was issued.
    Authenticated {
        /// Access token lifetime in seconds.
        expires_in: u64,
    },
    /// Credentials accepted, but the account must set a new password first;
    /// the issued session is restricted to the password-change flow.
    PasswordChangeRequired {
        /// Access token lifetime in seconds.
        expires_in: u64,
    },
    /// First factor verified; a second-factor challenge must be completed.
    MfaRequired {
        /// Opaque token identifying the pending MFA flow.
        flow_token: String,
        /// Lifetime of the MFA flow in seconds.
        expires_in: u64,
        /// Second-factor methods available for this account.
        methods: Vec<aster_drive_model::types::MfaMethod>,
    },
}

/// Generic message-only response (used after email operations).
#[derive(serde::Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ActionMessageResp {
    /// Human-readable confirmation message.
    pub message: String,
}

/// Update the user's avatar source.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct UpdateAvatarSourceReq {
    /// New avatar source (`none`, `gravatar`, or `upload`).
    pub source: aster_drive_model::types::AvatarSource,
}

/// Update display name in user profile.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct UpdateProfileReq {
    /// New display name; `None` leaves the current name untouched.
    pub display_name: Option<String>,
}

/// Change the authenticated user's password.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ChangePasswordReq {
    /// Current password, for re-authentication.
    pub current_password: String,
    /// New password to set.
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
    /// Parses the comma-separated `fields` value into response field groups.
    ///
    /// Returns `Ok(None)` when the value is absent or trims to nothing;
    /// unknown field names return a validation error.
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
    /// Email address of the account to send the reset link to.
    pub email: String,
}

/// Confirm a password reset with the token from the email.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PasswordResetConfirmReq {
    /// One-time token from the reset email.
    pub token: String,
    /// New password to set.
    pub new_password: String,
}

/// Request an email address change.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RequestEmailChangeReq {
    /// Email address to change to; a verification link is sent to it.
    pub new_email: String,
}

/// Start registering a passkey for the authenticated user.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PasskeyRegisterStartReq {
    /// Display name for the new passkey; `None` falls back to a server
    /// default derived from the account.
    pub name: Option<String>,
}

/// Finish registering a passkey for the authenticated user.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PasskeyRegisterFinishReq {
    /// Flow ID returned by the registration start call.
    pub flow_id: String,
    /// Raw browser WebAuthn attestation response.
    pub credential: serde_json::Value,
    /// Display name override for the passkey; `None` keeps the name chosen
    /// at registration start.
    pub name: Option<String>,
}

/// Rename an existing passkey.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PatchPasskeyReq {
    /// New display name for the passkey.
    pub name: String,
}

/// Start a passkey login challenge.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PasskeyLoginStartReq {
    /// Keep WebAuthn conditional mediation (autofill UI) for the challenge;
    /// defaults to `false`.
    pub conditional: Option<bool>,
}

/// Finish a passkey login challenge.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PasskeyLoginFinishReq {
    /// Flow ID returned by the login start call.
    pub flow_id: String,
    /// Raw browser WebAuthn assertion response.
    pub credential: serde_json::Value,
}

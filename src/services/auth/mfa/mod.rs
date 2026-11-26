//! MFA 业务逻辑。

mod budget;
mod crypto;
mod login_flow;
mod management;
mod recovery_codes;
pub mod totp;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use aster_drive_model::types::{MfaMethod, MfaPersistentFactorMethod};

pub use login_flow::{
    MfaBrowserBinding, MfaChallengeStart, PrimaryLoginCompletion, cleanup_expired_flows,
    complete_primary_login_or_start_mfa, send_email_code, verify_challenge,
};
pub use management::{
    MfaFactorInfo, MfaSensitiveActionRequest, MfaStatus, TotpSetupFinishRequest,
    TotpSetupFinishResponse, TotpSetupStartResponse, delete_factor, get_status,
    regenerate_recovery_codes, reset_user_mfa, start_totp_setup, verify_totp_setup,
};

/// Lifetime of a pending MFA login challenge, in seconds.
const MFA_LOGIN_FLOW_TTL_SECS: u64 = 300;
/// Lifetime of a pending TOTP setup flow, in seconds.
const MFA_SETUP_FLOW_TTL_SECS: u64 = 300;
/// Failed verification attempts allowed per login challenge before it fails.
const MFA_MAX_ATTEMPTS: u32 = 5;
/// Length of a generated login email code.
const EMAIL_CODE_DIGITS: usize = 8;
/// Recovery codes generated per account.
const RECOVERY_CODE_COUNT: usize = 10;
/// Character length of each recovery code.
const RECOVERY_CODE_CHARS: usize = 12;

/// Second-factor method selector on the MFA challenge wire format.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    schema(as = MfaChallengeRequestMethod)
)]
#[serde(rename_all = "snake_case")]
pub enum MfaChallengeMethod {
    /// Time-based one-time code from an enrolled authenticator.
    Totp,
    /// Single-use recovery code.
    RecoveryCode,
    /// One-time code mailed to the verified address for this login.
    EmailCode,
}

impl From<MfaMethod> for MfaChallengeMethod {
    /// Wire-format method from the domain method; a 1:1 rename with no semantic change.
    fn from(value: MfaMethod) -> Self {
        match value {
            MfaMethod::Totp => Self::Totp,
            MfaMethod::RecoveryCode => Self::RecoveryCode,
            MfaMethod::EmailCode => Self::EmailCode,
        }
    }
}

impl From<MfaChallengeMethod> for MfaMethod {
    /// Domain method from the wire-format method; a 1:1 rename with no semantic change.
    fn from(value: MfaChallengeMethod) -> Self {
        match value {
            MfaChallengeMethod::Totp => Self::Totp,
            MfaChallengeMethod::RecoveryCode => Self::RecoveryCode,
            MfaChallengeMethod::EmailCode => Self::EmailCode,
        }
    }
}

/// Result of submitting a second-factor verification.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct MfaChallengeVerifyResponse {
    /// Verification outcome label matching the `LoginResponse` tag values (`"authenticated"`,
    /// `"mfa_required"`, `"password_change_required"`).
    pub status: &'static str,
    /// Remaining challenge lifetime in seconds.
    pub expires_in: u64,
}

/// Second-factor answer to a pending MFA login challenge.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct MfaChallengeVerifyRequest {
    /// Flow token returned when the challenge started.
    pub flow_token: String,
    /// Second-factor method the code was produced with.
    pub method: MfaChallengeMethod,
    /// Verifier code: TOTP digits, recovery code, or emailed login code.
    pub code: String,
}

/// Request to mail a login code for a pending MFA challenge.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct MfaEmailCodeSendRequest {
    /// Flow token of the pending MFA challenge the code belongs to.
    pub flow_token: String,
}

/// Lifetime parameters of a freshly mailed login code.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct MfaEmailCodeSendResponse {
    /// Code validity window in seconds.
    pub expires_in: u64,
    /// Seconds to wait before another code may be requested.
    pub resend_after: u64,
}

fn now_utc() -> chrono::DateTime<Utc> {
    Utc::now()
}

fn persistent_factor_method_label(method: MfaPersistentFactorMethod) -> &'static str {
    match method {
        MfaPersistentFactorMethod::Totp => "totp",
    }
}

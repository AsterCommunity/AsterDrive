//! MFA 业务逻辑。

mod crypto;
mod login_flow;
mod management;
mod recovery_codes;
pub mod totp;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::types::{MfaMethod, MfaPersistentFactorMethod};

pub use login_flow::{
    MfaChallengeStart, PrimaryLoginCompletion, cleanup_expired_flows,
    complete_primary_login_or_start_mfa, send_email_code, verify_challenge,
};
pub use management::{
    MfaFactorInfo, MfaSensitiveActionRequest, MfaStatus, TotpSetupFinishRequest,
    TotpSetupFinishResponse, TotpSetupStartResponse, delete_factor, get_status,
    regenerate_recovery_codes, reset_user_mfa, start_totp_setup, verify_totp_setup,
};

const MFA_LOGIN_FLOW_TTL_SECS: u64 = 300;
const MFA_SETUP_FLOW_TTL_SECS: u64 = 300;
const MFA_MAX_ATTEMPTS: i32 = 5;
const EMAIL_CODE_DIGITS: usize = 8;
const RECOVERY_CODE_COUNT: usize = 10;
const RECOVERY_CODE_CHARS: usize = 12;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    schema(as = MfaChallengeRequestMethod)
)]
#[serde(rename_all = "snake_case")]
pub enum MfaChallengeMethod {
    Totp,
    RecoveryCode,
    EmailCode,
}

impl From<MfaMethod> for MfaChallengeMethod {
    fn from(value: MfaMethod) -> Self {
        match value {
            MfaMethod::Totp => Self::Totp,
            MfaMethod::RecoveryCode => Self::RecoveryCode,
            MfaMethod::EmailCode => Self::EmailCode,
        }
    }
}

impl From<MfaChallengeMethod> for MfaMethod {
    fn from(value: MfaChallengeMethod) -> Self {
        match value {
            MfaChallengeMethod::Totp => Self::Totp,
            MfaChallengeMethod::RecoveryCode => Self::RecoveryCode,
            MfaChallengeMethod::EmailCode => Self::EmailCode,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct MfaChallengeVerifyResponse {
    pub status: &'static str,
    pub expires_in: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct MfaChallengeVerifyRequest {
    pub flow_token: String,
    pub method: MfaChallengeMethod,
    pub code: String,
}

#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct MfaEmailCodeSendRequest {
    pub flow_token: String,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct MfaEmailCodeSendResponse {
    pub expires_in: u64,
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

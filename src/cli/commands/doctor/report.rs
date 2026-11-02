use serde::Serialize;

use super::args::{DoctorArgs, DoctorDeepScope};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DoctorStatus {
    Ok,
    Warn,
    Fail,
}

impl DoctorStatus {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Warn => "warn",
            Self::Fail => "fail",
        }
    }
}

#[derive(Debug, Serialize)]
pub struct DoctorSummary {
    pub(super) total: usize,
    pub(super) ok: usize,
    pub(super) warn: usize,
    pub(super) fail: usize,
}

#[derive(Debug, Serialize)]
pub struct DoctorCheck {
    pub(super) name: &'static str,
    pub(super) label: &'static str,
    pub(super) status: DoctorStatus,
    pub(super) summary: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) details: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) suggestion: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DoctorReport {
    pub(super) strict: bool,
    pub(super) deep: bool,
    pub(super) fix: bool,
    pub(super) dry_run: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) scopes: Vec<DoctorDeepScope>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) policy_id: Option<i64>,
    pub(super) status: DoctorStatus,
    pub(super) database_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) backend: Option<String>,
    pub(super) summary: DoctorSummary,
    pub(super) checks: Vec<DoctorCheck>,
}

impl DoctorReport {
    pub(super) fn new(
        args: &DoctorArgs,
        database_url: String,
        backend: Option<String>,
        deep: bool,
        scopes: Vec<DoctorDeepScope>,
        checks: Vec<DoctorCheck>,
    ) -> Self {
        let mut ok = 0;
        let mut warn = 0;
        let mut fail = 0;
        for check in &checks {
            match check.status {
                DoctorStatus::Ok => ok += 1,
                DoctorStatus::Warn => warn += 1,
                DoctorStatus::Fail => fail += 1,
            }
        }
        let status = if fail > 0 || (args.strict && warn > 0) {
            DoctorStatus::Fail
        } else if warn > 0 {
            DoctorStatus::Warn
        } else {
            DoctorStatus::Ok
        };
        Self {
            strict: args.strict,
            deep,
            fix: args.fix,
            dry_run: args.dry_run,
            scopes,
            policy_id: args.policy_id,
            status,
            database_url,
            backend,
            summary: DoctorSummary {
                total: checks.len(),
                ok,
                warn,
                fail,
            },
            checks,
        }
    }

    pub fn should_exit_nonzero(&self) -> bool {
        self.status == DoctorStatus::Fail
    }
}

pub(super) fn doctor_check(
    name: &'static str,
    label: &'static str,
    status: DoctorStatus,
    summary: impl Into<String>,
    details: Vec<String>,
    suggestion: Option<String>,
) -> DoctorCheck {
    DoctorCheck {
        name,
        label,
        status,
        summary: summary.into(),
        details,
        suggestion,
    }
}

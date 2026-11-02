//! Machine-readable envelopes and terminal-aware output selection.
use crate::errors::AsterError;
use clap::ValueEnum;
use serde::Serialize;
use std::io::{self, IsTerminal};
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    Auto,
    Json,
    PrettyJson,
    Human,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::cli) enum ResolvedOutputFormat {
    Json,
    PrettyJson,
    Human,
}

impl OutputFormat {
    pub(in crate::cli) fn resolve(self) -> ResolvedOutputFormat {
        match self {
            Self::Auto => {
                if io::stdout().is_terminal() {
                    ResolvedOutputFormat::Human
                } else {
                    ResolvedOutputFormat::Json
                }
            }
            Self::Json => ResolvedOutputFormat::Json,
            Self::PrettyJson => ResolvedOutputFormat::PrettyJson,
            Self::Human => ResolvedOutputFormat::Human,
        }
    }
}

#[derive(Debug, Serialize)]
pub(in crate::cli) struct SuccessEnvelope<T> {
    pub ok: bool,
    pub data: T,
}

#[derive(Debug, Serialize)]
pub(in crate::cli) struct ErrorEnvelope<'a> {
    pub ok: bool,
    pub error: ErrorPayload<'a>,
}

#[derive(Debug, Serialize)]
pub(in crate::cli) struct ErrorPayload<'a> {
    pub code: &'a str,
    pub error_type: &'a str,
    pub message: &'a str,
}

pub(in crate::cli) fn render_success_envelope<T>(data: &T, pretty: bool) -> String
where
    T: Serialize,
{
    let envelope = SuccessEnvelope { ok: true, data };
    if pretty {
        serde_json::to_string_pretty(&envelope)
            .unwrap_or_else(|error| render_serialization_error(error, true))
    } else {
        serde_json::to_string(&envelope)
            .unwrap_or_else(|error| render_serialization_error(error, false))
    }
}

pub(in crate::cli) fn render_error_json(err: &AsterError, pretty: bool) -> String {
    let envelope = ErrorEnvelope {
        ok: false,
        error: ErrorPayload {
            code: err.code(),
            error_type: err.error_type(),
            message: err.message(),
        },
    };
    if pretty {
        serde_json::to_string_pretty(&envelope)
            .unwrap_or_else(|error| render_serialization_error(error, true))
    } else {
        serde_json::to_string(&envelope)
            .unwrap_or_else(|error| render_serialization_error(error, false))
    }
}

pub(in crate::cli) fn render_serialization_error(error: serde_json::Error, pretty: bool) -> String {
    let message = format!("failed to serialize CLI envelope: {error}");
    let fallback = ErrorEnvelope {
        ok: false,
        error: ErrorPayload {
            code: "serialization_error",
            error_type: "internal",
            message: &message,
        },
    };
    if pretty {
        serde_json::to_string_pretty(&fallback).unwrap_or_default()
    } else {
        serde_json::to_string(&fallback).unwrap_or_default()
    }
}

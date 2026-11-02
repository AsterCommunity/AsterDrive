/// User cancellation is CLI control flow, separate from business failures.
#[derive(Debug)]
pub(in crate::cli) enum CliError {
    Canceled,
    Failed(crate::errors::AsterError),
}

impl From<crate::errors::AsterError> for CliError {
    fn from(error: crate::errors::AsterError) -> Self {
        Self::Failed(error)
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Canceled => f.write_str("canceled"),
            Self::Failed(error) => std::fmt::Display::fmt(error, f),
        }
    }
}

impl std::error::Error for CliError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Canceled => None,
            Self::Failed(error) => Some(error),
        }
    }
}

pub(in crate::cli) type CliResult<T> = std::result::Result<T, CliError>;

mod audit;
mod ingestion;
mod maintenance;
mod search;
mod workspaces;

use std::fmt::{Display, Formatter};

use knowledge_app::{AppError, AppErrorKind};
use knowledge_domain::DomainError;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

pub use audit::*;
pub use ingestion::*;
pub use maintenance::*;
pub use search::*;
pub use workspaces::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "code", content = "details", rename_all = "snake_case")]
pub enum CommandError {
    InvalidRequest(String),
    NotFound,
    AccessDenied,
    Conflict(String),
    StorageFailure(String),
    IntegrityFailure(String),
}

impl CommandError {
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidRequest(_) => "invalid_request",
            Self::NotFound => "not_found",
            Self::AccessDenied => "access_denied",
            Self::Conflict(_) => "conflict",
            Self::StorageFailure(_) => "storage_failure",
            Self::IntegrityFailure(_) => "integrity_failure",
        }
    }
}

impl Display for CommandError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "command failed: {}", self.code())
    }
}

impl std::error::Error for CommandError {}

impl From<DomainError> for CommandError {
    fn from(_error: DomainError) -> Self {
        Self::InvalidRequest("request validation failed".to_owned())
    }
}

impl From<AppError> for CommandError {
    fn from(error: AppError) -> Self {
        match error.kind() {
            AppErrorKind::InvalidInput => {
                Self::InvalidRequest("request validation failed".to_owned())
            }
            AppErrorKind::NotFound => Self::NotFound,
            AppErrorKind::AccessDenied => Self::AccessDenied,
            AppErrorKind::Conflict => Self::Conflict("local knowledge conflict".to_owned()),
            AppErrorKind::Storage => {
                Self::StorageFailure("local knowledge operation failed".to_owned())
            }
            AppErrorKind::Integrity => {
                Self::IntegrityFailure("local integrity operation failed".to_owned())
            }
        }
    }
}

pub(crate) fn parse_id<T: DeserializeOwned>(value: &str, field: &str) -> Result<T, CommandError> {
    serde_json::from_value(serde_json::Value::String(value.to_owned()))
        .map_err(|_| CommandError::InvalidRequest(format!("invalid {field}")))
}

pub(crate) fn validate_workspace_name(name: &str) -> Result<(), CommandError> {
    if name.chars().any(char::is_control) {
        return Err(CommandError::InvalidRequest(
            "workspace name contains control characters".to_owned(),
        ));
    }
    DomainError::workspace_name(name)
        .map(|_| ())
        .map_err(|_| CommandError::InvalidRequest("invalid workspace name".to_owned()))
}

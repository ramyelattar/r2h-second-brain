use knowledge_app::{AuditVerificationFailure, AuditVerificationResult};
use knowledge_domain::{AuditActor, AuditEvent};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuditListRequest {
    pub workspace_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IntegrityVerifyRequest {
    pub workspace_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditActorDto {
    User,
    System,
    Maintenance,
}

impl From<AuditActor> for AuditActorDto {
    fn from(actor: AuditActor) -> Self {
        match actor {
            AuditActor::User => Self::User,
            AuditActor::System => Self::System,
            AuditActor::Maintenance => Self::Maintenance,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEventDto {
    pub id: String,
    pub workspace_id: String,
    pub sequence: u64,
    pub event_type: String,
    pub actor: AuditActorDto,
    pub occurred_at_utc: String,
    pub event_hash: String,
}

impl From<AuditEvent> for AuditEventDto {
    fn from(event: AuditEvent) -> Self {
        Self {
            id: event.id.to_string(),
            workspace_id: event.workspace_id.to_string(),
            sequence: event.sequence,
            event_type: event.event_type,
            actor: event.actor.into(),
            occurred_at_utc: event.occurred_at_utc.to_rfc3339(),
            event_hash: event.event_hash,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IntegrityFailureKindDto {
    InvalidPayload,
    PreviousHashMismatch,
    EventHashMismatch,
    WorkspaceMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrityFailureDto {
    pub kind: IntegrityFailureKindDto,
    pub sequence: u64,
}

impl From<AuditVerificationFailure> for IntegrityFailureDto {
    fn from(failure: AuditVerificationFailure) -> Self {
        match failure {
            AuditVerificationFailure::InvalidPayload { sequence } => Self {
                kind: IntegrityFailureKindDto::InvalidPayload,
                sequence,
            },
            AuditVerificationFailure::PreviousHashMismatch { sequence } => Self {
                kind: IntegrityFailureKindDto::PreviousHashMismatch,
                sequence,
            },
            AuditVerificationFailure::EventHashMismatch { sequence } => Self {
                kind: IntegrityFailureKindDto::EventHashMismatch,
                sequence,
            },
            AuditVerificationFailure::WorkspaceMismatch { sequence } => Self {
                kind: IntegrityFailureKindDto::WorkspaceMismatch,
                sequence,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrityVerificationDto {
    pub workspace_id: String,
    pub event_count: usize,
    pub is_valid: bool,
    pub first_failure: Option<IntegrityFailureDto>,
}

impl From<AuditVerificationResult> for IntegrityVerificationDto {
    fn from(result: AuditVerificationResult) -> Self {
        Self {
            workspace_id: result.workspace_id.to_string(),
            event_count: result.event_count,
            is_valid: result.is_valid,
            first_failure: result.first_failure.map(Into::into),
        }
    }
}

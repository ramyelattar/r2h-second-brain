use knowledge_app::{
    BackupManifest, BlobMaintenanceReport, IntegrityFailure, IntegrityMode, RestoreReport,
    WorkspaceIntegrityResult,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateBackupRequest {
    pub workspace_id: String,
    pub destination: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntegrityModeDto {
    Quick,
    Full,
}

impl From<IntegrityModeDto> for IntegrityMode {
    fn from(value: IntegrityModeDto) -> Self {
        match value {
            IntegrityModeDto::Quick => Self::Quick,
            IntegrityModeDto::Full => Self::Full,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerifyWorkspaceIntegrityRequest {
    pub workspace_id: String,
    pub mode: IntegrityModeDto,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScanOrphanBlobsRequest {
    pub workspace_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestoreWorkspaceBackupRequest {
    pub backup: String,
    pub new_workspace_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupManifestDto {
    pub format_version: u32,
    pub source_workspace_id: String,
    pub created_utc: String,
    pub database_sha256: String,
    pub blob_count: u64,
    pub blob_hashes: Vec<String>,
    pub application_version: String,
}

impl From<BackupManifest> for BackupManifestDto {
    fn from(value: BackupManifest) -> Self {
        Self {
            format_version: value.format_version,
            source_workspace_id: value.source_workspace_id.to_string(),
            created_utc: value.created_utc.to_rfc3339(),
            database_sha256: value.database_sha256,
            blob_count: value.blob_count,
            blob_hashes: value.blob_hashes,
            application_version: value.application_version,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceIntegrityFailureKindDto {
    Schema,
    ForeignKeys,
    FtsCount,
    AuditChain,
    MissingBlob,
    BlobHash,
    BlockHash,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceIntegrityFailureDto {
    pub kind: WorkspaceIntegrityFailureKindDto,
    pub affected_id: Option<String>,
}

impl From<IntegrityFailure> for WorkspaceIntegrityFailureDto {
    fn from(value: IntegrityFailure) -> Self {
        match value {
            IntegrityFailure::Schema => Self::plain(WorkspaceIntegrityFailureKindDto::Schema),
            IntegrityFailure::ForeignKeys => {
                Self::plain(WorkspaceIntegrityFailureKindDto::ForeignKeys)
            }
            IntegrityFailure::FtsCount => Self::plain(WorkspaceIntegrityFailureKindDto::FtsCount),
            IntegrityFailure::AuditChain => {
                Self::plain(WorkspaceIntegrityFailureKindDto::AuditChain)
            }
            IntegrityFailure::MissingBlob { hash } => {
                Self::affected(WorkspaceIntegrityFailureKindDto::MissingBlob, hash)
            }
            IntegrityFailure::BlobHash { hash } => {
                Self::affected(WorkspaceIntegrityFailureKindDto::BlobHash, hash)
            }
            IntegrityFailure::BlockHash { block_id } => {
                Self::affected(WorkspaceIntegrityFailureKindDto::BlockHash, block_id)
            }
        }
    }
}

impl WorkspaceIntegrityFailureDto {
    fn plain(kind: WorkspaceIntegrityFailureKindDto) -> Self {
        Self {
            kind,
            affected_id: None,
        }
    }

    fn affected(kind: WorkspaceIntegrityFailureKindDto, affected_id: String) -> Self {
        Self {
            kind,
            affected_id: Some(affected_id),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceIntegrityDto {
    pub workspace_id: String,
    pub mode: IntegrityModeDto,
    pub schema_version: u32,
    pub referenced_blob_count: u64,
    pub verified_blob_count: u64,
    pub block_count: u64,
    pub is_valid: bool,
    pub first_failure: Option<WorkspaceIntegrityFailureDto>,
}

impl From<WorkspaceIntegrityResult> for WorkspaceIntegrityDto {
    fn from(value: WorkspaceIntegrityResult) -> Self {
        Self {
            workspace_id: value.workspace_id.to_string(),
            mode: match value.mode {
                IntegrityMode::Quick => IntegrityModeDto::Quick,
                IntegrityMode::Full => IntegrityModeDto::Full,
            },
            schema_version: value.schema_version,
            referenced_blob_count: value.referenced_blob_count,
            verified_blob_count: value.verified_blob_count,
            block_count: value.block_count,
            is_valid: value.is_valid,
            first_failure: value.first_failure.map(Into::into),
        }
    }
}

impl Serialize for IntegrityModeDto {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(match self {
            Self::Quick => "quick",
            Self::Full => "full",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrphanBlobsDto {
    pub hashes: Vec<String>,
}

impl From<BlobMaintenanceReport> for OrphanBlobsDto {
    fn from(value: BlobMaintenanceReport) -> Self {
        Self {
            hashes: value
                .orphan_hashes
                .into_iter()
                .map(|hash| hash.as_str().to_owned())
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreReportDto {
    pub workspace_id: String,
    pub source_count: u64,
    pub document_count: u64,
    pub document_version_count: u64,
    pub content_block_count: u64,
    pub blob_count: u64,
}

impl From<RestoreReport> for RestoreReportDto {
    fn from(value: RestoreReport) -> Self {
        Self {
            workspace_id: value.workspace_id.to_string(),
            source_count: value.source_count,
            document_count: value.document_count,
            document_version_count: value.document_version_count,
            content_block_count: value.content_block_count,
            blob_count: value.blob_count,
        }
    }
}

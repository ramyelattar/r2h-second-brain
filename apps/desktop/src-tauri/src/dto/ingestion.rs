use knowledge_domain::{DocumentVersion, DocumentVersionStatus, Source, SourceKind};
use serde::{Deserialize, Serialize};

use crate::CommandError;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IngestFilesRequest {
    pub workspace_id: String,
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceListRequest {
    pub workspace_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceGetRequest {
    pub workspace_id: String,
    pub source_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKindDto {
    PlainText,
    Markdown,
    Json,
    SourceCode,
    BuildLog,
    PdfText,
}

impl From<SourceKind> for SourceKindDto {
    fn from(kind: SourceKind) -> Self {
        match kind {
            SourceKind::PlainText => Self::PlainText,
            SourceKind::Markdown => Self::Markdown,
            SourceKind::Json => Self::Json,
            SourceKind::SourceCode => Self::SourceCode,
            SourceKind::BuildLog => Self::BuildLog,
            SourceKind::PdfText => Self::PdfText,
        }
    }
}

impl From<SourceKindDto> for SourceKind {
    fn from(kind: SourceKindDto) -> Self {
        match kind {
            SourceKindDto::PlainText => Self::PlainText,
            SourceKindDto::Markdown => Self::Markdown,
            SourceKindDto::Json => Self::Json,
            SourceKindDto::SourceCode => Self::SourceCode,
            SourceKindDto::BuildLog => Self::BuildLog,
            SourceKindDto::PdfText => Self::PdfText,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentVersionDto {
    pub id: String,
    pub content_sha256: String,
    pub parser_id: String,
    pub parser_version: String,
    pub status: DocumentVersionStatusDto,
    pub created_at_utc: String,
    pub failure_code: Option<String>,
    pub failure_message: Option<String>,
}

impl From<DocumentVersion> for DocumentVersionDto {
    fn from(version: DocumentVersion) -> Self {
        Self {
            id: version.id.to_string(),
            content_sha256: version.content_sha256.as_str().to_owned(),
            parser_id: version.parser_id,
            parser_version: version.parser_version,
            status: version.status.into(),
            created_at_utc: version.created_at_utc.to_rfc3339(),
            failure_code: version.failure_code,
            failure_message: version.failure_message,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentVersionStatusDto {
    Processing,
    Ready,
    Failed,
}

impl From<DocumentVersionStatus> for DocumentVersionStatusDto {
    fn from(status: DocumentVersionStatus) -> Self {
        match status {
            DocumentVersionStatus::Processing => Self::Processing,
            DocumentVersionStatus::Ready => Self::Ready,
            DocumentVersionStatus::Failed => Self::Failed,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceDto {
    pub id: String,
    pub workspace_id: String,
    pub kind: SourceKindDto,
    pub display_name: String,
    pub canonical_locator: String,
    pub created_at_utc: String,
    pub last_seen_at_utc: String,
    pub versions: Vec<DocumentVersionDto>,
}

impl SourceDto {
    #[must_use]
    pub fn new(source: Source, versions: Vec<DocumentVersion>) -> Self {
        Self {
            id: source.id.to_string(),
            workspace_id: source.workspace_id.to_string(),
            kind: source.kind.into(),
            display_name: source.display_name,
            canonical_locator: source.canonical_locator,
            created_at_utc: source.created_at_utc.to_rfc3339(),
            last_seen_at_utc: source.last_seen_at_utc.to_rfc3339(),
            versions: versions.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IngestStatusDto {
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestFileResultDto {
    pub path: String,
    pub status: IngestStatusDto,
    pub source_id: Option<String>,
    pub document_version_id: Option<String>,
    pub error: Option<CommandError>,
}

use knowledge_domain::{Citation, SearchHit, SourceSpan};
use serde::{Deserialize, Serialize};

use crate::{CommandError, SourceKindDto, parse_id};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchRequestDto {
    pub workspace_id: String,
    pub query: String,
    #[serde(default)]
    pub source_kinds: Vec<SourceKindDto>,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CitationResolveRequest {
    pub workspace_id: String,
    pub hit: SearchHitDto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceSpanDto {
    Lines {
        #[serde(rename = "startLine")]
        start_line: u32,
        #[serde(rename = "endLine")]
        end_line: u32,
    },
    PdfPage {
        page: u32,
        #[serde(rename = "startChar")]
        start_char: u32,
        #[serde(rename = "endChar")]
        end_char: u32,
    },
}

impl From<SourceSpan> for SourceSpanDto {
    fn from(span: SourceSpan) -> Self {
        match span {
            SourceSpan::Lines {
                start_line,
                end_line,
            } => Self::Lines {
                start_line,
                end_line,
            },
            SourceSpan::PdfPage {
                page,
                start_char,
                end_char,
            } => Self::PdfPage {
                page,
                start_char,
                end_char,
            },
        }
    }
}

impl TryFrom<SourceSpanDto> for SourceSpan {
    type Error = CommandError;

    fn try_from(span: SourceSpanDto) -> Result<Self, Self::Error> {
        match span {
            SourceSpanDto::Lines {
                start_line,
                end_line,
            } => SourceSpan::lines(start_line, end_line),
            SourceSpanDto::PdfPage {
                page,
                start_char,
                end_char,
            } => SourceSpan::pdf_page(page, start_char, end_char),
        }
        .map_err(|_| CommandError::InvalidRequest("invalid source span".to_owned()))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchHitDto {
    pub block_id: String,
    pub document_version_id: String,
    pub source_id: String,
    pub score: f64,
    pub snippet: String,
    pub span: SourceSpanDto,
}

impl From<SearchHit> for SearchHitDto {
    fn from(hit: SearchHit) -> Self {
        Self {
            block_id: hit.block_id.to_string(),
            document_version_id: hit.document_version_id.to_string(),
            source_id: hit.source_id.to_string(),
            score: hit.score,
            snippet: hit.snippet,
            span: hit.span.into(),
        }
    }
}

impl TryFrom<SearchHitDto> for SearchHit {
    type Error = CommandError;

    fn try_from(hit: SearchHitDto) -> Result<Self, Self::Error> {
        SearchHit::new(
            parse_id(&hit.block_id, "block ID")?,
            parse_id(&hit.document_version_id, "document version ID")?,
            parse_id(&hit.source_id, "source ID")?,
            hit.score,
            hit.snippet,
            hit.span.try_into()?,
        )
        .map_err(|_| CommandError::InvalidRequest("invalid search hit".to_owned()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationDto {
    pub source_id: String,
    pub display_name: String,
    pub canonical_locator: String,
    pub content_sha256: String,
    pub span: SourceSpanDto,
    pub excerpt: String,
}

impl From<Citation> for CitationDto {
    fn from(citation: Citation) -> Self {
        Self {
            source_id: citation.source_id.to_string(),
            display_name: citation.display_name,
            canonical_locator: citation.canonical_locator,
            content_sha256: citation.content_sha256.as_str().to_owned(),
            span: citation.span.into(),
            excerpt: citation.excerpt,
        }
    }
}

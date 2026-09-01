from __future__ import annotations

from pathlib import Path


ROOT = Path(r"E:\Projects\r2h-second-brain")

DOMAIN = ROOT / "crates" / "knowledge-domain" / "src" / "search.rs"
DB_REPO = ROOT / "crates" / "knowledge-db" / "src" / "repositories" / "content_blocks.rs"
SEARCH_SERVICE = ROOT / "crates" / "knowledge-search" / "src" / "service.rs"
APP_SERVICES = ROOT / "crates" / "knowledge-app" / "src" / "services.rs"
RAG = ROOT / "apps" / "desktop" / "src-tauri" / "src" / "rag.rs"
SEARCH_TESTS = ROOT / "crates" / "knowledge-search" / "tests" / "search.rs"

FILES = [
    DOMAIN,
    DB_REPO,
    SEARCH_SERVICE,
    APP_SERVICES,
    RAG,
    SEARCH_TESTS,
]

for path in FILES:
    if not path.is_file():
        raise RuntimeError(f"Required file not found: {path}")


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8-sig")


def write(path: Path, value: str) -> None:
    backup = path.with_name(path.name + ".before-rag-1c1")

    if not backup.exists():
        backup.write_text(
            read(path),
            encoding="utf-8",
            newline="\n",
        )

    path.write_text(
        value,
        encoding="utf-8",
        newline="\n",
    )


def replace_once(
    text: str,
    old: str,
    new: str,
    label: str,
) -> str:
    count = text.count(old)

    if count != 1:
        raise RuntimeError(
            f"{label}: expected exactly one match, found {count}"
        )

    return text.replace(old, new, 1)


# ============================================================
# 1. Database repository: scoped FTS query
# ============================================================

db = read(DB_REPO)

if "const SCOPED_SEARCH_QUERY" not in db:
    marker = '''const SEARCH_QUERY: &str = "SELECT blocks.id, blocks.document_version_id, blocks.source_id, blocks.ordinal, blocks.body, blocks.span_json, bm25(content_blocks_fts) \\
 FROM content_blocks_fts \\
 CROSS JOIN content_blocks AS blocks ON blocks.id = content_blocks_fts.block_id \\
 JOIN document_versions AS versions ON versions.id = blocks.document_version_id AND versions.workspace_id = blocks.workspace_id \\
 JOIN sources ON sources.id = blocks.source_id AND sources.workspace_id = blocks.workspace_id \\
 JOIN workspaces ON workspaces.id = blocks.workspace_id \\
   WHERE content_blocks_fts.workspace_id = ?1 AND blocks.workspace_id = ?1 AND versions.workspace_id = ?1 AND sources.workspace_id = ?1 \\
   AND content_blocks_fts MATCH ?2 AND (?3 = 0 OR sources.kind IN (?4, ?5, ?6, ?7, ?8, ?9)) \\
   AND versions.processing_status = 'ready' AND sources.deleted_at_utc IS NULL AND workspaces.archived_at_utc IS NULL \\
 ORDER BY bm25(content_blocks_fts) ASC, sources.id ASC, blocks.ordinal ASC, blocks.id ASC LIMIT ?10";'''

    addition = marker + '''

const SCOPED_SEARCH_QUERY: &str = "SELECT blocks.id, blocks.document_version_id, blocks.source_id, blocks.ordinal, blocks.body, blocks.span_json, bm25(content_blocks_fts) \\
 FROM content_blocks_fts \\
 CROSS JOIN content_blocks AS blocks ON blocks.id = content_blocks_fts.block_id \\
 JOIN document_versions AS versions ON versions.id = blocks.document_version_id AND versions.workspace_id = blocks.workspace_id \\
 JOIN sources ON sources.id = blocks.source_id AND sources.workspace_id = blocks.workspace_id \\
 JOIN workspaces ON workspaces.id = blocks.workspace_id \\
   WHERE content_blocks_fts.workspace_id = ?1 AND blocks.workspace_id = ?1 AND versions.workspace_id = ?1 AND sources.workspace_id = ?1 \\
   AND content_blocks_fts MATCH ?2 AND (?3 = 0 OR sources.kind IN (?4, ?5, ?6, ?7, ?8, ?9)) \\
   AND sources.id IN (SELECT value FROM json_each(?10)) \\
   AND versions.processing_status = 'ready' AND sources.deleted_at_utc IS NULL AND workspaces.archived_at_utc IS NULL \\
 ORDER BY bm25(content_blocks_fts) ASC, sources.id ASC, blocks.ordinal ASC, blocks.id ASC LIMIT ?11";'''

    db = replace_once(
        db,
        marker,
        addition,
        "add scoped SQL query",
    )

db = replace_once(
    db,
    '''    pub fn search_with_metrics(
        &self,
        workspace_id: WorkspaceId,
        fts_query: &str,
        source_kinds: &[SourceKind],
        limit: u32,
    ) -> Result<(Vec<SearchBlock>, ContentBlockSearchMetrics), DbError> {
        self.search_with_query(workspace_id, fts_query, source_kinds, limit, SEARCH_QUERY)
    }

    fn search_with_query(
        &self,
        workspace_id: WorkspaceId,
        fts_query: &str,
        source_kinds: &[SourceKind],
        limit: u32,
        query: &str,
    ) -> Result<(Vec<SearchBlock>, ContentBlockSearchMetrics), DbError> {''',
    '''    pub fn search_with_metrics(
        &self,
        workspace_id: WorkspaceId,
        fts_query: &str,
        source_kinds: &[SourceKind],
        limit: u32,
    ) -> Result<(Vec<SearchBlock>, ContentBlockSearchMetrics), DbError> {
        self.search_with_query(
            workspace_id,
            fts_query,
            source_kinds,
            &[],
            limit,
            SEARCH_QUERY,
        )
    }

    pub fn search_scoped_with_metrics(
        &self,
        workspace_id: WorkspaceId,
        fts_query: &str,
        source_kinds: &[SourceKind],
        source_ids: &[SourceId],
        limit: u32,
    ) -> Result<(Vec<SearchBlock>, ContentBlockSearchMetrics), DbError> {
        if source_ids.is_empty() {
            return self.search_with_metrics(
                workspace_id,
                fts_query,
                source_kinds,
                limit,
            );
        }

        self.search_with_query(
            workspace_id,
            fts_query,
            source_kinds,
            source_ids,
            limit,
            SCOPED_SEARCH_QUERY,
        )
    }

    fn search_with_query(
        &self,
        workspace_id: WorkspaceId,
        fts_query: &str,
        source_kinds: &[SourceKind],
        source_ids: &[SourceId],
        limit: u32,
        query: &str,
    ) -> Result<(Vec<SearchBlock>, ContentBlockSearchMetrics), DbError> {''',
    "extend repository search methods",
)

old_params = '''            let execution_started = Instant::now();
            let mut rows = statement.query(params![
                workspace_id.to_string(),
                fts_query,
                i64::try_from(source_kinds.len()).map_err(|_| DbError::InvalidByteLength)?,
                kinds[0],
                kinds[1],
                kinds[2],
                kinds[3],
                kinds[4],
                kinds[5],
                i64::from(limit)
            ])?;'''

new_params = '''            let execution_started = Instant::now();

            let source_ids_json = serde_json::to_string(
                &source_ids
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
            )
            .map_err(json_error)?;

            let mut rows = if source_ids.is_empty() {
                statement.query(params![
                    workspace_id.to_string(),
                    fts_query,
                    i64::try_from(source_kinds.len())
                        .map_err(|_| DbError::InvalidByteLength)?,
                    kinds[0],
                    kinds[1],
                    kinds[2],
                    kinds[3],
                    kinds[4],
                    kinds[5],
                    i64::from(limit)
                ])?
            } else {
                statement.query(params![
                    workspace_id.to_string(),
                    fts_query,
                    i64::try_from(source_kinds.len())
                        .map_err(|_| DbError::InvalidByteLength)?,
                    kinds[0],
                    kinds[1],
                    kinds[2],
                    kinds[3],
                    kinds[4],
                    kinds[5],
                    source_ids_json,
                    i64::from(limit)
                ])?
            };'''

db = replace_once(
    db,
    old_params,
    new_params,
    "bind scoped source IDs",
)

# Existing internal performance test calls search_with_query directly.
db = db.replace(
    '''            .search_with_query(
                fixture.workspace_id,
                "\\"term\\"",
                &[],
                10,
                &baseline_query,
            )?;''',
    '''            .search_with_query(
                fixture.workspace_id,
                "\\"term\\"",
                &[],
                &[],
                10,
                &baseline_query,
            )?;''',
)

write(DB_REPO, db)


# ============================================================
# 2. Search service: source-scoped API
# ============================================================

service = read(SEARCH_SERVICE)

service = replace_once(
    service,
    '''    pub fn search(&self, request: &SearchRequest) -> Result<Vec<SearchHit>, SearchError> {
        self.search_with_metrics(request).map(|(hits, _)| hits)
    }

    pub fn search_with_metrics(
        &self,
        request: &SearchRequest,
    ) -> Result<(Vec<SearchHit>, SearchMetrics), SearchError> {''',
    '''    pub fn search(&self, request: &SearchRequest) -> Result<Vec<SearchHit>, SearchError> {
        self.search_with_metrics(request).map(|(hits, _)| hits)
    }

    pub fn search_scoped(
        &self,
        request: &SearchRequest,
        source_ids: &[knowledge_domain::SourceId],
    ) -> Result<Vec<SearchHit>, SearchError> {
        self.search_scoped_with_metrics(request, source_ids)
            .map(|(hits, _)| hits)
    }

    pub fn search_with_metrics(
        &self,
        request: &SearchRequest,
    ) -> Result<(Vec<SearchHit>, SearchMetrics), SearchError> {
        self.search_scoped_with_metrics(request, &[])
    }

    pub fn search_scoped_with_metrics(
        &self,
        request: &SearchRequest,
        source_ids: &[knowledge_domain::SourceId],
    ) -> Result<(Vec<SearchHit>, SearchMetrics), SearchError> {''',
    "add SearchService scoped API",
)

service = replace_once(
    service,
    '''        let (rows, database_metrics) = ContentBlockRepository::new(self.database)
            .search_with_metrics(
                request.workspace_id,
                &query,
                &request.source_kinds,
                request.limit,
            )
            .map_err(SearchError::Database)?;''',
    '''        let (rows, database_metrics) = ContentBlockRepository::new(self.database)
            .search_scoped_with_metrics(
                request.workspace_id,
                &query,
                &request.source_kinds,
                source_ids,
                request.limit,
            )
            .map_err(SearchError::Database)?;''',
    "use repository scoped search",
)

write(SEARCH_SERVICE, service)


# ============================================================
# 3. KnowledgeCore facade
# ============================================================

app_services = read(APP_SERVICES)

app_services = replace_once(
    app_services,
    '''    pub fn search(&self, request: &SearchRequest) -> Result<Vec<SearchHit>, AppError> {
        SearchService::new(self.database())
            .search(request)
            .map_err(AppError::Search)
    }

    pub fn resolve_citation(''',
    '''    pub fn search(&self, request: &SearchRequest) -> Result<Vec<SearchHit>, AppError> {
        SearchService::new(self.database())
            .search(request)
            .map_err(AppError::Search)
    }

    pub fn search_scoped(
        &self,
        request: &SearchRequest,
        source_ids: &[SourceId],
    ) -> Result<Vec<SearchHit>, AppError> {
        SearchService::new(self.database())
            .search_scoped(request, source_ids)
            .map_err(AppError::Search)
    }

    pub fn resolve_citation(''',
    "add KnowledgeCore scoped search",
)

write(APP_SERVICES, app_services)


# ============================================================
# 4. Desktop RAG: validate selected sources and use scoped API
# ============================================================

rag = read(RAG)

rag = replace_once(
    rag,
    '''use serde::{Deserialize, Serialize};

use crate::{''',
    '''use knowledge_domain::{SearchRequest, SourceId};
use serde::{Deserialize, Serialize};

use crate::{''',
    "import domain search types",
)

rag = replace_once(
    rag,
    '''    AppState, CitationResolveRequest, SearchRequestDto,
    chat_bridge::ChatSendRequest,
    commands::search::{citation_resolve, search_execute},
};''',
    '''    AppState, CitationResolveRequest,
    chat_bridge::ChatSendRequest,
    commands::{require_workspace, search::citation_resolve},
    parse_id,
};''',
    "replace desktop search imports",
)

old_retrieval = '''    let search_hits = search_execute(
        state,
        SearchRequestDto {
            workspace_id: workspace_id.to_owned(),
            query: request.query.trim().to_owned(),
            source_kinds: Vec::new(),
            limit: CANDIDATE_LIMIT,
        },
    )
    .map_err(|error| format!("Workspace retrieval failed: {error:?}"))?;

    let candidate_count = search_hits.len();

    let selected_source_ids = request
        .selected_source_ids
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();

    let filtered_hits = search_hits
        .into_iter()
        .filter(|hit| {
            selected_source_ids.is_empty()
                || selected_source_ids
                    .iter()
                    .any(|source_id| *source_id == hit.source_id)
        })
        .take(FINAL_EVIDENCE_LIMIT)
        .collect::<Vec<_>>();'''

new_retrieval = '''    let workspace_id_value = require_workspace(
        state,
        workspace_id,
        true,
    )
    .map_err(|error| {
        format!("Workspace validation failed: {error:?}")
    })?;

    let mut selected_source_ids = Vec::<SourceId>::new();

    for value in request
        .selected_source_ids
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        let source_id: SourceId = parse_id(value, "source ID")
            .map_err(|error| {
                format!("Invalid selected source: {error:?}")
            })?;

        let source = state
            .core()
            .source(workspace_id_value, source_id)
            .map_err(|error| {
                format!("Source validation failed: {error}")
            })?
            .ok_or_else(|| {
                "Selected source does not belong to the workspace"
                    .to_owned()
            })?;

        if source.workspace_id != workspace_id_value {
            return Err(
                "Selected source does not belong to the workspace"
                    .to_owned(),
            );
        }

        if !selected_source_ids.contains(&source_id) {
            selected_source_ids.push(source_id);
        }
    }

    selected_source_ids.sort_by_key(ToString::to_string);

    let search_request = SearchRequest::new(
        workspace_id_value,
        request.query.trim(),
        Vec::new(),
        CANDIDATE_LIMIT,
    )
    .map_err(|error| {
        format!("Invalid workspace search request: {error}")
    })?;

    let search_hits = state
        .core()
        .search_scoped(
            &search_request,
            &selected_source_ids,
        )
        .map_err(|error| {
            format!("Workspace retrieval failed: {error}")
        })?;

    let candidate_count = search_hits.len();

    let filtered_hits = search_hits
        .into_iter()
        .take(FINAL_EVIDENCE_LIMIT)
        .map(Into::into)
        .collect::<Vec<crate::SearchHitDto>>();'''

rag = replace_once(
    rag,
    old_retrieval,
    new_retrieval,
    "move source filtering into Knowledge Core",
)

write(RAG, rag)


# ============================================================
# 5. Search integration tests
# ============================================================

tests = read(SEARCH_TESTS)

if "source_scoped_search_excludes_other_sources_before_result_construction" not in tests:
    tests += r'''

#[test]
fn source_scoped_search_excludes_other_sources_before_result_construction(
) -> Result<(), Box<dyn std::error::Error>> {
    let directory = TempDir::new()?;
    let database =
        KnowledgeDb::open(&directory.path().join("knowledge.sqlite"))?;

    let workspace =
        WorkspaceRepository::new(&database).create("Scoped")?;

    let source_a =
        SourceRepository::new(&database).register_or_touch(
            &RegisterSourceRecord {
                workspace_id: workspace.id,
                kind: SourceKind::PlainText,
                canonical_locator: "file:///a.txt".to_owned(),
                display_name: "a.txt".to_owned(),
                observed_at_utc: Utc::now(),
            },
        )?;

    let source_b =
        SourceRepository::new(&database).register_or_touch(
            &RegisterSourceRecord {
                workspace_id: workspace.id,
                kind: SourceKind::PlainText,
                canonical_locator: "file:///b.txt".to_owned(),
                display_name: "b.txt".to_owned(),
                observed_at_utc: Utc::now(),
            },
        )?;

    for (source_id, hash, body) in [
        (
            source_a.source.id,
            "a".repeat(64),
            "shared scoped evidence from alpha",
        ),
        (
            source_b.source.id,
            "b".repeat(64),
            "shared scoped evidence from beta",
        ),
    ] {
        let version =
            DocumentRepository::new(&database)
                .create_processing_version(
                    &CreateVersionRequest {
                        workspace_id: workspace.id,
                        source_id,
                        content_sha256:
                            ContentHash::parse(hash)?,
                        byte_length: body.len() as u64,
                        parser_id: "test".to_owned(),
                        parser_version: "1".to_owned(),
                        media_type: "text/plain".to_owned(),
                        ingested_at_utc: Utc::now(),
                    },
                )?;

        SearchService::new(&database).index_document_blocks(
            workspace.id,
            version.version.id,
            &[ProcessedBlock {
                ordinal: 0,
                heading_path: vec!["Scope".to_owned()],
                body: body.to_owned(),
                normalized_body: normalize_for_search(body),
                span: SourceSpan::lines(1, 1)?,
                body_sha256:
                    ContentHash::parse("c".repeat(64))?,
            }],
        )?;
    }

    let request = SearchRequest::new(
        workspace.id,
        "shared scoped evidence",
        Vec::new(),
        10,
    )?;

    let hits = SearchService::new(&database)
        .search_scoped(&request, &[source_b.source.id])?;

    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].source_id, source_b.source.id);
    assert!(hits[0].snippet.contains("beta"));

    Ok(())
}

#[test]
fn empty_source_scope_preserves_normal_workspace_search(
) -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;

    let (workspace_id, _) = fixture.index(
        "Empty scope",
        "file:///scope.txt",
        "normal empty scope behavior",
    )?;

    let request = SearchRequest::new(
        workspace_id,
        "normal empty scope",
        Vec::new(),
        10,
    )?;

    let normal =
        SearchService::new(&fixture.database).search(&request)?;

    let scoped = SearchService::new(&fixture.database)
        .search_scoped(&request, &[])?;

    assert_eq!(normal, scoped);

    Ok(())
}
'''

write(SEARCH_TESTS, tests)

print("RAG_1C1_SOURCE_FILTERING_WRITTEN")
print("Modified:")
for path in [
    DB_REPO,
    SEARCH_SERVICE,
    APP_SERVICES,
    RAG,
    SEARCH_TESTS,
]:
    print(f"  {path}")

from __future__ import annotations

from pathlib import Path


ROOT = Path(r"E:\Projects\r2h-second-brain")

MIGRATION = (
    ROOT
    / "crates"
    / "knowledge-db"
    / "migrations"
    / "0004_rag_conversations.sql"
)

MIGRATIONS = (
    ROOT
    / "crates"
    / "knowledge-db"
    / "src"
    / "migrations.rs"
)

MIGRATION_TESTS = (
    ROOT
    / "crates"
    / "knowledge-db"
    / "tests"
    / "migrations.rs"
)

APP_SERVICES = (
    ROOT
    / "crates"
    / "knowledge-app"
    / "src"
    / "services.rs"
)

CHAT_BRIDGE = (
    ROOT
    / "apps"
    / "desktop"
    / "src-tauri"
    / "src"
    / "chat_bridge.rs"
)

RAG = (
    ROOT
    / "apps"
    / "desktop"
    / "src-tauri"
    / "src"
    / "rag.rs"
)

LIB = (
    ROOT
    / "apps"
    / "desktop"
    / "src-tauri"
    / "src"
    / "lib.rs"
)

APP = (
    ROOT
    / "apps"
    / "desktop"
    / "src"
    / "App.tsx"
)

CONTRACTS = (
    ROOT
    / "apps"
    / "desktop"
    / "src"
    / "api"
    / "contracts.ts"
)

FILES = [
    MIGRATIONS,
    MIGRATION_TESTS,
    APP_SERVICES,
    CHAT_BRIDGE,
    RAG,
    LIB,
    APP,
    CONTRACTS,
]

for path in FILES:
    if not path.is_file():
        raise RuntimeError(f"Required file is missing: {path}")


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8-sig")


def write(path: Path, value: str) -> None:
    backup = path.with_name(path.name + ".before-rag-1c2")

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
# 1. Migration
# ============================================================

MIGRATION.write_text(
    """CREATE TABLE rag_conversations (
    conversation_id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    mode TEXT NOT NULL CHECK(
        mode IN ('ask_workspace', 'evidence_only')
    ),
    selected_source_ids_json TEXT NOT NULL,
    created_at_utc TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL,
    FOREIGN KEY(workspace_id) REFERENCES workspaces(id)
);

CREATE INDEX idx_rag_conversations_workspace
ON rag_conversations(workspace_id);

CREATE TABLE rag_turns (
    conversation_id TEXT NOT NULL,
    turn_id TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    retrieval_json TEXT NOT NULL,
    created_at_utc TEXT NOT NULL,
    PRIMARY KEY(conversation_id, turn_id),
    FOREIGN KEY(conversation_id)
        REFERENCES rag_conversations(conversation_id)
        ON DELETE CASCADE,
    FOREIGN KEY(workspace_id)
        REFERENCES workspaces(id)
);

CREATE INDEX idx_rag_turns_conversation
ON rag_turns(conversation_id);

CREATE TRIGGER rag_turn_workspace_insert_guard
BEFORE INSERT ON rag_turns
BEGIN
    SELECT CASE
        WHEN (
            SELECT workspace_id
            FROM rag_conversations
            WHERE conversation_id = NEW.conversation_id
        ) IS NULL
        THEN RAISE(ABORT, 'rag conversation missing')
        WHEN (
            SELECT workspace_id
            FROM rag_conversations
            WHERE conversation_id = NEW.conversation_id
        ) <> NEW.workspace_id
        THEN RAISE(ABORT, 'rag workspace mismatch')
    END;
END;

CREATE TRIGGER rag_turn_workspace_update_guard
BEFORE UPDATE ON rag_turns
BEGIN
    SELECT CASE
        WHEN (
            SELECT workspace_id
            FROM rag_conversations
            WHERE conversation_id = NEW.conversation_id
        ) <> NEW.workspace_id
        THEN RAISE(ABORT, 'rag workspace mismatch')
    END;
END;
""",
    encoding="utf-8",
    newline="\n",
)


# ============================================================
# 2. Register migration v4
# ============================================================

migrations = read(MIGRATIONS)

migrations = replace_once(
    migrations,
    "pub const LATEST_SCHEMA_VERSION: u32 = 3;",
    "pub const LATEST_SCHEMA_VERSION: u32 = 4;",
    "schema version",
)

migrations = replace_once(
    migrations,
    "const MIGRATIONS: [Migration; 3] = [",
    "const MIGRATIONS: [Migration; 4] = [",
    "migration array length",
)

migrations = replace_once(
    migrations,
    '''    Migration {
        info: MigrationInfo {
            version: 3,
            name: "audit_integrity",
        },
        sql: include_str!("../migrations/0003_audit_integrity.sql"),
    },
];''',
    '''    Migration {
        info: MigrationInfo {
            version: 3,
            name: "audit_integrity",
        },
        sql: include_str!("../migrations/0003_audit_integrity.sql"),
    },
    Migration {
        info: MigrationInfo {
            version: 4,
            name: "rag_conversations",
        },
        sql: include_str!(
            "../migrations/0004_rag_conversations.sql"
        ),
    },
];''',
    "register migration v4",
)

migrations = migrations.replace(
    '''                4,
                "invalid",''',
    '''                5,
                "invalid",''',
)

migrations = migrations.replace(
    '''WHERE version = 4",''',
    '''WHERE version = 5",''',
)

migrations = migrations.replace(
    "assert_eq!(schema_version, 3);",
    "assert_eq!(schema_version, 4);",
)

write(MIGRATIONS, migrations)


# ============================================================
# 3. Migration tests
# ============================================================

migration_tests = read(MIGRATION_TESTS)

migration_tests = migration_tests.replace(
    "assert_eq!(db.schema_version()?, 3);",
    "assert_eq!(db.schema_version()?, 4);",
)

migration_tests = migration_tests.replace(
    '''VALUES (4, 'future', 't')''',
    '''VALUES (5, 'future', 't')''',
)

migration_tests = migration_tests.replace(
    "assert_eq!(migrations, 3);",
    "assert_eq!(migrations, 4);",
)

if "rag_conversation_schema_enforces_workspace_binding" not in migration_tests:
    migration_tests += r'''

#[test]
fn rag_conversation_schema_enforces_workspace_binding(
) -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let db = KnowledgeDb::open(
        &temp.path().join("knowledge.db"),
    )?;

    db.with_connection(|connection| {
        connection.execute(
            "INSERT INTO workspaces \
             (id, name, created_at_utc, archived_at_utc) \
             VALUES ('workspace-a', 'A', 't', NULL)",
            [],
        )?;

        connection.execute(
            "INSERT INTO workspaces \
             (id, name, created_at_utc, archived_at_utc) \
             VALUES ('workspace-b', 'B', 't', NULL)",
            [],
        )?;

        connection.execute(
            "INSERT INTO rag_conversations \
             (conversation_id, workspace_id, mode, \
              selected_source_ids_json, created_at_utc, \
              updated_at_utc) \
             VALUES \
             ('conversation-1', 'workspace-a', \
              'ask_workspace', '[]', 't', 't')",
            [],
        )?;

        connection.execute(
            "INSERT INTO rag_turns \
             (conversation_id, turn_id, workspace_id, \
              retrieval_json, created_at_utc) \
             VALUES \
             ('conversation-1', 'turn-1', 'workspace-a', \
              '{}', 't')",
            [],
        )?;

        assert!(
            connection
                .execute(
                    "INSERT INTO rag_turns \
                     (conversation_id, turn_id, workspace_id, \
                      retrieval_json, created_at_utc) \
                     VALUES \
                     ('conversation-1', 'turn-2', \
                      'workspace-b', '{}', 't')",
                    [],
                )
                .is_err()
        );

        Ok(())
    })?;

    Ok(())
}
'''

write(MIGRATION_TESTS, migration_tests)


# ============================================================
# 4. KnowledgeCore persistence methods
# ============================================================

services = read(APP_SERVICES)

if "pub fn bind_rag_conversation(" not in services:
    marker = '''    pub fn resolve_citation(
        &self,
        workspace_id: WorkspaceId,
        hit: &SearchHit,
    ) -> Result<Citation, AppError> {'''

    addition = r'''    pub fn bind_rag_conversation(
        &self,
        conversation_id: &str,
        workspace_id: WorkspaceId,
        mode: &str,
        selected_source_ids: &[SourceId],
    ) -> Result<(), AppError> {
        let conversation_id = conversation_id.trim();

        if conversation_id.is_empty() {
            return Err(AppError::InvalidConfiguration);
        }

        if !matches!(
            mode,
            "ask_workspace" | "evidence_only"
        ) {
            return Err(AppError::InvalidConfiguration);
        }

        let selected_source_ids_json =
            serde_json::to_string(
                &selected_source_ids
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
            )
            .map_err(|_| AppError::InvalidConfiguration)?;

        self.database()
            .immediate_transaction(|transaction| {
                let existing_workspace: Option<String> =
                    transaction
                        .query_row(
                            "SELECT workspace_id \
                             FROM rag_conversations \
                             WHERE conversation_id = ?1",
                            rusqlite::params![
                                conversation_id
                            ],
                            |row| row.get(0),
                        )
                        .optional()?;

                if let Some(existing_workspace) =
                    existing_workspace
                {
                    if existing_workspace
                        != workspace_id.to_string()
                    {
                        return Err(
                            knowledge_db::DbError::
                                InternalAuditInvariant,
                        );
                    }

                    transaction.execute(
                        "UPDATE rag_conversations \
                         SET mode = ?1, \
                             selected_source_ids_json = ?2, \
                             updated_at_utc = ?3 \
                         WHERE conversation_id = ?4",
                        rusqlite::params![
                            mode,
                            selected_source_ids_json,
                            Utc::now(),
                            conversation_id,
                        ],
                    )?;

                    return Ok(());
                }

                transaction.execute(
                    "INSERT INTO rag_conversations \
                     (conversation_id, workspace_id, mode, \
                      selected_source_ids_json, \
                      created_at_utc, updated_at_utc) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                    rusqlite::params![
                        conversation_id,
                        workspace_id.to_string(),
                        mode,
                        selected_source_ids_json,
                        Utc::now(),
                    ],
                )?;

                Ok(())
            })
            .map_err(AppError::from)
    }

    pub fn rag_conversation_workspace(
        &self,
        conversation_id: &str,
    ) -> Result<Option<WorkspaceId>, AppError> {
        let conversation_id = conversation_id.trim();

        if conversation_id.is_empty() {
            return Ok(None);
        }

        self.database()
            .with_connection(|connection| {
                let value: Option<String> = connection
                    .query_row(
                        "SELECT workspace_id \
                         FROM rag_conversations \
                         WHERE conversation_id = ?1",
                        rusqlite::params![conversation_id],
                        |row| row.get(0),
                    )
                    .optional()?;

                value
                    .map(|value| {
                        value.parse().map_err(
                            knowledge_db::DbError::from,
                        )
                    })
                    .transpose()
            })
            .map_err(AppError::from)
    }

    pub fn save_rag_turn(
        &self,
        conversation_id: &str,
        turn_id: &str,
        workspace_id: WorkspaceId,
        retrieval: &serde_json::Value,
    ) -> Result<(), AppError> {
        let conversation_id = conversation_id.trim();
        let turn_id = turn_id.trim();

        if conversation_id.is_empty()
            || turn_id.is_empty()
        {
            return Err(AppError::InvalidConfiguration);
        }

        let retrieval_json =
            serde_json::to_string(retrieval)
                .map_err(|_| AppError::InvalidConfiguration)?;

        self.database()
            .immediate_transaction(|transaction| {
                transaction.execute(
                    "INSERT INTO rag_turns \
                     (conversation_id, turn_id, workspace_id, \
                      retrieval_json, created_at_utc) \
                     VALUES (?1, ?2, ?3, ?4, ?5) \
                     ON CONFLICT(conversation_id, turn_id) \
                     DO UPDATE SET \
                        retrieval_json = excluded.retrieval_json",
                    rusqlite::params![
                        conversation_id,
                        turn_id,
                        workspace_id.to_string(),
                        retrieval_json,
                        Utc::now(),
                    ],
                )?;

                Ok(())
            })
            .map_err(AppError::from)
    }

    pub fn rag_turns(
        &self,
        conversation_id: &str,
    ) -> Result<
        std::collections::BTreeMap<
            String,
            serde_json::Value,
        >,
        AppError,
    > {
        let conversation_id = conversation_id.trim();

        if conversation_id.is_empty() {
            return Ok(
                std::collections::BTreeMap::new(),
            );
        }

        self.database()
            .with_connection(|connection| {
                let mut statement = connection.prepare(
                    "SELECT turn_id, retrieval_json \
                     FROM rag_turns \
                     WHERE conversation_id = ?1 \
                     ORDER BY created_at_utc, turn_id",
                )?;

                let rows = statement.query_map(
                    rusqlite::params![conversation_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                        ))
                    },
                )?;

                let mut values =
                    std::collections::BTreeMap::new();

                for row in rows {
                    let (turn_id, retrieval_json) = row?;

                    let retrieval =
                        serde_json::from_str(
                            &retrieval_json,
                        )
                        .map_err(|source| {
                            rusqlite::Error::
                                FromSqlConversionFailure(
                                    1,
                                    rusqlite::types::Type::Text,
                                    Box::new(source),
                                )
                        })?;

                    values.insert(turn_id, retrieval);
                }

                Ok(values)
            })
            .map_err(AppError::from)
    }

'''

    services = replace_once(
        services,
        marker,
        addition + marker,
        "add RAG persistence services",
    )

write(APP_SERVICES, services)


# ============================================================
# 5. RAG DTOs must be serializable and cloneable
# ============================================================

rag = read(RAG)

rag = replace_once(
    rag,
    '''#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RagEvidenceDto {''',
    '''#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
)]
#[serde(rename_all = "camelCase")]
pub struct RagEvidenceDto {''',
    "RAG evidence deserialize",
)

rag = replace_once(
    rag,
    '''#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RagRetrievalDto {
    pub strategy: &'static str,''',
    '''#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
)]
#[serde(rename_all = "camelCase")]
pub struct RagRetrievalDto {
    pub strategy: String,''',
    "RAG retrieval deserialize",
)

rag = rag.replace(
    '''strategy: "fts_only",''',
    '''strategy: "fts_only".to_owned(),''',
)

if "pub fn mode_storage_name(" not in rag:
    marker = '''impl ChatKnowledgeMode {
    #[must_use]
    pub fn is_grounded(self) -> bool {
        matches!(self, Self::AskWorkspace | Self::EvidenceOnly)
    }
}'''

    replacement = '''impl ChatKnowledgeMode {
    #[must_use]
    pub fn is_grounded(self) -> bool {
        matches!(self, Self::AskWorkspace | Self::EvidenceOnly)
    }

    #[must_use]
    pub fn storage_name(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::AskWorkspace => "ask_workspace",
            Self::EvidenceOnly => "evidence_only",
        }
    }
}

pub fn mode_storage_name(
    mode: ChatKnowledgeMode,
) -> &'static str {
    mode.storage_name()
}'''

    rag = replace_once(
        rag,
        marker,
        replacement,
        "add mode storage name",
    )

write(RAG, rag)


# ============================================================
# 6. Chat stream completion information
# ============================================================

bridge = read(CHAT_BRIDGE)

if "pub struct ChatStreamCompletion" not in bridge:
    marker = '''#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatStreamStartResult {
    pub request_id: String,
}'''

    addition = marker + '''

#[derive(Debug, Clone)]
pub struct ChatStreamCompletion {
    pub conversation_id: Option<String>,
    pub turn_id: Option<String>,
    pub cancelled: bool,
}'''

    bridge = replace_once(
        bridge,
        marker,
        addition,
        "add stream completion DTO",
    )

bridge = replace_once(
    bridge,
    '''pub async fn stream_chat(
    app: AppHandle,
    request_id: String,
    request: ChatSendRequest,
    cancellation: CancellationToken,
) -> Result<(), String> {''',
    '''pub async fn stream_chat(
    app: AppHandle,
    request_id: String,
    request: ChatSendRequest,
    cancellation: CancellationToken,
) -> Result<ChatStreamCompletion, String> {''',
    "stream completion return type",
)

bridge = replace_once(
    bridge,
    '''                return Ok(());''',
    '''                return Ok(ChatStreamCompletion {
                    conversation_id,
                    turn_id,
                    cancelled: true,
                });''',
    "cancelled completion",
)

bridge = replace_once(
    bridge,
    '''    Ok(())
}

pub async fn send_chat(''',
    '''    Ok(ChatStreamCompletion {
        conversation_id,
        turn_id,
        cancelled: false,
    })
}

pub async fn send_chat(''',
    "normal stream completion",
)

write(CHAT_BRIDGE, bridge)


# ============================================================
# 7. Chat history DTO enrichment
# ============================================================

bridge = read(CHAT_BRIDGE)

bridge = replace_once(
    bridge,
    '''pub struct ChatHistoryResponseDto {
    #[serde(default)]
    pub chat: Vec<ChatHistoryMessageDto>,
    pub conversation_id: String,
    pub slug: String,
    #[serde(default)]
    pub agent: Value,
    pub is_owner: bool,
}''',
    '''pub struct ChatHistoryResponseDto {
    #[serde(default)]
    pub chat: Vec<ChatHistoryMessageDto>,
    pub conversation_id: String,
    pub slug: String,
    #[serde(default)]
    pub agent: Value,
    pub is_owner: bool,
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub rag_by_turn:
        std::collections::BTreeMap<String, Value>,
}''',
    "enrich history DTO",
)

write(CHAT_BRIDGE, bridge)


# ============================================================
# 8. Desktop command binding and persistence
# ============================================================

lib = read(LIB)

lib = replace_once(
    lib,
    '''async fn chat_history(
    conversation_id: String,
    khoj_runtime: tauri::State<'_, KhojRuntimeManager>,
) -> Result<chat_bridge::ChatHistoryDto, String> {''',
    '''async fn chat_history(
    conversation_id: String,
    state: tauri::State<'_, AppState>,
    khoj_runtime: tauri::State<'_, KhojRuntimeManager>,
) -> Result<chat_bridge::ChatHistoryDto, String> {''',
    "history state parameter",
)

lib = replace_once(
    lib,
    '''    chat_bridge::get_history(&conversation_id).await
}''',
    '''    let mut history =
        chat_bridge::get_history(&conversation_id).await?;

    history.response.workspace_id = state
        .core()
        .rag_conversation_workspace(
            &conversation_id,
        )
        .map_err(|error| {
            format!(
                "Unable to load conversation workspace: \
                 {error}"
            )
        })?
        .map(|workspace_id| workspace_id.to_string());

    history.response.rag_by_turn = state
        .core()
        .rag_turns(&conversation_id)
        .map_err(|error| {
            format!(
                "Unable to load conversation evidence: \
                 {error}"
            )
        })?;

    Ok(history)
}''',
    "enrich history response",
)

# Bind existing conversation before retrieval.
lib = replace_once(
    lib,
    '''    if request.mode.is_grounded() {
        let _ = app.emit(''',
    '''    if request.mode.is_grounded() {
        if let (
            Some(conversation_id),
            Some(workspace_id),
        ) = (
            request
                .conversation_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty()),
            request
                .workspace_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty()),
        ) {
            let workspace_id =
                parse_id(workspace_id, "workspace ID")
                    .map_err(|error| {
                        format!(
                            "Invalid conversation workspace: \
                             {error:?}"
                        )
                    })?;

            let selected_source_ids = request
                .selected_source_ids
                .iter()
                .map(|value| {
                    parse_id(value, "source ID")
                })
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| {
                    format!(
                        "Invalid conversation source scope: \
                         {error:?}"
                    )
                })?;

            state
                .core()
                .bind_rag_conversation(
                    conversation_id,
                    workspace_id,
                    rag::mode_storage_name(request.mode),
                    &selected_source_ids,
                )
                .map_err(|_| {
                    "This conversation belongs to a \
                     different workspace. Start a new \
                     conversation before changing \
                     workspace."
                        .to_owned()
                })?;
        }

        let _ = app.emit(''',
    "pre-bind existing conversation",
)

old_spawn = '''    let request = prepared.request;
    let cancellation = streams.register(request_id.clone())?;
    let stream_manager = streams.inner().clone();
    let task_request_id = request_id.clone();

    tauri::async_runtime::spawn(async move {
        let result =
            chat_bridge::stream_chat(app.clone(), task_request_id.clone(), request, cancellation)
                .await;

        if let Err(error) = result {
            let _ = app.emit(
                chat_bridge::CHAT_STREAM_EVENT,
                chat_bridge::ChatStreamEvent {
                    request_id: task_request_id.clone(),
                    kind: "error".to_owned(),
                    data: serde_json::Value::String(error),
                },
            );
        }

        stream_manager.remove(&task_request_id);
    });'''

new_spawn = '''    let request = prepared.request;
    let retrieval = prepared.retrieval;
    let request_mode = request.mode;
    let request_workspace_id =
        request.workspace_id.clone();
    let selected_source_ids =
        request.selected_source_ids.clone();

    let cancellation =
        streams.register(request_id.clone())?;
    let stream_manager = streams.inner().clone();
    let task_request_id = request_id.clone();
    let task_state = state.inner().clone();

    tauri::async_runtime::spawn(async move {
        let result = chat_bridge::stream_chat(
            app.clone(),
            task_request_id.clone(),
            request,
            cancellation,
        )
        .await;

        match result {
            Ok(completion) => {
                if !completion.cancelled {
                    if let (
                        Some(conversation_id),
                        Some(turn_id),
                        Some(workspace_id),
                        Some(retrieval),
                    ) = (
                        completion.conversation_id,
                        completion.turn_id,
                        request_workspace_id,
                        retrieval,
                    ) {
                        let persistence = (|| {
                            let workspace_id =
                                parse_id(
                                    &workspace_id,
                                    "workspace ID",
                                )
                                .map_err(|error| {
                                    format!(
                                        "Invalid persisted \
                                         workspace: {error:?}"
                                    )
                                })?;

                            let source_ids =
                                selected_source_ids
                                    .iter()
                                    .map(|value| {
                                        parse_id(
                                            value,
                                            "source ID",
                                        )
                                    })
                                    .collect::<
                                        Result<Vec<_>, _>,
                                    >()
                                    .map_err(|error| {
                                        format!(
                                            "Invalid persisted \
                                             source scope: \
                                             {error:?}"
                                        )
                                    })?;

                            task_state
                                .core()
                                .bind_rag_conversation(
                                    &conversation_id,
                                    workspace_id,
                                    rag::mode_storage_name(
                                        request_mode,
                                    ),
                                    &source_ids,
                                )
                                .map_err(|error| {
                                    format!(
                                        "Unable to bind RAG \
                                         conversation: {error}"
                                    )
                                })?;

                            let value =
                                serde_json::to_value(
                                    &retrieval,
                                )
                                .map_err(|error| {
                                    format!(
                                        "Unable to serialize \
                                         RAG metadata: {error}"
                                    )
                                })?;

                            task_state
                                .core()
                                .save_rag_turn(
                                    &conversation_id,
                                    &turn_id,
                                    workspace_id,
                                    &value,
                                )
                                .map_err(|error| {
                                    format!(
                                        "Unable to persist RAG \
                                         metadata: {error}"
                                    )
                                })?;

                            Ok::<(), String>(())
                        })();

                        if let Err(error) = persistence {
                            let _ = app.emit(
                                chat_bridge::
                                    CHAT_STREAM_EVENT,
                                chat_bridge::
                                    ChatStreamEvent {
                                    request_id:
                                        task_request_id
                                            .clone(),
                                    kind:
                                        "error".to_owned(),
                                    data:
                                        serde_json::Value::
                                            String(error),
                                },
                            );
                        }
                    }
                }
            }
            Err(error) => {
                let _ = app.emit(
                    chat_bridge::CHAT_STREAM_EVENT,
                    chat_bridge::ChatStreamEvent {
                        request_id:
                            task_request_id.clone(),
                        kind: "error".to_owned(),
                        data:
                            serde_json::Value::String(
                                error,
                            ),
                    },
                );
            }
        }

        stream_manager.remove(&task_request_id);
    });'''

lib = replace_once(
    lib,
    old_spawn,
    new_spawn,
    "persist stream completion",
)

write(LIB, lib)


# ============================================================
# 9. TypeScript history contracts
# ============================================================

contracts = read(CONTRACTS)

contracts = replace_once(
    contracts,
    '''    agent: Record<string, unknown> | null;
    is_owner: boolean;
  };''',
    '''    agent: Record<string, unknown> | null;
    is_owner: boolean;
    workspace_id: string | null;
    rag_by_turn: Record<string, RagRetrievalDto>;
  };''',
    "history persistence contracts",
)

write(CONTRACTS, contracts)


# ============================================================
# 10. Restore RAG metadata in React
# ============================================================

app = read(APP)

app = replace_once(
    app,
    '''    setConversationId(history.response.conversation_id);
    setMessages(history.response.chat);
  }''',
    '''    setConversationId(history.response.conversation_id);
    setMessages(history.response.chat);
    setRagByTurn(history.response.rag_by_turn ?? {});
  }''',
    "restore metadata during reconciliation",
)

app = replace_once(
    app,
    '''      setConversationId(history.response.conversation_id);
      setMessages(history.response.chat);
      setRagByTurn({});
      setPreviewEvidence(null);''',
    '''      setConversationId(history.response.conversation_id);
      setMessages(history.response.chat);
      setRagByTurn(
        history.response.rag_by_turn ?? {},
      );
      setPreviewEvidence(null);''',
    "restore metadata when opening session",
)

# Reject UI workspace mismatch for a bound conversation.
app = replace_once(
    app,
    '''      const history = await client.getChatHistory(id);

      conversationIdRef.current = history.response.conversation_id;''',
    '''      const history = await client.getChatHistory(id);

      if (
        history.response.workspace_id &&
        workspace &&
        history.response.workspace_id !== workspace.id
      ) {
        throw new Error(
          "This conversation belongs to a different workspace. " +
            "Select its workspace or start a new conversation.",
        );
      }

      conversationIdRef.current =
        history.response.conversation_id;''',
    "UI conversation workspace check",
)

write(APP, app)

print("RAG_1C2_PERSISTENCE_WRITTEN")
print("Created:")
print(f"  {MIGRATION}")
print("Modified:")
for path in [
    MIGRATIONS,
    MIGRATION_TESTS,
    APP_SERVICES,
    CHAT_BRIDGE,
    RAG,
    LIB,
    CONTRACTS,
    APP,
]:
    print(f"  {path}")

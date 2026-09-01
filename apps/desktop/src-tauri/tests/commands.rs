use std::{fs, io, path::PathBuf, thread};

use knowledge_app::{AppConfig, AppError};
use r2h_second_brain_desktop::{
    AppState, AuditListRequest, COMMAND_NAMES, CitationResolveRequest, CommandError,
    CreateBackupRequest, CreateWorkspaceRequest, IngestFilesRequest, IngestStatusDto,
    IntegrityModeDto, IntegrityVerifyRequest, RestoreWorkspaceBackupRequest,
    ScanOrphanBlobsRequest, SearchHitDto, SearchRequestDto, SourceGetRequest, SourceListRequest,
    SourceSpanDto, VerifyWorkspaceIntegrityRequest, WorkspaceDto, audit_list, citation_resolve,
    create_consistent_backup, integrity_verify, restore_workspace_backup, scan_orphan_blobs,
    search_execute, source_get, source_ingest_files, source_list, verify_workspace_integrity,
    workspace_create, workspace_list,
};

const APPROVED_COMMANDS: [&str; 34] = [
    "workspace_create",
    "workspace_list",
    "source_ingest_files",
    "source_list",
    "source_get",
    "search_execute",
    "citation_resolve",
    "audit_list",
    "integrity_verify",
    "create_consistent_backup",
    "verify_workspace_integrity",
    "scan_orphan_blobs",
    "restore_workspace_backup",
    "khoj_runtime_status",
    "khoj_runtime_start",
    "khoj_runtime_stop",
    "khoj_runtime_restart",
    "local_model_runtime_status",
    "local_model_runtime_start",
    "local_model_runtime_stop",
    "local_model_runtime_restart",
    "embedding_runtime_status",
    "embedding_runtime_start",
    "embedding_runtime_stop",
    "embedding_runtime_restart",
    "reranker_runtime_status",
    "reranker_runtime_start",
    "reranker_runtime_stop",
    "reranker_runtime_restart",
    "chat_send",
    "chat_sessions",
    "chat_history",
    "chat_stream_start",
    "chat_stream_cancel",
];

#[test]
fn command_registry_is_exact_and_excludes_generic_access() {
    assert_eq!(COMMAND_NAMES, APPROVED_COMMANDS);
    for dangerous in [
        "execute_sql",
        "read_file",
        "write_file",
        "list_directory",
        "run_command",
    ] {
        assert!(!COMMAND_NAMES.contains(&dangerous));
    }
}

#[test]
fn commands_reuse_one_initialized_core() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let created = fixture.create_workspace("Shared state")?;
    let listed = workspace_list(&fixture.state)?;
    assert_eq!(listed, vec![created]);
    assert_eq!(
        fixture.state.schema_version()?,
        knowledge_app::LATEST_SCHEMA_VERSION
    );
    Ok(())
}

#[test]
fn workspace_requests_validate_and_list_deterministically() -> Result<(), Box<dyn std::error::Error>>
{
    let fixture = Fixture::new()?;
    for name in ["", "   ", "bad\nname"] {
        let result = workspace_create(
            &fixture.state,
            CreateWorkspaceRequest {
                name: name.to_owned(),
            },
        );
        assert!(matches!(result, Err(CommandError::InvalidRequest(_))));
    }
    let result = workspace_create(
        &fixture.state,
        CreateWorkspaceRequest {
            name: "a".repeat(121),
        },
    );
    assert!(matches!(result, Err(CommandError::InvalidRequest(_))));

    let first = fixture.create_workspace("الأولى")?;
    let second = fixture.create_workspace("Second")?;
    assert_eq!(workspace_list(&fixture.state)?, vec![first, second]);
    Ok(())
}

#[test]
fn arabic_command_flow_survives_original_file_removal() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let workspace = fixture.create_workspace("مساحة المعرفة")?;
    let source_path = fixture.allowed.join("دليل.md");
    fs::write(
        &source_path,
        "# الحالة\n\nالمعرفة المحلية آمنة — Unicode café ١٢٣.",
    )?;

    let ingested = source_ingest_files(
        &fixture.state,
        IngestFilesRequest {
            workspace_id: workspace.id.clone(),
            paths: vec![path_string(&source_path)?],
        },
    )?;
    assert_eq!(ingested.len(), 1);
    assert_eq!(ingested[0].status, IngestStatusDto::Succeeded);
    let source_id = ingested[0]
        .source_id
        .clone()
        .ok_or_else(|| io::Error::other("successful ingest omitted source ID"))?;

    let sources = source_list(
        &fixture.state,
        SourceListRequest {
            workspace_id: workspace.id.clone(),
        },
    )?;
    assert_eq!(sources.len(), 1);
    let source = source_get(
        &fixture.state,
        SourceGetRequest {
            workspace_id: workspace.id.clone(),
            source_id,
        },
    )?;
    assert_eq!(source.display_name, "دليل.md");
    assert_eq!(source.versions.len(), 1);
    assert_eq!(source.versions[0].content_sha256.len(), 64);
    assert_eq!(
        source.versions[0].status,
        r2h_second_brain_desktop::DocumentVersionStatusDto::Ready
    );

    fs::remove_file(&source_path)?;
    let hits = fixture.search(&workspace, "المعرفة Unicode")?;
    assert_eq!(hits.len(), 1);
    let citation = citation_resolve(
        &fixture.state,
        CitationResolveRequest {
            workspace_id: workspace.id.clone(),
            hit: hits[0].clone(),
        },
    )?;
    assert!(citation.excerpt.contains("المعرفة المحلية"));

    let audit = audit_list(
        &fixture.state,
        AuditListRequest {
            workspace_id: workspace.id.clone(),
        },
    )?;
    assert!(
        audit
            .iter()
            .any(|event| event.event_type == "source.registered")
    );
    assert!(
        audit
            .iter()
            .any(|event| event.event_type == "document.version_ready")
    );
    assert!(audit.iter().all(|event| event.event_hash.len() == 64));
    let integrity = integrity_verify(
        &fixture.state,
        IntegrityVerifyRequest {
            workspace_id: workspace.id,
        },
    )?;
    assert!(integrity.is_valid);
    assert!(integrity.first_failure.is_none());
    Ok(())
}

#[test]
fn file_batch_keeps_success_and_sanitizes_outside_root_failure()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let workspace = fixture.create_workspace("Batch")?;
    let valid = fixture.allowed.join("valid.txt");
    let outside = fixture.directory.path().join("private.txt");
    fs::write(&valid, "offline batch evidence")?;
    fs::write(&outside, "must remain outside")?;

    let results = source_ingest_files(
        &fixture.state,
        IngestFilesRequest {
            workspace_id: workspace.id.clone(),
            paths: vec![
                path_string(&valid)?,
                path_string(&fixture.allowed.join("..").join("private.txt"))?,
            ],
        },
    )?;
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].status, IngestStatusDto::Succeeded);
    assert_eq!(results[1].status, IngestStatusDto::Failed);
    assert_eq!(results[1].error, Some(CommandError::AccessDenied));
    let serialized = serde_json::to_string(&results[1].error)?;
    assert!(!serialized.contains("private.txt"));
    assert!(!serialized.contains(&path_string(fixture.directory.path())?));
    assert_eq!(
        source_list(
            &fixture.state,
            SourceListRequest {
                workspace_id: workspace.id,
            },
        )?
        .len(),
        1
    );
    Ok(())
}

#[test]
fn workspace_ids_scope_sources_search_and_citations() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let first = fixture.create_workspace("First")?;
    let second = fixture.create_workspace("Second")?;
    let source_path = fixture.allowed.join("isolated.txt");
    fs::write(&source_path, "workspace isolation marker")?;
    let imported = source_ingest_files(
        &fixture.state,
        IngestFilesRequest {
            workspace_id: first.id.clone(),
            paths: vec![path_string(&source_path)?],
        },
    )?;
    let source_id = imported[0]
        .source_id
        .clone()
        .ok_or_else(|| io::Error::other("successful ingest omitted source ID"))?;
    let first_hits = fixture.search(&first, "isolation marker")?;
    assert_eq!(first_hits.len(), 1);
    assert!(fixture.search(&second, "isolation marker")?.is_empty());
    assert!(matches!(
        source_get(
            &fixture.state,
            SourceGetRequest {
                workspace_id: second.id.clone(),
                source_id,
            }
        ),
        Err(CommandError::NotFound)
    ));
    assert!(matches!(
        citation_resolve(
            &fixture.state,
            CitationResolveRequest {
                workspace_id: second.id,
                hit: first_hits[0].clone(),
            }
        ),
        Err(CommandError::NotFound | CommandError::AccessDenied)
    ));
    Ok(())
}

#[test]
fn invalid_ids_queries_limits_and_extra_fields_are_rejected()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let workspace = fixture.create_workspace("Validation")?;
    assert!(matches!(
        source_list(
            &fixture.state,
            SourceListRequest {
                workspace_id: "not-a-uuid".to_owned(),
            }
        ),
        Err(CommandError::InvalidRequest(_))
    ));
    assert!(matches!(
        source_get(
            &fixture.state,
            SourceGetRequest {
                workspace_id: workspace.id.clone(),
                source_id: "bad-source".to_owned(),
            }
        ),
        Err(CommandError::InvalidRequest(_))
    ));
    assert!(matches!(
        citation_resolve(
            &fixture.state,
            CitationResolveRequest {
                workspace_id: workspace.id.clone(),
                hit: SearchHitDto {
                    block_id: "bad-block".to_owned(),
                    document_version_id: "bad-version".to_owned(),
                    source_id: "bad-source".to_owned(),
                    score: 0.0,
                    snippet: "safe".to_owned(),
                    span: SourceSpanDto::Lines {
                        start_line: 1,
                        end_line: 1,
                    },
                },
            }
        ),
        Err(CommandError::InvalidRequest(_))
    ));
    for (query, limit) in [(" ", 10), ("valid", 0), ("valid", 101)] {
        assert!(matches!(
            search_execute(
                &fixture.state,
                SearchRequestDto {
                    workspace_id: workspace.id.clone(),
                    query: query.to_owned(),
                    source_kinds: vec![],
                    limit,
                }
            ),
            Err(CommandError::InvalidRequest(_))
        ));
    }
    assert!(
        serde_json::from_str::<CreateWorkspaceRequest>(r#"{"name":"known","extra":true}"#).is_err()
    );
    assert!(
        serde_json::from_str::<SearchRequestDto>(
            r#"{"workspaceId":"bad","query":"x","limit":5,"unknown":1}"#
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn parser_failure_error_is_stable_and_contains_no_private_path()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let workspace = fixture.create_workspace("Parser")?;
    let source = fixture.allowed.join("invalid.txt");
    fs::write(&source, [0xff, 0xfe, 0xfd])?;
    let results = source_ingest_files(
        &fixture.state,
        IngestFilesRequest {
            workspace_id: workspace.id,
            paths: vec![path_string(&source)?],
        },
    )?;
    let error = results[0]
        .error
        .as_ref()
        .ok_or_else(|| io::Error::other("parser failure omitted command error"))?;
    assert_eq!(error.code(), "storage_failure");
    let serialized = serde_json::to_string(error)?;
    assert!(!serialized.contains("invalid.txt"));
    assert!(!serialized.contains(&path_string(fixture.directory.path())?));
    Ok(())
}

#[test]
fn injection_looking_query_does_not_damage_or_leak_data() -> Result<(), Box<dyn std::error::Error>>
{
    let fixture = Fixture::new()?;
    let first = fixture.create_workspace("Injection A")?;
    let second = fixture.create_workspace("Injection B")?;
    let source_path = fixture.allowed.join("safe.txt");
    fs::write(&source_path, "healthy database marker")?;
    source_ingest_files(
        &fixture.state,
        IngestFilesRequest {
            workspace_id: first.id.clone(),
            paths: vec![path_string(&source_path)?],
        },
    )?;

    let suspicious = fixture.search(&second, "\"' OR 1=1; DROP TABLE workspaces; --")?;
    assert!(suspicious.is_empty());
    assert_eq!(fixture.search(&first, "healthy marker")?.len(), 1);
    assert_eq!(workspace_list(&fixture.state)?.len(), 2);
    Ok(())
}

#[test]
fn concurrent_read_commands_share_the_same_isolated_state() -> Result<(), Box<dyn std::error::Error>>
{
    let fixture = Fixture::new()?;
    let first = fixture.create_workspace("Concurrent A")?;
    let second = fixture.create_workspace("Concurrent B")?;
    let first_path = fixture.allowed.join("first.txt");
    let second_path = fixture.allowed.join("second.txt");
    fs::write(&first_path, "alpha concurrent")?;
    fs::write(&second_path, "beta concurrent")?;
    source_ingest_files(
        &fixture.state,
        IngestFilesRequest {
            workspace_id: first.id.clone(),
            paths: vec![path_string(&first_path)?],
        },
    )?;
    source_ingest_files(
        &fixture.state,
        IngestFilesRequest {
            workspace_id: second.id.clone(),
            paths: vec![path_string(&second_path)?],
        },
    )?;

    let first_state = fixture.state.clone();
    let first_id = first.id;
    let first_worker = thread::spawn(move || {
        search_execute(
            &first_state,
            SearchRequestDto {
                workspace_id: first_id,
                query: "alpha".to_owned(),
                source_kinds: vec![],
                limit: 10,
            },
        )
    });
    let second_state = fixture.state.clone();
    let second_id = second.id;
    let second_worker = thread::spawn(move || {
        search_execute(
            &second_state,
            SearchRequestDto {
                workspace_id: second_id,
                query: "beta".to_owned(),
                source_kinds: vec![],
                limit: 10,
            },
        )
    });
    let list_state = fixture.state.clone();
    let list_worker = thread::spawn(move || workspace_list(&list_state));

    let first_hits = first_worker
        .join()
        .map_err(|_| io::Error::other("first search worker failed"))??;
    let second_hits = second_worker
        .join()
        .map_err(|_| io::Error::other("second search worker failed"))??;
    let listed = list_worker
        .join()
        .map_err(|_| io::Error::other("workspace list worker failed"))??;
    assert_eq!(first_hits.len(), 1);
    assert_eq!(second_hits.len(), 1);
    assert_eq!(listed.len(), 2);
    Ok(())
}

#[test]
fn serialization_and_error_mapping_are_stable_and_sanitized()
-> Result<(), Box<dyn std::error::Error>> {
    let request: CreateWorkspaceRequest = serde_json::from_str(r#"{"name":"واجهة"}"#)?;
    assert_eq!(request.name, "واجهة");
    let error = CommandError::from(AppError::InvalidConfiguration);
    assert_eq!(error.code(), "storage_failure");
    let value = serde_json::to_value(error)?;
    assert_eq!(value["code"], "storage_failure");
    assert_eq!(value["details"], "local knowledge operation failed");
    let serialized = value.to_string();
    for private in ["knowledge-core.db", "SELECT ", "E:\\", "backtrace"] {
        assert!(!serialized.contains(private));
    }
    Ok(())
}

#[test]
fn maintenance_commands_are_typed_workspace_scoped_and_recoverable()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let workspace = fixture.create_workspace("Maintenance")?;
    let source = fixture.allowed.join("maintenance.txt");
    fs::write(&source, "desktop maintenance recovery marker")?;
    source_ingest_files(
        &fixture.state,
        IngestFilesRequest {
            workspace_id: workspace.id.clone(),
            paths: vec![path_string(&source)?],
        },
    )?;
    let backup = fixture.directory.path().join("desktop.r2hkb");
    let manifest = create_consistent_backup(
        &fixture.state,
        CreateBackupRequest {
            workspace_id: workspace.id.clone(),
            destination: path_string(&backup)?,
        },
    )?;
    assert_eq!(manifest.format_version, 1);
    assert_eq!(manifest.blob_count, 1);
    let integrity = verify_workspace_integrity(
        &fixture.state,
        VerifyWorkspaceIntegrityRequest {
            workspace_id: workspace.id.clone(),
            mode: IntegrityModeDto::Full,
        },
    )?;
    assert!(integrity.is_valid);
    assert_eq!(integrity.verified_blob_count, 1);
    assert!(
        scan_orphan_blobs(
            &fixture.state,
            ScanOrphanBlobsRequest {
                workspace_id: workspace.id,
            },
        )?
        .hashes
        .is_empty()
    );
    let restored_id = knowledge_domain::WorkspaceId::new().to_string();
    let restored = restore_workspace_backup(
        &fixture.state,
        RestoreWorkspaceBackupRequest {
            backup: path_string(&backup)?,
            new_workspace_id: restored_id.clone(),
        },
    )?;
    assert_eq!(restored.workspace_id, restored_id);
    assert_eq!(restored.content_block_count, 1);
    Ok(())
}

struct Fixture {
    directory: tempfile::TempDir,
    allowed: PathBuf,
    state: AppState,
}

impl Fixture {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let allowed = directory.path().join("allowed");
        fs::create_dir_all(&allowed)?;
        let state = AppState::open(
            AppConfig::new(directory.path().join("app"))?
                .with_allowed_import_roots(vec![allowed.clone()]),
        )?;
        Ok(Self {
            directory,
            allowed,
            state,
        })
    }

    fn create_workspace(&self, name: &str) -> Result<WorkspaceDto, Box<dyn std::error::Error>> {
        Ok(workspace_create(
            &self.state,
            CreateWorkspaceRequest {
                name: name.to_owned(),
            },
        )?)
    }

    fn search(
        &self,
        workspace: &WorkspaceDto,
        query: &str,
    ) -> Result<Vec<r2h_second_brain_desktop::SearchHitDto>, Box<dyn std::error::Error>> {
        Ok(search_execute(
            &self.state,
            SearchRequestDto {
                workspace_id: workspace.id.clone(),
                query: query.to_owned(),
                source_kinds: vec![],
                limit: 10,
            },
        )?)
    }
}

fn path_string(path: &std::path::Path) -> Result<String, Box<dyn std::error::Error>> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| io::Error::other("test path is not Unicode").into())
}

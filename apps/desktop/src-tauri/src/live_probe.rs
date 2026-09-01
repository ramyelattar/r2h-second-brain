//! Manual live Local AI verification probes. These tests start the real
//! ProgramData-backed runtime managers and issue real inference through the
//! production clients, broker, and RAG pipeline. They are `#[ignore]`d so
//! default release gates stay offline.
//!
//! Run sequentially (services are shared across tests):
//!
//! ```text
//! R2H_LOCAL_AI_ROOT=C:\ProgramData\R2H.AI-ELE \
//! cargo test -p r2h-second-brain-desktop --lib live_probe -- --ignored --test-threads=1 --nocapture
//! ```

use std::{
    path::PathBuf,
    sync::atomic::{AtomicU32, Ordering},
    time::{Duration, Instant},
};

use tokio_util::sync::CancellationToken;

use knowledge_app::AppConfig;

use crate::{
    ai_pack::R2hAiPackResolver,
    app_state::AppState,
    chat_bridge::ChatSendRequest,
    commands::{
        audit_list, citation_resolve, require_workspace, source_ingest_files, workspace_create,
        workspace_list,
    },
    dto::{
        AuditListRequest, CitationResolveRequest, CreateWorkspaceRequest, IngestFilesRequest,
        SearchHitDto, SourceSpanDto, WorkspaceDto,
    },
    embedding_client::{
        EMBEDDING_MODEL_ID, EMBEDDING_MODEL_REVISION, EmbeddingClient, cosine_similarity,
    },
    embedding_worker::EmbeddingWorkerManager,
    local_model_runtime::{
        LocalModelRuntimeConfig, LocalModelRuntimeManager, LocalModelRuntimeState,
    },
    prime_integration::{
        BrokerLimits, BrokerRequest, CapabilityLimits, METHOD_PROVIDER_GENERATE, PROTOCOL_VERSION,
        PrimeCapabilityBroker, R2hLocalGenerationProvider, ResponseStatus,
    },
    rag::{ChatKnowledgeMode, RagStrategy, prepare_prime_grounded_request},
    reranker_client::RerankerClient,
    retrieval_runtime::{RetrievalRuntimeConfig, RetrievalRuntimeManager, RetrievalRuntimeState},
};

static GENERATION_PID: AtomicU32 = AtomicU32::new(0);
static EMBEDDING_PID: AtomicU32 = AtomicU32::new(0);
static RERANKER_PID: AtomicU32 = AtomicU32::new(0);

type ProbeResult = Result<(), Box<dyn std::error::Error>>;

fn live_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../run-data/live-ai-e2e")
}

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn live_state() -> Result<AppState, Box<dyn std::error::Error>> {
    let root = live_root();
    std::fs::create_dir_all(root.join("imports"))?;
    let config =
        AppConfig::new(root.clone())?.with_allowed_import_roots(vec![root.join("imports")]);
    Ok(AppState::open(config)?)
}

fn generation_manager() -> LocalModelRuntimeManager {
    LocalModelRuntimeManager::new(LocalModelRuntimeConfig::with_pack(
        project_root(),
        R2hAiPackResolver::from_environment(),
    ))
}

const DOC_A: &str = "Electrical feeder voltage drop depends on current, conductor resistance, conductor length, and power factor.";
const DOC_B: &str = "Transformer loading and secondary voltage regulation should be checked against connected load.";
const DOC_C: &str = "Concrete curing requires moisture retention and temperature control.";

fn write_corpus() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let imports = live_root().join("imports");
    std::fs::create_dir_all(&imports)?;
    let documents = [
        ("doc-a-voltage-drop.txt", DOC_A),
        ("doc-b-transformer.txt", DOC_B),
        ("doc-c-concrete.txt", DOC_C),
    ];
    documents
        .iter()
        .map(|(name, body)| {
            let path = imports.join(name);
            std::fs::write(&path, body)?;
            Ok(path
                .to_str()
                .ok_or_else(|| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, "unicode corpus path")
                })?
                .to_owned())
        })
        .collect()
}

fn ensure_workspace(state: &AppState) -> Result<WorkspaceDto, Box<dyn std::error::Error>> {
    let listed = workspace_list(state)?;
    if let Some(existing) = listed.into_iter().next() {
        return Ok(existing);
    }
    Ok(workspace_create(
        state,
        CreateWorkspaceRequest {
            name: "Live AI E2E".to_owned(),
        },
    )?)
}

#[test]
#[ignore = "live Local AI probe; requires the ProgramData pack"]
fn probe_10_generation_start() -> ProbeResult {
    let manager = generation_manager();
    let started = Instant::now();
    manager.start()?;
    let status = manager.status()?;
    assert_eq!(status.state, LocalModelRuntimeState::Ready);
    if let Some(pid) = status.pid {
        GENERATION_PID.store(pid, Ordering::SeqCst);
    }
    println!(
        "GENERATION_READY pid={:?} endpoint={} model={} startup_ms={}",
        status.pid,
        status.endpoint,
        status.model_id,
        started.elapsed().as_millis()
    );
    Ok(())
}

#[tokio::test]
#[ignore = "live Local AI probe"]
async fn probe_11_generation_inference() -> ProbeResult {
    let runtime = generation_manager();
    runtime.start()?;

    let provider = R2hLocalGenerationProvider::new(runtime)
        .map_err(|error| format!("prime provider construction failed: {error:?}"))?;
    let broker = PrimeCapabilityBroker::with_provider(
        std::sync::Arc::new(provider),
        BrokerLimits {
            capability: CapabilityLimits::default(),
            downstream_timeout: Duration::from_secs(150),
        },
    );
    let grant = broker.create_session()?;
    let request = BrokerRequest {
        protocol_version: PROTOCOL_VERSION.to_owned(),
        request_id: "live-gen-1".to_owned(),
        capability: grant.capability().to_owned(),
        session_id: grant.session_id().to_owned(),
        method: METHOD_PROVIDER_GENERATE.to_owned(),
        payload: serde_json::json!({
            "requestId": "live-gen-1",
            "prompt": "Reply with exactly: LOCAL_AI_OK",
            "maxTokens": 16,
            "temperature": 0.1
        }),
    };

    let started = Instant::now();
    let frame = serde_json::to_vec(&request)?;
    let response = broker.handle_frame(&frame, CancellationToken::new()).await;
    let latency = started.elapsed().as_millis();
    println!(
        "GENERATION_INFER status={:?} latency_ms={latency}",
        response.status
    );
    assert_eq!(response.status, ResponseStatus::Success);
    let payload = response
        .payload
        .as_ref()
        .ok_or("provider response payload missing")?;
    let text = payload
        .get("text")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    println!("GENERATION_INFER text={text:?}");
    assert!(
        text.contains("LOCAL_AI_OK"),
        "generation must return the requested marker, got: {text:?}"
    );
    Ok(())
}

#[test]
#[ignore = "live Local AI probe"]
fn probe_20_embedding_start() -> ProbeResult {
    let manager = RetrievalRuntimeManager::new(RetrievalRuntimeConfig::embedding(project_root()));
    let started = Instant::now();
    manager.start()?;
    let status = manager.status()?;
    assert_eq!(status.state, RetrievalRuntimeState::Ready);
    if let Some(pid) = status.pid {
        EMBEDDING_PID.store(pid, Ordering::SeqCst);
    }
    println!(
        "EMBEDDING_READY pid={:?} endpoint={} model={} startup_ms={}",
        status.pid,
        status.endpoint,
        status.model_id,
        started.elapsed().as_millis()
    );
    Ok(())
}

#[tokio::test]
#[ignore = "live Local AI probe"]
async fn probe_21_embedding_inference() -> ProbeResult {
    let client = EmbeddingClient::local().map_err(|error| format!("client: {error}"))?;
    let texts = [
        "electrical transformer voltage",
        "transformer electrical voltage",
        "banana fruit nutrition",
    ];
    let mut vectors = Vec::new();
    for text in texts {
        let started = Instant::now();
        let vector = client
            .embed(text)
            .await
            .map_err(|error| format!("embed: {error}"))?;
        println!(
            "EMBED input={text:?} dim={} latency_ms={}",
            vector.len(),
            started.elapsed().as_millis()
        );
        assert!(!vector.is_empty(), "embedding must not be empty");
        assert!(
            vector.iter().all(|value| value.is_finite()),
            "embedding values must be finite"
        );
        vectors.push(vector);
    }
    let repeated = client
        .embed(texts[0])
        .await
        .map_err(|error| format!("repeat embed: {error}"))?;
    assert_eq!(repeated.len(), vectors[0].len(), "dimension must be stable");

    let ab =
        cosine_similarity(&vectors[0], &vectors[1]).map_err(|error| format!("cosine: {error}"))?;
    let ac =
        cosine_similarity(&vectors[0], &vectors[2]).map_err(|error| format!("cosine: {error}"))?;
    println!("EMBED cosine(A,B)={ab:.6} cosine(A,C)={ac:.6}");
    assert!(ab > ac, "semantic ordering violated: {ab} <= {ac}");
    Ok(())
}

#[test]
#[ignore = "live Local AI probe"]
fn probe_30_reranker_start() -> ProbeResult {
    // The torch model load needs ~2 GB of commit; the generation server holds
    // several GB. It has already been exercised by probes 10/11, so free its
    // commit before loading the reranker (16 GB host, 3.4 GB pagefile).
    let generation_pid = GENERATION_PID.load(Ordering::SeqCst);
    if generation_pid != 0 {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &generation_pid.to_string(), "/T", "/F"])
            .status();
        std::thread::sleep(Duration::from_secs(3));
    }

    let manager = RetrievalRuntimeManager::new(RetrievalRuntimeConfig::reranker(project_root()));
    let started = Instant::now();
    manager.start()?;
    let status = manager.status()?;
    assert_eq!(status.state, RetrievalRuntimeState::Ready);
    if let Some(pid) = status.pid {
        RERANKER_PID.store(pid, Ordering::SeqCst);
    }
    println!(
        "RERANKER_READY pid={:?} endpoint={} model={} startup_ms={}",
        status.pid,
        status.endpoint,
        status.model_id,
        started.elapsed().as_millis()
    );
    Ok(())
}

#[tokio::test]
#[ignore = "live Local AI probe"]
async fn probe_31_reranker_inference() -> ProbeResult {
    let client = RerankerClient::local().map_err(|error| format!("client: {error}"))?;
    let query = "electrical cable voltage drop";
    let candidates = [
        (
            "A",
            "Voltage drop in electrical feeders depends on current, conductor resistance, length, and power factor.",
        ),
        (
            "B",
            "Concrete curing requires maintaining adequate moisture.",
        ),
        (
            "C",
            "Electrical conductor sizing should account for allowable voltage drop and load current.",
        ),
    ];
    let mut scores = Vec::new();
    for (label, document) in candidates {
        let started = Instant::now();
        let score = client
            .score(query, document)
            .await
            .map_err(|error| format!("rerank: {error}"))?;
        println!(
            "RERANK candidate={label} score={score:.6} latency_ms={}",
            started.elapsed().as_millis()
        );
        assert!(score.is_finite() && (0.0..=1.0).contains(&score));
        scores.push((label, score));
    }
    let relevance_of = |label: &str| {
        scores
            .iter()
            .find(|(candidate, _)| *candidate == label)
            .map(|(_, score)| *score)
            .ok_or_else(|| format!("candidate {label} was not scored"))
    };
    assert!(
        relevance_of("A")? > relevance_of("B")?,
        "relevant candidate A must outrank irrelevant candidate B"
    );
    assert!(
        relevance_of("C")? > relevance_of("B")?,
        "relevant candidate C must outrank irrelevant candidate B"
    );
    Ok(())
}

#[test]
#[ignore = "live Local AI probe"]
fn probe_40_rag_corpus_ingest_and_embed() -> ProbeResult {
    let state = live_state()?;
    let workspace = ensure_workspace(&state)?;
    let ingested = source_ingest_files(
        &state,
        IngestFilesRequest {
            workspace_id: workspace.id.clone(),
            paths: write_corpus()?,
        },
    )?;
    assert_eq!(ingested.len(), 3);

    let worker = EmbeddingWorkerManager::default();
    worker
        .start(state.clone())
        .map_err(|error| format!("embedding worker: {error}"))?;
    let deadline = Instant::now() + Duration::from_secs(300);
    loop {
        let stored = state.core().current_embeddings(
            require_workspace(&state, &workspace.id, true)?,
            EMBEDDING_MODEL_ID,
            EMBEDDING_MODEL_REVISION,
        )?;
        println!("EMBED_WORKER stored={}", stored.len());
        if stored.len() >= 3 {
            worker
                .stop()
                .map_err(|error| format!("embedding worker stop: {error}"))?;
            break;
        }
        assert!(
            Instant::now() < deadline,
            "embedding worker did not embed the corpus in time"
        );
        std::thread::sleep(Duration::from_secs(2));
    }

    let audit = audit_list(
        &state,
        AuditListRequest {
            workspace_id: workspace.id.clone(),
        },
    )?;
    assert!(
        audit
            .iter()
            .any(|event| event.event_type == "source.registered"),
        "audit must record source registration"
    );
    assert!(
        audit
            .iter()
            .any(|event| event.event_type == "document.version_ready"),
        "audit must record version ready"
    );
    Ok(())
}

#[tokio::test]
#[ignore = "live Local AI probe"]
async fn probe_41_vector_only_rag() -> ProbeResult {
    let state = live_state()?;
    let workspace = ensure_workspace(&state)?;
    let request = ChatSendRequest {
        query: "كيف يؤثر طول الموصل والتيار على فقد الجهد في الكابلات؟".to_owned(),
        conversation_id: None,
        create_new: false,
        mode: ChatKnowledgeMode::EvidenceOnly,
        workspace_id: Some(workspace.id.clone()),
        selected_source_ids: Vec::new(),
    };
    let started = Instant::now();
    let prepared = prepare_prime_grounded_request(&state, request)
        .await
        .map_err(|error| format!("prime grounded retrieval: {error}"))?;
    let retrieval = prepared
        .retrieval
        .as_ref()
        .ok_or("retrieval dto missing")?;
    println!(
        "VECTOR_RAG strategy={:?} fts_used={} vector_used={} reranker_used={} candidates={} selected={} latency_ms={}",
        retrieval.strategy,
        retrieval.fts_used,
        retrieval.vector_used,
        retrieval.reranker_used,
        retrieval.candidate_count,
        retrieval.selected_count,
        started.elapsed().as_millis()
    );
    assert_eq!(retrieval.strategy, RagStrategy::VectorOnly);
    assert!(retrieval.vector_used);
    assert!(!retrieval.fts_used);
    assert!(!retrieval.evidence.is_empty());
    let top = &retrieval.evidence[0];
    assert!(
        top.excerpt.contains("voltage drop"),
        "vector-only top evidence must be the voltage drop document, got: {}",
        top.excerpt
    );
    let citation = citation_resolve(
        &state,
        CitationResolveRequest {
            workspace_id: workspace.id.clone(),
            hit: SearchHitDto {
                block_id: top.block_id.clone(),
                document_version_id: top.document_version_id.clone(),
                source_id: top.source_id.clone(),
                score: top.score,
                snippet: top.excerpt.clone(),
                span: SourceSpanDto::Lines {
                    start_line: 1,
                    end_line: 1,
                },
            },
        },
    )?;
    println!(
        "VECTOR_RAG citation locator={} sha256_len={}",
        citation.canonical_locator,
        citation.content_sha256.len()
    );
    assert!(!citation.excerpt.is_empty());
    assert_eq!(citation.content_sha256.len(), 64);
    Ok(())
}

#[tokio::test]
#[ignore = "live Local AI probe"]
async fn probe_42_hybrid_rag() -> ProbeResult {
    let state = live_state()?;
    let workspace = ensure_workspace(&state)?;
    let request = ChatSendRequest {
        query: "voltage drop conductor length".to_owned(),
        conversation_id: None,
        create_new: false,
        mode: ChatKnowledgeMode::EvidenceOnly,
        workspace_id: Some(workspace.id.clone()),
        selected_source_ids: Vec::new(),
    };
    let started = Instant::now();
    let prepared = prepare_prime_grounded_request(&state, request)
        .await
        .map_err(|error| format!("prime grounded retrieval: {error}"))?;
    let retrieval = prepared
        .retrieval
        .as_ref()
        .ok_or("retrieval dto missing")?;
    println!(
        "HYBRID_RAG strategy={:?} fts_used={} vector_used={} reranker_used={} candidates={} selected={} latency_ms={}",
        retrieval.strategy,
        retrieval.fts_used,
        retrieval.vector_used,
        retrieval.reranker_used,
        retrieval.candidate_count,
        retrieval.selected_count,
        started.elapsed().as_millis()
    );
    assert_eq!(retrieval.strategy, RagStrategy::Hybrid);
    assert!(retrieval.fts_used && retrieval.vector_used && retrieval.reranker_used);
    assert!(!retrieval.evidence.is_empty());
    assert!(
        retrieval
            .evidence
            .iter()
            .any(|evidence| evidence.excerpt.contains("voltage drop")),
        "hybrid evidence must include the voltage drop document"
    );
    Ok(())
}

#[tokio::test]
#[ignore = "live Local AI probe"]
async fn probe_50_restart_persistence() -> ProbeResult {
    for pid in [
        GENERATION_PID.load(Ordering::SeqCst),
        EMBEDDING_PID.load(Ordering::SeqCst),
        RERANKER_PID.load(Ordering::SeqCst),
    ] {
        if pid != 0 {
            let _ = std::process::Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .status();
        }
    }
    std::thread::sleep(Duration::from_secs(3));

    let state = live_state()?;
    let workspace = ensure_workspace(&state)?;
    let stored_before = state.core().current_embeddings(
        require_workspace(&state, &workspace.id, true)?,
        EMBEDDING_MODEL_ID,
        EMBEDDING_MODEL_REVISION,
    )?;
    assert!(
        !stored_before.is_empty(),
        "persisted embeddings must survive a service restart"
    );
    println!(
        "RESTART stored_embeddings_before_restart={}",
        stored_before.len()
    );

    // Restart the retrieval services first: the hybrid search below needs
    // them, and loading the generation model first would re-exhaust commit
    // on this host (16 GB RAM, 3.4 GB pagefile).
    let embedding = RetrievalRuntimeManager::new(RetrievalRuntimeConfig::embedding(project_root()));
    embedding.start()?;
    let reranker = RetrievalRuntimeManager::new(RetrievalRuntimeConfig::reranker(project_root()));
    reranker.start()?;

    let request = ChatSendRequest {
        query: "voltage drop conductor length".to_owned(),
        conversation_id: None,
        create_new: false,
        mode: ChatKnowledgeMode::EvidenceOnly,
        workspace_id: Some(workspace.id.clone()),
        selected_source_ids: Vec::new(),
    };
    let prepared = prepare_prime_grounded_request(&state, request)
        .await
        .map_err(|error| format!("prime grounded retrieval after restart: {error}"))?;
    let retrieval = prepared
        .retrieval
        .as_ref()
        .ok_or("retrieval dto missing")?;
    println!(
        "RESTART_HYBRID strategy={:?} vector_used={} reranker_used={} selected={}",
        retrieval.strategy,
        retrieval.vector_used,
        retrieval.reranker_used,
        retrieval.selected_count
    );
    assert_eq!(retrieval.strategy, RagStrategy::Hybrid);
    assert!(retrieval.vector_used && retrieval.reranker_used);
    assert!(!retrieval.evidence.is_empty());

    let generation = generation_manager();
    generation.start()?;
    let generation_status = generation.status()?;
    assert_eq!(generation_status.state, LocalModelRuntimeState::Ready);
    println!(
        "RESTART_GENERATION pid={:?} endpoint={}",
        generation_status.pid, generation_status.endpoint
    );
    Ok(())
}

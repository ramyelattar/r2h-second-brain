use std::{
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    net::{SocketAddr, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex, MutexGuard},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::Serialize;

use crate::ai_pack::{AiPackRole, R2hAiPackResolver};

const EMBEDDING_ENDPOINT: &str = "http://127.0.0.1:42112";
const RERANKER_ENDPOINT: &str = "http://127.0.0.1:42113";

pub(crate) const EMBEDDING_MODEL_ID: &str = "qwen3-embedding-0.6b-gguf";
pub(crate) const RERANKER_MODEL_ID: &str = "qwen3-reranker-0.6b";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalRuntimeRole {
    Embedding,
    Reranker,
}

impl RetrievalRuntimeRole {
    fn runtime_name(self) -> &'static str {
        match self {
            Self::Embedding => "embedding",
            Self::Reranker => "reranker",
        }
    }

    fn display_name(self) -> &'static str {
        match self {
            Self::Embedding => "Qwen3 Embedding",
            Self::Reranker => "Qwen3 Reranker",
        }
    }

    fn model_id(self) -> &'static str {
        match self {
            Self::Embedding => EMBEDDING_MODEL_ID,
            Self::Reranker => RERANKER_MODEL_ID,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub enum RetrievalRuntimeState {
    Stopped,
    Starting,
    Ready,
    Failed,
    Stopping,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RetrievalRuntimeStatus {
    pub role: RetrievalRuntimeRole,
    pub state: RetrievalRuntimeState,
    pub pid: Option<u32>,
    pub endpoint: String,
    pub model_id: String,
    pub model_path: String,
    pub started_at: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct RetrievalRuntimeConfig {
    pub role: RetrievalRuntimeRole,
    pub project_root: PathBuf,
    pub(crate) pack: R2hAiPackResolver,
    pub endpoint: String,
    pub startup_timeout: Duration,
    pub readiness_interval: Duration,
    pub shutdown_timeout: Duration,
    pub context_size: u32,
    pub batch_size: u32,
    pub ubatch_size: u32,
}

impl RetrievalRuntimeConfig {
    pub fn embedding(project_root: PathBuf) -> Self {
        Self::embedding_with_pack(project_root, R2hAiPackResolver::from_environment())
    }

    pub(crate) fn embedding_with_pack(project_root: PathBuf, pack: R2hAiPackResolver) -> Self {
        Self {
            role: RetrievalRuntimeRole::Embedding,
            project_root,
            pack,
            endpoint: EMBEDDING_ENDPOINT.to_owned(),
            startup_timeout: Duration::from_secs(180),
            readiness_interval: Duration::from_millis(500),
            shutdown_timeout: Duration::from_secs(10),
            context_size: 8192,
            batch_size: 2048,
            ubatch_size: 512,
        }
    }

    pub fn reranker(project_root: PathBuf) -> Self {
        Self::reranker_with_pack(project_root, R2hAiPackResolver::from_environment())
    }

    pub(crate) fn reranker_with_pack(project_root: PathBuf, pack: R2hAiPackResolver) -> Self {
        Self {
            role: RetrievalRuntimeRole::Reranker,
            project_root,
            pack,
            endpoint: RERANKER_ENDPOINT.to_owned(),
            startup_timeout: Duration::from_secs(180),
            readiness_interval: Duration::from_millis(500),
            shutdown_timeout: Duration::from_secs(10),
            context_size: 8192,
            batch_size: 2048,
            ubatch_size: 512,
        }
    }

    fn runtime_root(&self) -> PathBuf {
        self.project_root
            .join("run-data")
            .join("local-models")
            .join(self.role.runtime_name())
    }

    fn model_id(&self) -> &'static str {
        self.role.model_id()
    }

    fn port(&self) -> io::Result<u16> {
        let (address, _) = endpoint_address(&self.endpoint)?;
        Ok(address.port())
    }
}

#[derive(Clone)]
pub struct RetrievalRuntimeManager {
    config: RetrievalRuntimeConfig,
    inner: Arc<Mutex<RuntimeInner>>,
}

struct RuntimeInner {
    status: RetrievalRuntimeStatus,
    process: Option<Child>,
    lock_path: Option<PathBuf>,
    stop_requested: bool,
}

impl RetrievalRuntimeManager {
    pub fn new(config: RetrievalRuntimeConfig) -> Self {
        let status = RetrievalRuntimeStatus {
            role: config.role,
            state: RetrievalRuntimeState::Stopped,
            pid: None,
            endpoint: config.endpoint.clone(),
            model_id: config.model_id().to_owned(),
            model_path: config.pack.manifest_path().display().to_string(),
            started_at: None,
            last_error: None,
        };

        Self {
            config,
            inner: Arc::new(Mutex::new(RuntimeInner {
                status,
                process: None,
                lock_path: None,
                stop_requested: false,
            })),
        }
    }

    pub fn status(&self) -> io::Result<RetrievalRuntimeStatus> {
        Ok(self.lock_inner()?.status.clone())
    }

    pub fn start(&self) -> io::Result<()> {
        {
            let mut inner = self.lock_inner()?;

            if matches!(
                inner.status.state,
                RetrievalRuntimeState::Starting | RetrievalRuntimeState::Ready
            ) {
                return Ok(());
            }

            inner.status.state = RetrievalRuntimeState::Starting;
            inner.status.last_error = None;
            inner.stop_requested = false;
        }

        let result = self.start_owned_runtime();

        if let Err(error) = result {
            let message = sanitize_error(self.config.role, &error);
            let mut inner = self.lock_inner()?;

            if let Some(process) = inner.process.as_mut() {
                force_kill_tree(process);
            }

            inner.process = None;

            if let Some(lock_path) = inner.lock_path.take() {
                let _ = fs::remove_file(lock_path);
            }

            inner.status.pid = None;
            inner.status.started_at = None;

            if inner.stop_requested || error.kind() == io::ErrorKind::Interrupted {
                inner.status.state = RetrievalRuntimeState::Stopped;
                inner.status.last_error = None;
                return Ok(());
            }

            inner.status.state = RetrievalRuntimeState::Failed;
            inner.status.last_error = Some(message);

            return Err(error);
        }

        Ok(())
    }

    fn start_owned_runtime(&self) -> io::Result<()> {
        let capability = self
            .config
            .pack
            .resolve(match self.config.role {
                RetrievalRuntimeRole::Embedding => AiPackRole::Embedding,
                RetrievalRuntimeRole::Reranker => AiPackRole::Reranker,
            })
            .map_err(ai_pack_io_error)?;

        {
            let mut inner = self.lock_inner()?;
            inner.status.model_id = capability.model_id.clone();
            inner.status.model_path = capability.model_path.display().to_string();
        }

        let runtime_root = self.config.runtime_root();
        let logs = runtime_root.join("logs");

        fs::create_dir_all(&logs)?;

        if health_ready(&self.config.endpoint)? {
            let mut inner = self.lock_inner()?;

            inner.status.state = RetrievalRuntimeState::Ready;
            inner.status.pid = None;
            inner.status.started_at = Some(timestamp_now()?);
            inner.status.last_error = None;

            append_runtime_log(
                &logs.join("runtime.log"),
                &format!(
                    "Existing {} runtime detected",
                    self.config.role.display_name()
                ),
            )?;

            return Ok(());
        }

        let lock_path = runtime_root.join("runtime.lock");
        acquire_runtime_lock(&lock_path)?;

        {
            let mut inner = self.lock_inner()?;
            inner.lock_path = Some(lock_path);
        }

        append_runtime_log(
            &logs.join("runtime.log"),
            &format!("Starting {} runtime", self.config.role.display_name()),
        )?;

        let stdout = OpenOptions::new()
            .create(true)
            .append(true)
            .open(logs.join("stdout.log"))?;

        let stderr = OpenOptions::new()
            .create(true)
            .append(true)
            .open(logs.join("stderr.log"))?;

        let threads = thread::available_parallelism()
            .map(|count| count.get().saturating_sub(2).max(2))
            .unwrap_or(4);

        let port = self.config.port()?.to_string();
        let mut command;
        let working_directory;

        if self.config.role == RetrievalRuntimeRole::Reranker {
            let python = capability.runtime_path;
            if !python.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!(
                        "PYTHON_RUNTIME_MISSING: ProgramData Python runtime is missing: {}",
                        python.display()
                    ),
                ));
            }

            let worker = capability.worker_path.as_ref().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "RERANKER_WORKER_MISSING: ProgramData reranker worker entrypoint is not declared",
                )
            })?;
            if !worker.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!(
                        "RERANKER_WORKER_MISSING: ProgramData reranker worker entrypoint is missing: {}",
                        worker.display()
                    ),
                ));
            }

            let python_text = python
                .to_str()
                .ok_or_else(|| io::Error::other("Python runtime path is invalid"))?;
            let worker_text = worker
                .to_str()
                .ok_or_else(|| io::Error::other("reranker worker path is invalid"))?;
            let worker_directory = worker
                .parent()
                .ok_or_else(|| io::Error::other("reranker worker directory is invalid"))?;
            let local_ai_root = capability.pack_root.join("local-ai");
            let local_ai_root_text = local_ai_root
                .to_str()
                .ok_or_else(|| io::Error::other("AI pack root path is invalid"))?;
            let model_text = capability
                .model_path
                .to_str()
                .ok_or_else(|| io::Error::other("reranker model path is invalid"))?;

            command = Command::new(python_text);
            command
                .args([
                    worker_text,
                    "--http",
                    "--host",
                    "127.0.0.1",
                    "--port",
                    &port,
                ])
                .env("R2H_LOCAL_AI_ROOT", local_ai_root_text)
                .env("R2H_PROGRAMDATA_LOCAL_AI_PACK_ROOT", local_ai_root_text)
                .env("RERANKER_MODEL_NAME", capability.model_id.as_str())
                .env("RERANKER_MODEL_PATH", model_text)
                .env("TRANSFORMERS_OFFLINE", "1")
                .env("HF_HUB_OFFLINE", "1")
                .env("PYTHONDONTWRITEBYTECODE", "1")
                .env("PYTHONNOUSERSITE", "1");
            working_directory = worker_directory.to_path_buf();
        } else {
            let server = capability.runtime_path;
            let model = capability.model_path;

            if !server.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!(
                        "LLAMA_RUNTIME_MISSING: ProgramData HTTP runtime is missing: {}",
                        server.display()
                    ),
                ));
            }

            if !model.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "RETRIEVAL_MODEL_MISSING: ProgramData retrieval model file is missing",
                ));
            }

            let context_size = self.config.context_size.to_string();
            let batch_size = self.config.batch_size.to_string();
            let ubatch_size = self.config.ubatch_size.to_string();
            let threads = threads.to_string();
            let server_text = server
                .to_str()
                .ok_or_else(|| io::Error::other("llama server path is invalid"))?;
            let model_text = model
                .to_str()
                .ok_or_else(|| io::Error::other("retrieval model path is invalid"))?;
            let mut arguments = vec![
                "--model",
                model_text,
                "--alias",
                capability.model_id.as_str(),
                "--host",
                "127.0.0.1",
                "--port",
                &port,
                "--ctx-size",
                &context_size,
                "--batch-size",
                &batch_size,
                "--ubatch-size",
                &ubatch_size,
                "--threads",
                &threads,
                "--threads-batch",
                &threads,
                "--parallel",
                "1",
                "--gpu-layers",
                "0",
                "--offline",
                "--no-ui",
            ];

            if self.config.role == RetrievalRuntimeRole::Embedding {
                arguments.extend(["--embedding", "--pooling", "last", "--embd-normalize", "2"]);
            }

            command = Command::new(server_text);
            command.args(arguments);
            working_directory = server
                .parent()
                .ok_or_else(|| io::Error::other("llama runtime directory is invalid"))?
                .to_path_buf();
        }

        command
            .current_dir(working_directory)
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr));

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }

        let child = command.spawn()?;
        let pid = child.id();

        {
            let mut inner = self.lock_inner()?;
            inner.status.pid = Some(pid);
            inner.process = Some(child);
        }

        let deadline = Instant::now() + self.config.startup_timeout;

        loop {
            {
                let mut inner = self.lock_inner()?;

                if inner.stop_requested {
                    return Err(io::Error::new(
                        io::ErrorKind::Interrupted,
                        "retrieval runtime startup was cancelled",
                    ));
                }

                if let Some(process) = inner.process.as_mut()
                    && process.try_wait()?.is_some()
                {
                    return Err(io::Error::other(
                        "llama-server exited before retrieval runtime readiness",
                    ));
                }
            }

            if health_ready(&self.config.endpoint)? {
                let mut inner = self.lock_inner()?;

                if inner.stop_requested {
                    return Err(io::Error::new(
                        io::ErrorKind::Interrupted,
                        "retrieval runtime startup was cancelled",
                    ));
                }

                inner.status.state = RetrievalRuntimeState::Ready;
                inner.status.started_at = Some(timestamp_now()?);
                inner.status.last_error = None;

                append_runtime_log(
                    &logs.join("runtime.log"),
                    &format!("{} runtime ready", self.config.role.display_name()),
                )?;

                return Ok(());
            }

            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "retrieval runtime readiness timed out",
                ));
            }

            thread::sleep(self.config.readiness_interval);
        }
    }

    pub fn stop(&self) -> io::Result<()> {
        {
            let mut inner = self.lock_inner()?;

            if inner.status.state == RetrievalRuntimeState::Stopped {
                return Ok(());
            }

            inner.status.state = RetrievalRuntimeState::Stopping;
            inner.stop_requested = true;

            if let Some(process) = inner.process.as_mut() {
                request_stop_tree(process);
            } else {
                reset_stopped_state(&mut inner);
                return Ok(());
            }
        }

        let deadline = Instant::now() + self.config.shutdown_timeout;

        loop {
            let exited = {
                let mut inner = self.lock_inner()?;

                match inner.process.as_mut() {
                    Some(process) => process.try_wait()?.is_some(),
                    None => true,
                }
            };

            if exited {
                break;
            }

            if Instant::now() >= deadline {
                let mut inner = self.lock_inner()?;

                if let Some(process) = inner.process.as_mut() {
                    force_kill_tree(process);
                }

                break;
            }

            thread::sleep(Duration::from_millis(100));
        }

        append_runtime_log(
            &self.config.runtime_root().join("logs").join("runtime.log"),
            &format!("{} runtime stopped", self.config.role.display_name()),
        )?;

        let mut inner = self.lock_inner()?;

        inner.process = None;

        if let Some(lock_path) = inner.lock_path.take() {
            let _ = fs::remove_file(lock_path);
        }

        reset_stopped_state(&mut inner);

        Ok(())
    }

    fn lock_inner(&self) -> io::Result<MutexGuard<'_, RuntimeInner>> {
        self.inner
            .lock()
            .map_err(|_| io::Error::other("retrieval runtime mutex poisoned"))
    }
}

fn reset_stopped_state(inner: &mut RuntimeInner) {
    inner.status.state = RetrievalRuntimeState::Stopped;
    inner.status.pid = None;
    inner.status.started_at = None;
    inner.status.last_error = None;
    inner.stop_requested = false;
}

fn health_ready(endpoint: &str) -> io::Result<bool> {
    let (address, host) = endpoint_address(endpoint)?;

    let mut stream = match TcpStream::connect_timeout(&address, Duration::from_millis(400)) {
        Ok(stream) => stream,
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::ConnectionRefused | io::ErrorKind::TimedOut
            ) =>
        {
            return Ok(false);
        }
        Err(error) => return Err(error),
    };

    stream.set_read_timeout(Some(Duration::from_millis(700)))?;
    stream.set_write_timeout(Some(Duration::from_millis(700)))?;

    write!(
        stream,
        "GET /health HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
    )?;

    let mut response = Vec::with_capacity(4096);
    let mut buffer = [0_u8; 1024];

    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                response.extend_from_slice(&buffer[..count]);

                if response.windows(4).any(|window| window == b"\r\n\r\n") || response.len() >= 4096
                {
                    break;
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                ) =>
            {
                break;
            }
            Err(error) => return Err(error),
        }
    }

    let text = String::from_utf8_lossy(&response);

    Ok(text.starts_with("HTTP/1.1 200") || text.starts_with("HTTP/1.0 200"))
}

fn endpoint_address(endpoint: &str) -> io::Result<(SocketAddr, String)> {
    let authority = endpoint
        .strip_prefix("http://")
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "retrieval endpoint must use http://",
            )
        })?
        .split('/')
        .next()
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "retrieval endpoint is invalid")
        })?;

    let address: SocketAddr = authority.parse().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "retrieval endpoint must be a numeric loopback address",
        )
    })?;

    if !address.ip().is_loopback() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "retrieval endpoint must be loopback-only",
        ));
    }

    Ok((address, authority.to_owned()))
}

fn acquire_runtime_lock(lock_path: &Path) -> io::Result<()> {
    if let Some(parent) = lock_path.parent() {
        fs::create_dir_all(parent)?;
    }

    match OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(lock_path)
    {
        Ok(mut file) => {
            writeln!(file, "{}", std::process::id())?;
            file.flush()
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            fs::remove_file(lock_path)?;

            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(lock_path)?;

            writeln!(file, "{}", std::process::id())?;
            file.flush()
        }
        Err(error) => Err(error),
    }
}

fn request_stop_tree(process: &mut Child) {
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/PID", &process.id().to_string(), "/T"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    #[cfg(not(windows))]
    {
        let _ = process.kill();
    }
}

fn force_kill_tree(process: &mut Child) {
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/PID", &process.id().to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    #[cfg(not(windows))]
    {
        let _ = process.kill();
    }
}

fn append_runtime_log(path: &Path, message: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut file = OpenOptions::new().create(true).append(true).open(path)?;

    writeln!(file, "{} {message}", timestamp_now()?)
}

fn timestamp_now() -> io::Result<String> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| io::Error::other("system clock is before UNIX epoch"))?
        .as_secs();

    Ok(seconds.to_string())
}

fn sanitize_error(role: RetrievalRuntimeRole, error: &io::Error) -> String {
    let message = error.to_string();
    if message.starts_with("AI_PACK_")
        || message.starts_with("MODEL_")
        || message.starts_with("EMBEDDING_MODEL_")
        || message.starts_with("RERANKER_MODEL_")
        || message.starts_with("RERANKER_WORKER_")
        || message.starts_with("LLAMA_RUNTIME_")
        || message.starts_with("PYTHON_RUNTIME_")
    {
        return message;
    }

    let component = role.display_name();

    match error.kind() {
        io::ErrorKind::NotFound => format!("{component} runtime component is missing"),
        io::ErrorKind::TimedOut => format!("{component} runtime startup timed out"),
        io::ErrorKind::AlreadyExists => format!("{component} runtime is already managed"),
        _ => format!("{component} runtime failed"),
    }
}

fn ai_pack_io_error(error: crate::ai_pack::AiPackError) -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, error.to_string())
}

#[cfg(test)]
mod tests {
    use std::env;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn embedding_configuration_is_loopback_and_uses_expected_asset() -> io::Result<()> {
        let root = PathBuf::from("project-root");
        let config = RetrievalRuntimeConfig::embedding(root);

        assert_eq!(config.role, RetrievalRuntimeRole::Embedding);
        assert_eq!(config.endpoint, "http://127.0.0.1:42112");
        assert_eq!(config.port()?, 42112);
        assert_eq!(config.model_id(), "qwen3-embedding-0.6b-gguf");
        assert_eq!(config.pack.root(), R2hAiPackResolver::default_root());

        Ok(())
    }

    #[test]
    fn reranker_configuration_is_loopback_and_uses_expected_asset() -> io::Result<()> {
        let root = PathBuf::from("project-root");
        let config = RetrievalRuntimeConfig::reranker(root);

        assert_eq!(config.role, RetrievalRuntimeRole::Reranker);
        assert_eq!(config.endpoint, "http://127.0.0.1:42113");
        assert_eq!(config.port()?, 42113);
        assert_eq!(config.model_id(), "qwen3-reranker-0.6b");
        assert_eq!(config.pack.root(), R2hAiPackResolver::default_root());

        Ok(())
    }

    #[test]
    fn endpoint_parser_rejects_non_loopback_addresses() -> io::Result<()> {
        let result = endpoint_address("http://192.168.1.10:42112");

        match result {
            Ok(_) => Err(io::Error::other(
                "non-loopback endpoint unexpectedly succeeded",
            )),
            Err(error) => {
                assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
                Ok(())
            }
        }
    }

    #[test]
    fn new_manager_starts_in_stopped_state() -> io::Result<()> {
        let config = RetrievalRuntimeConfig::embedding(PathBuf::from("project-root"));
        let manager = RetrievalRuntimeManager::new(config);

        let status = manager.status()?;

        assert_eq!(status.role, RetrievalRuntimeRole::Embedding);
        assert_eq!(status.state, RetrievalRuntimeState::Stopped);
        assert_eq!(status.pid, None);
        assert_eq!(status.model_id, EMBEDDING_MODEL_ID);

        Ok(())
    }

    #[test]
    fn live_programdata_python_reranker_smoke_when_enabled()
    -> Result<(), Box<dyn std::error::Error>> {
        if env::var_os("R2H_LIVE_RERANKER_SMOKE").is_none() {
            return Ok(());
        }

        let project = tempdir()?;
        let pack = R2hAiPackResolver::from_environment();
        let manager = RetrievalRuntimeManager::new(RetrievalRuntimeConfig::reranker_with_pack(
            project.path().to_path_buf(),
            pack,
        ));

        manager.start()?;
        let status = manager.status()?;
        assert_eq!(status.state, RetrievalRuntimeState::Ready);

        let client = crate::reranker_client::RerankerClient::local()?;
        let score_result = tokio::runtime::Runtime::new()?.block_on(client.score(
            "fire alarm battery standby and alarm duration",
            "Fire alarm battery sizing uses quiescent current, alarm current, and required duration.",
        ));

        manager.stop()?;

        let score = score_result?;

        assert!(score.is_finite());
        assert!((0.0..=1.0).contains(&score));
        assert!(
            score > 0.5,
            "representative relevant document should score as relevant: {score}"
        );
        Ok(())
    }
}

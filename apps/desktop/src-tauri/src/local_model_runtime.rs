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

const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:42111";

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub enum LocalModelRuntimeState {
    Stopped,
    Starting,
    Ready,
    Failed,
    Stopping,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LocalModelRuntimeStatus {
    pub state: LocalModelRuntimeState,
    pub pid: Option<u32>,
    pub endpoint: String,
    pub model_id: String,
    pub model_path: String,
    pub started_at: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct LocalModelRuntimeConfig {
    pub project_root: PathBuf,
    pub(crate) pack: R2hAiPackResolver,
    pub endpoint: String,
    pub startup_timeout: Duration,
    pub readiness_interval: Duration,
    pub shutdown_timeout: Duration,
    pub context_size: u32,
}

impl LocalModelRuntimeConfig {
    pub fn development(project_root: PathBuf) -> Self {
        Self::with_pack(project_root, R2hAiPackResolver::from_environment())
    }

    pub(crate) fn with_pack(project_root: PathBuf, pack: R2hAiPackResolver) -> Self {
        Self {
            project_root,
            pack,
            endpoint: DEFAULT_ENDPOINT.to_owned(),
            startup_timeout: Duration::from_secs(180),
            readiness_interval: Duration::from_millis(500),
            shutdown_timeout: Duration::from_secs(10),
            context_size: 4096,
        }
    }

    fn runtime_root(&self) -> PathBuf {
        self.project_root.join("run-data").join("local-models")
    }
}

#[derive(Clone)]
pub struct LocalModelRuntimeManager {
    config: LocalModelRuntimeConfig,
    inner: Arc<Mutex<RuntimeInner>>,
}

struct RuntimeInner {
    status: LocalModelRuntimeStatus,
    process: Option<Child>,
    lock_path: Option<PathBuf>,
    stop_requested: bool,
}

impl LocalModelRuntimeManager {
    pub fn new(config: LocalModelRuntimeConfig) -> Self {
        let endpoint = config.endpoint.clone();
        let model_path = config.pack.manifest_path().display().to_string();

        Self {
            config,
            inner: Arc::new(Mutex::new(RuntimeInner {
                status: LocalModelRuntimeStatus {
                    state: LocalModelRuntimeState::Stopped,
                    pid: None,
                    endpoint,
                    model_id: "unresolved".to_owned(),
                    model_path,
                    started_at: None,
                    last_error: None,
                },
                process: None,
                lock_path: None,
                stop_requested: false,
            })),
        }
    }

    pub fn status(&self) -> io::Result<LocalModelRuntimeStatus> {
        Ok(self.lock_inner()?.status.clone())
    }

    pub fn start(&self) -> io::Result<()> {
        {
            let mut inner = self.lock_inner()?;

            if matches!(
                inner.status.state,
                LocalModelRuntimeState::Starting | LocalModelRuntimeState::Ready
            ) {
                return Ok(());
            }

            inner.status.state = LocalModelRuntimeState::Starting;
            inner.status.last_error = None;
            inner.stop_requested = false;
        }

        let result = self.start_owned_runtime();

        if let Err(error) = result {
            let message = sanitize_error(&error);
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
                inner.status.state = LocalModelRuntimeState::Stopped;
                inner.status.last_error = None;
                return Ok(());
            }

            inner.status.state = LocalModelRuntimeState::Failed;
            inner.status.last_error = Some(message);
            return Err(error);
        }

        Ok(())
    }

    fn start_owned_runtime(&self) -> io::Result<()> {
        let capability = self
            .config
            .pack
            .resolve(AiPackRole::Generation)
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
            inner.status.state = LocalModelRuntimeState::Ready;
            inner.status.pid = None;
            inner.status.started_at = Some(timestamp_now()?);
            inner.status.last_error = None;
            append_runtime_log(
                &logs.join("chat-runtime.log"),
                "Existing Qwen runtime detected",
            )?;
            return Ok(());
        }

        let lock_path = runtime_root.join("chat-runtime.lock");
        acquire_runtime_lock(&lock_path)?;

        {
            let mut inner = self.lock_inner()?;
            inner.lock_path = Some(lock_path);
        }

        append_runtime_log(
            &logs.join("chat-runtime.log"),
            "Starting Qwen3 chat runtime",
        )?;

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
                "GENERATION_MODEL_MISSING: ProgramData generation model file is missing",
            ));
        }

        let stdout = OpenOptions::new()
            .create(true)
            .append(true)
            .open(logs.join("chat-stdout.log"))?;

        let stderr = OpenOptions::new()
            .create(true)
            .append(true)
            .open(logs.join("chat-stderr.log"))?;

        let threads = thread::available_parallelism()
            .map(|count| count.get().saturating_sub(2).max(2))
            .unwrap_or(4);

        let mut command = Command::new(&server);
        command
            .args([
                "--model",
                model
                    .to_str()
                    .ok_or_else(|| io::Error::other("Qwen model path is invalid"))?,
                "--alias",
                capability.model_id.as_str(),
                "--host",
                "127.0.0.1",
                "--port",
                "42111",
                "--ctx-size",
                &self.config.context_size.to_string(),
                "--threads",
                &threads.to_string(),
                "--jinja",
                "--reasoning",
                "off",
            ])
            .current_dir(
                server
                    .parent()
                    .ok_or_else(|| io::Error::other("llama runtime directory is invalid"))?,
            )
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
                        "Qwen startup was cancelled",
                    ));
                }

                if let Some(process) = inner.process.as_mut()
                    && process.try_wait()?.is_some()
                {
                    return Err(io::Error::other("llama-server exited before readiness"));
                }
            }

            if health_ready(&self.config.endpoint)? {
                let mut inner = self.lock_inner()?;

                if inner.stop_requested {
                    return Err(io::Error::new(
                        io::ErrorKind::Interrupted,
                        "Qwen startup was cancelled",
                    ));
                }

                inner.status.state = LocalModelRuntimeState::Ready;
                inner.status.started_at = Some(timestamp_now()?);
                inner.status.last_error = None;

                append_runtime_log(&logs.join("chat-runtime.log"), "Qwen3 chat runtime ready")?;

                return Ok(());
            }

            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Qwen readiness timed out",
                ));
            }

            thread::sleep(self.config.readiness_interval);
        }
    }

    pub fn stop(&self) -> io::Result<()> {
        {
            let mut inner = self.lock_inner()?;

            if inner.status.state == LocalModelRuntimeState::Stopped {
                return Ok(());
            }

            inner.status.state = LocalModelRuntimeState::Stopping;
            inner.stop_requested = true;

            if let Some(process) = inner.process.as_mut() {
                request_stop_tree(process);
            } else {
                inner.status.state = LocalModelRuntimeState::Stopped;
                inner.status.pid = None;
                inner.status.started_at = None;
                inner.status.last_error = None;
                inner.stop_requested = false;
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

        let runtime_log = self
            .config
            .runtime_root()
            .join("logs")
            .join("chat-runtime.log");

        append_runtime_log(&runtime_log, "Qwen3 chat runtime stopped")?;

        let mut inner = self.lock_inner()?;
        inner.process = None;

        if let Some(lock_path) = inner.lock_path.take() {
            let _ = fs::remove_file(lock_path);
        }

        inner.status.state = LocalModelRuntimeState::Stopped;
        inner.status.pid = None;
        inner.status.started_at = None;
        inner.status.last_error = None;
        inner.stop_requested = false;

        Ok(())
    }

    fn lock_inner(&self) -> io::Result<MutexGuard<'_, RuntimeInner>> {
        self.inner
            .lock()
            .map_err(|_| io::Error::other("Local model runtime mutex poisoned"))
    }
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
                "Local model endpoint must use http://",
            )
        })?
        .split('/')
        .next()
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "Local model endpoint is invalid",
            )
        })?;

    let address: SocketAddr = authority.parse().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Local model endpoint must be a numeric loopback address",
        )
    })?;

    if !address.ip().is_loopback() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Local model endpoint must be loopback-only",
        ));
    }

    Ok((address, authority.to_owned()))
}

fn acquire_runtime_lock(lock_path: &Path) -> io::Result<()> {
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

fn sanitize_error(error: &io::Error) -> String {
    let message = error.to_string();
    if message.starts_with("AI_PACK_")
        || message.starts_with("MODEL_")
        || message.starts_with("GENERATION_MODEL_")
        || message.starts_with("EMBEDDING_MODEL_")
        || message.starts_with("RERANKER_MODEL_")
        || message.starts_with("LLAMA_RUNTIME_")
        || message.starts_with("PYTHON_RUNTIME_")
    {
        return message;
    }

    match error.kind() {
        io::ErrorKind::NotFound => "Local Qwen runtime component is missing".to_owned(),
        io::ErrorKind::TimedOut => "Local Qwen runtime startup timed out".to_owned(),
        io::ErrorKind::AlreadyExists => "Local Qwen runtime is already managed".to_owned(),
        _ => "Local Qwen runtime failed".to_owned(),
    }
}

fn ai_pack_io_error(error: crate::ai_pack::AiPackError) -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, error.to_string())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;
    use tempfile::tempdir;

    use super::*;
    use crate::ai_pack::R2hAiPackResolver;

    fn write_generation_manifest(root: &Path) -> io::Result<()> {
        let manifest_directory = root.join("local-ai/manifests");
        fs::create_dir_all(&manifest_directory)?;
        fs::write(
            manifest_directory.join("r2h-multi-evidence-models.json"),
            serde_json::to_vec(&json!({
                "schemaVersion": 1,
                "offlineOnly": true,
                "models": {
                    "generation": {
                        "id": "generation-test",
                        "role": "generation",
                        "path": "local-ai/models/generation/model.gguf",
                        "type": "gguf",
                        "provider": "node-llama-cpp",
                        "required": true,
                        "expectedFiles": ["model.gguf"]
                    }
                }
            }))?,
        )?;

        let model = root.join("local-ai/models/generation/model.gguf");
        fs::create_dir_all(
            model
                .parent()
                .ok_or_else(|| io::Error::other("model parent missing"))?,
        )?;
        fs::write(model, b"test model")?;
        Ok(())
    }

    #[test]
    fn custom_configuration_keeps_one_authoritative_pack_resolver() {
        let pack = R2hAiPackResolver::new("programdata-pack");
        let config =
            LocalModelRuntimeConfig::with_pack(PathBuf::from("project-root"), pack.clone());

        assert_eq!(config.pack.root(), pack.root());
        assert_eq!(config.endpoint, DEFAULT_ENDPOINT);
    }

    #[test]
    fn missing_programdata_http_runtime_is_reported_without_repo_fallback()
    -> Result<(), Box<dyn std::error::Error>> {
        let pack_directory = tempdir()?;
        write_generation_manifest(pack_directory.path())?;

        let project_directory = tempdir()?;
        let legacy_server = project_directory
            .path()
            .join("legacy-runtime/llama-server.exe");
        fs::create_dir_all(
            legacy_server
                .parent()
                .ok_or_else(|| io::Error::other("legacy runtime parent missing"))?,
        )?;
        fs::write(legacy_server, b"legacy runtime")?;

        let mut config = LocalModelRuntimeConfig::with_pack(
            project_directory.path().to_path_buf(),
            R2hAiPackResolver::new(pack_directory.path()),
        );
        config.startup_timeout = Duration::from_millis(50);
        config.readiness_interval = Duration::from_millis(1);

        let manager = LocalModelRuntimeManager::new(config);
        let Err(error) = manager.start() else {
            return Err("missing ProgramData server must fail closed".into());
        };
        let status = manager.status()?;

        assert!(error.to_string().contains("LLAMA_RUNTIME_MISSING"));
        assert_eq!(status.state, LocalModelRuntimeState::Failed);
        assert!(status.last_error.as_deref().is_some_and(|message| {
            message.starts_with("LLAMA_RUNTIME_MISSING: ProgramData HTTP runtime is missing")
        }));
        Ok(())
    }
}

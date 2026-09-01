use std::{
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    net::{SocketAddr, TcpStream},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{Arc, Mutex, MutexGuard},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::Serialize;

const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:42110";

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub enum KhojRuntimeState {
    Stopped,
    Starting,
    Ready,
    Failed,
    Stopping,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct KhojRuntimeStatus {
    pub state: KhojRuntimeState,
    pub pid: Option<u32>,
    pub endpoint: String,
    pub started_at: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct KhojRuntimeConfig {
    pub project_root: PathBuf,
    pub endpoint: String,
    pub startup_timeout: Duration,
    pub readiness_interval: Duration,
    pub shutdown_timeout: Duration,
}

impl KhojRuntimeConfig {
    pub fn development(project_root: PathBuf) -> Self {
        Self {
            project_root,
            endpoint: DEFAULT_ENDPOINT.to_owned(),
            startup_timeout: Duration::from_secs(600),
            readiness_interval: Duration::from_millis(500),
            shutdown_timeout: Duration::from_secs(15),
        }
    }

    fn runtime_root(&self) -> PathBuf {
        self.project_root.join("run-data").join("khoj")
    }
}

pub trait KhojProcess: Send {
    fn id(&self) -> u32;
    fn request_stop(&mut self) -> io::Result<()>;
    fn has_exited(&mut self) -> io::Result<bool>;
    fn force_kill_tree(&mut self) -> io::Result<()>;
}

pub trait KhojProcessLauncher: Send + Sync {
    fn launch(
        &self,
        config: &KhojRuntimeConfig,
        stdout_log: &Path,
        stderr_log: &Path,
    ) -> io::Result<Box<dyn KhojProcess>>;
}

pub trait KhojReadinessProbe: Send + Sync {
    fn is_ready(&self, endpoint: &str) -> io::Result<bool>;
}

#[derive(Clone)]
pub struct KhojRuntimeManager {
    config: KhojRuntimeConfig,
    launcher: Arc<dyn KhojProcessLauncher>,
    probe: Arc<dyn KhojReadinessProbe>,
    inner: Arc<Mutex<RuntimeInner>>,
}

struct RuntimeInner {
    status: KhojRuntimeStatus,
    process: Option<Box<dyn KhojProcess>>,
    lock_path: Option<PathBuf>,
    stop_requested: bool,
}

impl KhojRuntimeManager {
    pub fn new(config: KhojRuntimeConfig) -> Self {
        Self::with_dependencies(config, Arc::new(SystemLauncher), Arc::new(HttpProbe))
    }

    pub fn with_dependencies(
        config: KhojRuntimeConfig,
        launcher: Arc<dyn KhojProcessLauncher>,
        probe: Arc<dyn KhojReadinessProbe>,
    ) -> Self {
        let endpoint = config.endpoint.clone();
        Self {
            config,
            launcher,
            probe,
            inner: Arc::new(Mutex::new(RuntimeInner {
                status: KhojRuntimeStatus {
                    state: KhojRuntimeState::Stopped,
                    pid: None,
                    endpoint,
                    started_at: None,
                    last_error: None,
                },
                process: None,
                lock_path: None,
                stop_requested: false,
            })),
        }
    }

    pub fn status(&self) -> io::Result<KhojRuntimeStatus> {
        Ok(self.lock_inner()?.status.clone())
    }

    pub fn start(&self) -> io::Result<()> {
        {
            let mut inner = self.lock_inner()?;
            if matches!(
                inner.status.state,
                KhojRuntimeState::Starting | KhojRuntimeState::Ready
            ) {
                return Ok(());
            }
            inner.status.state = KhojRuntimeState::Starting;
            inner.status.last_error = None;
            inner.stop_requested = false;
        }

        let result = self.start_owned_runtime();
        if let Err(error) = result {
            let message = sanitize_error(&error);
            let mut inner = self.lock_inner()?;
            if let Some(process) = inner.process.as_mut() {
                let _ = process.force_kill_tree();
            }
            inner.process = None;
            if let Some(lock_path) = inner.lock_path.take() {
                let _ = fs::remove_file(lock_path);
            }
            inner.status.pid = None;
            inner.status.started_at = None;
            if inner.stop_requested || error.kind() == io::ErrorKind::Interrupted {
                inner.status.state = KhojRuntimeState::Stopped;
                inner.status.last_error = None;
                return Ok(());
            }
            inner.status.state = KhojRuntimeState::Failed;
            inner.status.last_error = Some(message);
            return Err(error);
        }
        Ok(())
    }

    fn start_owned_runtime(&self) -> io::Result<()> {
        let runtime_root = self.config.runtime_root();
        let logs = runtime_root.join("logs");
        fs::create_dir_all(&logs)?;

        let lock_path = runtime_root.join("runtime.lock");
        acquire_runtime_lock(&lock_path)?;
        {
            let mut inner = self.lock_inner()?;
            inner.lock_path = Some(lock_path.clone());
        }
        append_runtime_log(&logs.join("khoj-runtime.log"), "Starting Khoj runtime")?;

        let stdout_log = logs.join("khoj-stdout.log");
        let stderr_log = logs.join("khoj-stderr.log");
        let process = self
            .launcher
            .launch(&self.config, &stdout_log, &stderr_log)?;
        let pid = process.id();

        {
            let mut inner = self.lock_inner()?;
            inner.status.pid = Some(pid);
            inner.process = Some(process);
        }

        let deadline = Instant::now() + self.config.startup_timeout;
        loop {
            {
                let mut inner = self.lock_inner()?;
                if inner.stop_requested {
                    return Err(io::Error::new(
                        io::ErrorKind::Interrupted,
                        "Khoj startup was cancelled",
                    ));
                }
                if let Some(process) = inner.process.as_mut()
                    && process.has_exited()?
                {
                    return Err(io::Error::other("Khoj process exited before readiness"));
                }
            }

            if self.probe.is_ready(&self.config.endpoint)? {
                let mut inner = self.lock_inner()?;
                if inner.stop_requested {
                    return Err(io::Error::new(
                        io::ErrorKind::Interrupted,
                        "Khoj startup was cancelled",
                    ));
                }
                inner.status.state = KhojRuntimeState::Ready;
                inner.status.started_at = Some(timestamp_now()?);
                append_runtime_log(&logs.join("khoj-runtime.log"), "Khoj runtime ready")?;
                return Ok(());
            }

            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Khoj readiness timed out",
                ));
            }
            thread::sleep(self.config.readiness_interval);
        }
    }

    pub fn stop(&self) -> io::Result<()> {
        {
            let mut inner = self.lock_inner()?;
            if inner.status.state == KhojRuntimeState::Stopped {
                return Ok(());
            }
            inner.status.state = KhojRuntimeState::Stopping;
            inner.stop_requested = true;
            if let Some(process) = inner.process.as_mut() {
                process.request_stop()?;
            }
        }

        let deadline = Instant::now() + self.config.shutdown_timeout;
        loop {
            let exited = {
                let mut inner = self.lock_inner()?;
                match inner.process.as_mut() {
                    Some(process) => process.has_exited()?,
                    None => true,
                }
            };
            if exited {
                break;
            }
            if Instant::now() >= deadline {
                let mut inner = self.lock_inner()?;
                if let Some(process) = inner.process.as_mut() {
                    process.force_kill_tree()?;
                }
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }

        let mut inner = self.lock_inner()?;
        inner.process = None;
        if let Some(lock_path) = inner.lock_path.take() {
            let _ = fs::remove_file(lock_path);
        }
        inner.status.state = KhojRuntimeState::Stopped;
        inner.status.pid = None;
        inner.status.started_at = None;
        inner.status.last_error = None;
        inner.stop_requested = false;
        Ok(())
    }

    fn lock_inner(&self) -> io::Result<MutexGuard<'_, RuntimeInner>> {
        self.inner
            .lock()
            .map_err(|_| io::Error::other("Khoj runtime mutex poisoned"))
    }
}

struct SystemLauncher;

impl KhojProcessLauncher for SystemLauncher {
    fn launch(
        &self,
        config: &KhojRuntimeConfig,
        stdout_log: &Path,
        stderr_log: &Path,
    ) -> io::Result<Box<dyn KhojProcess>> {
        let python = config
            .project_root
            .join("engines")
            .join("khoj")
            .join(".venv")
            .join("Scripts")
            .join("python.exe");
        let wrapper = config
            .project_root
            .join("scripts")
            .join("run-khoj-windows.py");
        if !python.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Khoj Python runtime is missing",
            ));
        }
        if !wrapper.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Khoj launcher is missing",
            ));
        }

        let stdout = OpenOptions::new()
            .create(true)
            .append(true)
            .open(stdout_log)?;
        let stderr = OpenOptions::new()
            .create(true)
            .append(true)
            .open(stderr_log)?;
        let mut command = Command::new(python);
        command
            .arg(wrapper)
            .current_dir(&config.project_root)
            .env("OPENAI_BASE_URL", "http://127.0.0.1:42111/v1/")
            .env("OPENAI_API_KEY", "r2h-local")
            .env("KHOJ_DEFAULT_CHAT_MODEL", "qwen3-4b-q4km-generation")
            .env("R2H_KHOJ_LOCAL_MODEL_PROVIDER", "llama.cpp")
            .stdin(Stdio::piped())
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr));
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let mut child = command.spawn()?;
        let stdin = child.stdin.take();
        Ok(Box::new(SystemProcess { child, stdin }))
    }
}

struct SystemProcess {
    child: Child,
    stdin: Option<ChildStdin>,
}

impl KhojProcess for SystemProcess {
    fn id(&self) -> u32 {
        self.child.id()
    }

    fn request_stop(&mut self) -> io::Result<()> {
        if let Some(stdin) = self.stdin.as_mut() {
            stdin.write_all(b"STOP\n")?;
            stdin.flush()?;
        }
        Ok(())
    }

    fn has_exited(&mut self) -> io::Result<bool> {
        Ok(self.child.try_wait()?.is_some())
    }

    fn force_kill_tree(&mut self) -> io::Result<()> {
        #[cfg(windows)]
        {
            let status = Command::new("taskkill")
                .args(["/PID", &self.child.id().to_string(), "/T", "/F"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()?;
            if !status.success() && self.child.try_wait()?.is_none() {
                self.child.kill()?;
            }
        }
        #[cfg(not(windows))]
        {
            self.child.kill()?;
        }
        Ok(())
    }
}

struct HttpProbe;

impl KhojReadinessProbe for HttpProbe {
    fn is_ready(&self, endpoint: &str) -> io::Result<bool> {
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
        stream.set_read_timeout(Some(Duration::from_millis(500)))?;
        stream.set_write_timeout(Some(Duration::from_millis(500)))?;
        write!(
            stream,
            "GET / HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
        )?;
        let mut response = Vec::with_capacity(8192);
        let mut buffer = [0_u8; 1024];

        loop {
            match stream.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    response.extend_from_slice(&buffer[..count]);

                    if response.windows(4).any(|window| window == b"\r\n\r\n")
                        || response.len() >= 8192
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
        let status_ok = text.starts_with("HTTP/1.1 200") || text.starts_with("HTTP/1.0 200");

        Ok(status_ok)
    }
}

fn endpoint_address(endpoint: &str) -> io::Result<(SocketAddr, String)> {
    let authority = endpoint
        .strip_prefix("http://")
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "Khoj endpoint must use http://",
            )
        })?
        .split('/')
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Khoj endpoint is invalid"))?;
    let address: SocketAddr = authority.parse().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Khoj endpoint must be a numeric loopback address",
        )
    })?;
    if !address.ip().is_loopback() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Khoj endpoint must be loopback-only",
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
            let owner = fs::read_to_string(lock_path)
                .ok()
                .and_then(|value| value.trim().parse::<u32>().ok());
            if owner.is_some_and(process_is_running) {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "Khoj runtime is already managed",
                ));
            }
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

fn process_is_running(pid: u32) -> bool {
    #[cfg(windows)]
    {
        let filter = format!("PID eq {pid}");
        Command::new("tasklist")
            .args(["/FI", &filter, "/NH"])
            .output()
            .map(|output| String::from_utf8_lossy(&output.stdout).contains(&pid.to_string()))
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        let _ = pid;
        false
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
    match error.kind() {
        io::ErrorKind::NotFound => "Khoj runtime component is missing".to_owned(),
        io::ErrorKind::TimedOut => "Khoj startup timed out".to_owned(),
        io::ErrorKind::AlreadyExists => "Khoj runtime is already managed".to_owned(),
        _ => "Khoj runtime failed".to_owned(),
    }
}

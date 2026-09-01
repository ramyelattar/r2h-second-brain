use std::{
    io,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use r2h_second_brain_desktop::{
    KhojProcess, KhojProcessLauncher, KhojReadinessProbe, KhojRuntimeConfig, KhojRuntimeManager,
    KhojRuntimeState,
};
use tempfile::TempDir;

#[derive(Default)]
struct FakeProcessState {
    stop_requested: bool,
    killed: bool,
    exited: bool,
}

struct FakeProcess {
    state: Arc<Mutex<FakeProcessState>>,
}

impl KhojProcess for FakeProcess {
    fn id(&self) -> u32 {
        4242
    }

    fn request_stop(&mut self) -> io::Result<()> {
        self.state.lock().map_err(lock_error)?.stop_requested = true;
        self.state.lock().map_err(lock_error)?.exited = true;
        Ok(())
    }

    fn has_exited(&mut self) -> io::Result<bool> {
        Ok(self.state.lock().map_err(lock_error)?.exited)
    }

    fn force_kill_tree(&mut self) -> io::Result<()> {
        let mut state = self.state.lock().map_err(lock_error)?;
        state.killed = true;
        state.exited = true;
        Ok(())
    }
}

struct FakeLauncher {
    launches: Arc<Mutex<u32>>,
    process_state: Arc<Mutex<FakeProcessState>>,
}

impl KhojProcessLauncher for FakeLauncher {
    fn launch(
        &self,
        _config: &KhojRuntimeConfig,
        _stdout_log: &std::path::Path,
        _stderr_log: &std::path::Path,
    ) -> io::Result<Box<dyn KhojProcess>> {
        *self.launches.lock().map_err(lock_error)? += 1;
        Ok(Box::new(FakeProcess {
            state: Arc::clone(&self.process_state),
        }))
    }
}

struct SequenceProbe {
    attempts: Mutex<Vec<bool>>,
}

impl KhojReadinessProbe for SequenceProbe {
    fn is_ready(&self, _endpoint: &str) -> io::Result<bool> {
        let mut attempts = self.attempts.lock().map_err(lock_error)?;
        if attempts.is_empty() {
            return Ok(false);
        }
        Ok(attempts.remove(0))
    }
}

#[test]
fn start_transitions_to_ready_and_records_pid() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(vec![false, true])?;

    fixture.manager.start()?;

    let status = fixture.manager.status()?;
    assert_eq!(status.state, KhojRuntimeState::Ready);
    assert_eq!(status.pid, Some(4242));
    assert!(status.started_at.is_some());
    assert_eq!(*fixture.launches.lock().map_err(lock_error)?, 1);
    Ok(())
}

#[test]
fn second_start_is_idempotent_while_ready() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(vec![true])?;

    fixture.manager.start()?;
    fixture.manager.start()?;

    assert_eq!(*fixture.launches.lock().map_err(lock_error)?, 1);
    assert_eq!(fixture.manager.status()?.state, KhojRuntimeState::Ready);
    Ok(())
}

#[test]
fn stop_requests_graceful_shutdown_and_returns_to_stopped() -> Result<(), Box<dyn std::error::Error>>
{
    let fixture = Fixture::new(vec![true])?;
    fixture.manager.start()?;

    fixture.manager.stop()?;

    let process = fixture.process_state.lock().map_err(lock_error)?;
    assert!(process.stop_requested);
    assert!(!process.killed);
    assert_eq!(fixture.manager.status()?.state, KhojRuntimeState::Stopped);
    Ok(())
}

#[test]
fn readiness_timeout_marks_runtime_failed_and_kills_process()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(vec![false, false, false])?;

    let result = fixture.manager.start();

    assert!(result.is_err());
    assert_eq!(fixture.manager.status()?.state, KhojRuntimeState::Failed);
    assert!(fixture.manager.status()?.last_error.is_some());
    assert!(fixture.process_state.lock().map_err(lock_error)?.killed);
    Ok(())
}

struct Fixture {
    _directory: TempDir,
    manager: KhojRuntimeManager,
    launches: Arc<Mutex<u32>>,
    process_state: Arc<Mutex<FakeProcessState>>,
}

impl Fixture {
    fn new(readiness: Vec<bool>) -> Result<Self, Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let launches = Arc::new(Mutex::new(0));
        let process_state = Arc::new(Mutex::new(FakeProcessState::default()));
        let launcher = FakeLauncher {
            launches: Arc::clone(&launches),
            process_state: Arc::clone(&process_state),
        };
        let probe = SequenceProbe {
            attempts: Mutex::new(readiness),
        };
        let config = KhojRuntimeConfig {
            project_root: PathBuf::from(directory.path()),
            endpoint: "http://127.0.0.1:42110".to_owned(),
            startup_timeout: Duration::from_millis(15),
            readiness_interval: Duration::from_millis(1),
            shutdown_timeout: Duration::from_millis(15),
        };
        let manager =
            KhojRuntimeManager::with_dependencies(config, Arc::new(launcher), Arc::new(probe));
        Ok(Self {
            _directory: directory,
            manager,
            launches,
            process_state,
        })
    }
}

fn lock_error<T>(_error: std::sync::PoisonError<T>) -> io::Error {
    io::Error::other("test mutex poisoned")
}

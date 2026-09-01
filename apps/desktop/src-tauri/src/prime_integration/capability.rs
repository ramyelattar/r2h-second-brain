// Capability management keeps API surface beyond the currently registered
// local generation provider.
#![allow(dead_code)]

use std::{
    collections::HashMap,
    fmt::{self, Debug, Display, Formatter},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use knowledge_domain::WorkspaceId;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

pub(crate) const METHOD_PROVIDER_GENERATE: &str = "prime.provider.generate.v1";
pub(crate) const METHOD_BROKER_HELLO: &str = "broker.hello.v1";
pub(crate) const METHOD_BROKER_PING: &str = "broker.ping.v1";

pub(crate) const ALLOWED_METHODS: [&str; 3] = [
    METHOD_PROVIDER_GENERATE,
    METHOD_BROKER_HELLO,
    METHOD_BROKER_PING,
];

const PIPE_PREFIX: &str = r"\\.\pipe\r2h-prime-";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CapabilityLimits {
    pub(crate) max_active_sessions: usize,
    pub(crate) session_ttl: Duration,
    pub(crate) max_concurrent_requests_per_session: usize,
}

impl Default for CapabilityLimits {
    fn default() -> Self {
        Self {
            max_active_sessions: 8,
            session_ttl: Duration::from_secs(15 * 60),
            max_concurrent_requests_per_session: 1,
        }
    }
}

#[derive(Clone)]
pub(crate) struct CapabilityGrant {
    session_id: String,
    capability: CapabilitySecret,
    pipe_name: String,
}

impl CapabilityGrant {
    pub(crate) fn session_id(&self) -> &str {
        &self.session_id
    }

    pub(crate) fn capability(&self) -> &str {
        self.capability.as_str()
    }

    pub(crate) fn pipe_name(&self) -> &str {
        &self.pipe_name
    }

    pub(crate) fn allowed_methods(&self) -> &'static [&'static str] {
        &ALLOWED_METHODS
    }
}

impl Debug for CapabilityGrant {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CapabilityGrant")
            .field("session_id", &self.session_id)
            .field("capability", &"[REDACTED]")
            .field("pipe_name", &self.pipe_name)
            .finish()
    }
}

#[derive(Clone)]
struct CapabilitySecret(String);

impl CapabilitySecret {
    fn generate() -> Self {
        Self(format!("{}{}", WorkspaceId::new(), WorkspaceId::new()))
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

impl Debug for CapabilitySecret {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

struct CapabilitySession {
    capability: CapabilitySecret,
    expires_at: Instant,
    revoked: bool,
    request_slots: Arc<Semaphore>,
}

#[derive(Clone)]
pub(crate) struct CapabilityStore {
    inner: Arc<Mutex<HashMap<String, CapabilitySession>>>,
    limits: CapabilityLimits,
}

impl CapabilityStore {
    pub(crate) fn new(limits: CapabilityLimits) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            limits,
        }
    }

    pub(crate) fn create_session(&self) -> Result<CapabilityGrant, CapabilityError> {
        let now = Instant::now();
        let mut sessions = self
            .inner
            .lock()
            .map_err(|_| CapabilityError::Unavailable)?;

        remove_inactive_sessions(&mut sessions, now);

        if sessions.len() >= self.limits.max_active_sessions {
            return Err(CapabilityError::SessionLimitReached);
        }

        let session_id = loop {
            let candidate = random_session_id();
            if !sessions.contains_key(&candidate) {
                break candidate;
            }
        };
        let capability = CapabilitySecret::generate();
        let pipe_name = format!("{PIPE_PREFIX}{session_id}");
        let request_slots = Arc::new(Semaphore::new(
            self.limits.max_concurrent_requests_per_session,
        ));

        sessions.insert(
            session_id.clone(),
            CapabilitySession {
                capability: capability.clone(),
                expires_at: now + self.limits.session_ttl,
                revoked: false,
                request_slots,
            },
        );

        Ok(CapabilityGrant {
            session_id,
            capability,
            pipe_name,
        })
    }

    pub(crate) fn acquire(
        &self,
        session_id: &str,
        capability: &str,
    ) -> Result<CapabilityLease, CapabilityError> {
        let now = Instant::now();
        let sessions = self
            .inner
            .lock()
            .map_err(|_| CapabilityError::Unavailable)?;
        let session = sessions
            .get(session_id)
            .ok_or(CapabilityError::UnknownSession)?;

        if session.revoked {
            return Err(CapabilityError::Revoked);
        }
        if now >= session.expires_at {
            return Err(CapabilityError::Expired);
        }
        if !constant_time_equal(
            session.capability.as_str().as_bytes(),
            capability.as_bytes(),
        ) {
            return Err(CapabilityError::InvalidSecret);
        }

        let permit = session
            .request_slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| CapabilityError::RequestLimitReached)?;

        Ok(CapabilityLease { _permit: permit })
    }

    pub(crate) fn revoke(&self, session_id: &str) -> Result<bool, CapabilityError> {
        let mut sessions = self
            .inner
            .lock()
            .map_err(|_| CapabilityError::Unavailable)?;
        let Some(session) = sessions.get_mut(session_id) else {
            return Ok(false);
        };

        session.revoked = true;
        Ok(true)
    }

    pub(crate) fn cleanup(&self) -> Result<usize, CapabilityError> {
        let mut sessions = self
            .inner
            .lock()
            .map_err(|_| CapabilityError::Unavailable)?;
        let before = sessions.len();
        remove_inactive_sessions(&mut sessions, Instant::now());
        Ok(before.saturating_sub(sessions.len()))
    }

    pub(crate) fn active_session_count(&self) -> Result<usize, CapabilityError> {
        let sessions = self
            .inner
            .lock()
            .map_err(|_| CapabilityError::Unavailable)?;
        Ok(sessions.len())
    }
}

pub(crate) struct CapabilityLease {
    _permit: OwnedSemaphorePermit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CapabilityError {
    UnknownSession,
    InvalidSecret,
    Expired,
    Revoked,
    RequestLimitReached,
    SessionLimitReached,
    Unavailable,
}

impl Display for CapabilityError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::UnknownSession => "capability session is unknown",
            Self::InvalidSecret => "capability is invalid",
            Self::Expired => "capability has expired",
            Self::Revoked => "capability has been revoked",
            Self::RequestLimitReached => "capability request limit reached",
            Self::SessionLimitReached => "capability session limit reached",
            Self::Unavailable => "capability store is unavailable",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for CapabilityError {}

fn random_session_id() -> String {
    WorkspaceId::new().to_string()
}

fn remove_inactive_sessions(sessions: &mut HashMap<String, CapabilitySession>, now: Instant) {
    sessions.retain(|_, session| !session.revoked && now < session.expires_at);
}

fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }

    let mut difference = 0_u8;
    for (left, right) in left.iter().zip(right.iter()) {
        difference |= left ^ right;
    }
    difference == 0
}

#[cfg(test)]
mod tests {
    use std::{thread, time::Duration};

    use super::*;

    #[test]
    fn creates_unguessable_memory_only_grants_with_fixed_methods() -> Result<(), CapabilityError> {
        let store = CapabilityStore::new(CapabilityLimits::default());
        let first = store.create_session()?;
        let second = store.create_session()?;

        assert!(!first.session_id().is_empty());
        assert!(!first.capability().is_empty());
        assert_ne!(first.session_id(), second.session_id());
        assert_ne!(first.capability(), second.capability());
        assert_eq!(first.allowed_methods(), &ALLOWED_METHODS);
        assert!(first.pipe_name().starts_with(PIPE_PREFIX));
        assert!(!first.pipe_name().contains("workspace"));
        assert!(!first.pipe_name().contains("token"));

        let debug = format!("{first:?}");
        assert!(!debug.contains(first.capability()));

        Ok(())
    }

    #[test]
    fn validates_wrong_expired_revoked_and_mismatched_capabilities() -> Result<(), CapabilityError>
    {
        let store = CapabilityStore::new(CapabilityLimits {
            session_ttl: Duration::from_millis(5),
            ..CapabilityLimits::default()
        });
        let grant = store.create_session()?;
        let other = store.create_session()?;

        assert_eq!(
            store.acquire(grant.session_id(), "wrong").err(),
            Some(CapabilityError::InvalidSecret)
        );
        assert_eq!(
            store.acquire(other.session_id(), grant.capability()).err(),
            Some(CapabilityError::InvalidSecret)
        );

        thread::sleep(Duration::from_millis(10));
        assert_eq!(
            store.acquire(grant.session_id(), grant.capability()).err(),
            Some(CapabilityError::Expired)
        );

        let revoked = store.create_session()?;
        assert!(store.revoke(revoked.session_id())?);
        assert_eq!(
            store
                .acquire(revoked.session_id(), revoked.capability())
                .err(),
            Some(CapabilityError::Revoked)
        );

        Ok(())
    }

    #[test]
    fn cleanup_removes_expired_and_revoked_sessions() -> Result<(), CapabilityError> {
        let store = CapabilityStore::new(CapabilityLimits {
            session_ttl: Duration::from_millis(5),
            ..CapabilityLimits::default()
        });
        let expired = store.create_session()?;
        let revoked = store.create_session()?;
        assert!(store.revoke(revoked.session_id())?);
        thread::sleep(Duration::from_millis(10));

        assert_eq!(store.active_session_count()?, 2);
        assert_eq!(store.cleanup()?, 2);
        assert_eq!(store.active_session_count()?, 0);
        assert_eq!(
            store
                .acquire(expired.session_id(), expired.capability())
                .err(),
            Some(CapabilityError::UnknownSession)
        );

        Ok(())
    }

    #[test]
    fn per_session_request_limit_is_bounded() -> Result<(), CapabilityError> {
        let store = CapabilityStore::new(CapabilityLimits::default());
        let grant = store.create_session()?;
        let permit = store.acquire(grant.session_id(), grant.capability())?;

        assert_eq!(
            store.acquire(grant.session_id(), grant.capability()).err(),
            Some(CapabilityError::RequestLimitReached)
        );
        drop(permit);
        assert!(
            store
                .acquire(grant.session_id(), grant.capability())
                .is_ok()
        );

        Ok(())
    }
}

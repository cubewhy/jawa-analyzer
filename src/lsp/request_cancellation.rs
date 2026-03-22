use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

use dashmap::DashMap;
use tokio::sync::Notify;
use tower_lsp::jsonrpc;
use tower_lsp::lsp_types::Url;

pub type RequestResult<T> = Result<T, Cancelled>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cancelled {
    Client,
    Superseded,
}

impl Cancelled {
    const fn as_u8(self) -> u8 {
        match self {
            Self::Client => 1,
            Self::Superseded => 2,
        }
    }

    const fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Client),
            2 => Some(Self::Superseded),
            _ => None,
        }
    }

    pub fn into_lsp_error(self) -> jsonrpc::Error {
        match self {
            Self::Client | Self::Superseded => jsonrpc::Error::request_cancelled(),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Client => "client",
            Self::Superseded => "superseded",
        }
    }
}

#[derive(Debug)]
struct CancellationState {
    reason: AtomicU8,
    notify: Notify,
}

#[derive(Debug, Clone)]
pub struct CancellationToken {
    state: Arc<CancellationState>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self {
            state: Arc::new(CancellationState {
                reason: AtomicU8::new(0),
                notify: Notify::new(),
            }),
        }
    }

    pub fn cancel(&self, reason: Cancelled) -> bool {
        let updated = self
            .state
            .reason
            .compare_exchange(0, reason.as_u8(), Ordering::AcqRel, Ordering::Acquire)
            .is_ok();
        if updated {
            self.state.notify.notify_waiters();
        }
        updated
    }

    pub fn is_cancelled(&self) -> bool {
        self.reason().is_some()
    }

    pub fn reason(&self) -> Option<Cancelled> {
        Cancelled::from_u8(self.state.reason.load(Ordering::Acquire))
    }

    pub fn check(&self) -> RequestResult<()> {
        match self.reason() {
            Some(reason) => Err(reason),
            None => Ok(()),
        }
    }

    pub async fn cancelled(&self) -> Cancelled {
        loop {
            if let Some(reason) = self.reason() {
                return reason;
            }
            self.state.notify.notified().await;
        }
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RequestFamily {
    Completion,
    GotoDefinition,
    InlayHints,
    SemanticTokensFull,
    SemanticTokensRange,
    DocumentSymbol,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct RequestKey {
    family: RequestFamily,
    uri: Arc<str>,
}

impl RequestKey {
    fn new(family: RequestFamily, uri: &Url) -> Self {
        Self {
            family,
            uri: Arc::from(uri.as_str()),
        }
    }
}

#[derive(Debug, Clone)]
struct ActiveRequest {
    generation: u64,
    token: CancellationToken,
}

#[derive(Debug)]
pub struct RequestCancellationManager {
    next_generation: AtomicU64,
    active: DashMap<RequestKey, ActiveRequest>,
}

impl RequestCancellationManager {
    pub fn new() -> Self {
        Self {
            next_generation: AtomicU64::new(1),
            active: DashMap::new(),
        }
    }

    pub fn begin(self: &Arc<Self>, family: RequestFamily, uri: &Url) -> RequestGuard {
        let key = RequestKey::new(family, uri);
        let generation = self.next_generation.fetch_add(1, Ordering::Relaxed);
        let token = CancellationToken::new();
        let previous = self.active.insert(
            key.clone(),
            ActiveRequest {
                generation,
                token: token.clone(),
            },
        );

        if let Some(previous) = previous
            && previous.token.cancel(Cancelled::Superseded)
        {
            tracing::debug!(
                request_family = ?family,
                uri = %key.uri,
                previous_generation = previous.generation,
                generation,
                cancel_reason = %Cancelled::Superseded.as_str(),
                "superseded older request generation"
            );
        }

        tracing::debug!(
            request_family = ?family,
            uri = %key.uri,
            generation,
            "registered request generation"
        );

        RequestGuard {
            manager: Arc::clone(self),
            key,
            generation,
            token,
            completed: false,
        }
    }
}

impl Default for RequestCancellationManager {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct RequestGuard {
    manager: Arc<RequestCancellationManager>,
    key: RequestKey,
    generation: u64,
    token: CancellationToken,
    completed: bool,
}

impl RequestGuard {
    pub fn token(&self) -> &CancellationToken {
        &self.token
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn family(&self) -> RequestFamily {
        self.key.family
    }

    pub fn finish(mut self) {
        self.completed = true;
    }
}

impl Drop for RequestGuard {
    fn drop(&mut self) {
        if !self.completed && self.token.cancel(Cancelled::Client) {
            tracing::debug!(
                request_family = ?self.key.family,
                uri = %self.key.uri,
                generation = self.generation,
                cancel_reason = %Cancelled::Client.as_str(),
                "request guard dropped before completion"
            );
        }

        let _ = self
            .manager
            .active
            .remove_if(&self.key, |_, active| active.generation == self.generation);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropped_guard_marks_request_cancelled() {
        let manager = Arc::new(RequestCancellationManager::new());
        let uri = Url::parse("file:///test.java").expect("uri");

        let guard = manager.begin(RequestFamily::Completion, &uri);
        let token = guard.token().clone();
        drop(guard);

        assert_eq!(token.reason(), Some(Cancelled::Client));
    }

    #[test]
    fn newer_request_supersedes_older_generation() {
        let manager = Arc::new(RequestCancellationManager::new());
        let uri = Url::parse("file:///test.java").expect("uri");

        let first = manager.begin(RequestFamily::Completion, &uri);
        let first_token = first.token().clone();
        let second = manager.begin(RequestFamily::Completion, &uri);

        assert_eq!(first_token.reason(), Some(Cancelled::Superseded));
        assert_eq!(second.token().reason(), None);

        second.finish();
        drop(first);
    }
}

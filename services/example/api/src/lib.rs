//! The composition root: the one place that knows which adapter backs each port, and runs the servers.

use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use example_application::CreateNote;
use example_infrastructure::MemoryNoteRepository;
use platform_clock::SystemClock;
use platform_http::{Limits, Server, ServerError};
use platform_task::{TaskErrors, Tasks};
use tokio_util::sync::CancellationToken;

const LIMITS: Limits = Limits {
    request_timeout: Duration::from_secs(10),
    shutdown_timeout: Duration::from_secs(10),
    max_body_bytes: 1024 * 1024,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub addr: String,
}

impl Config {
    /// `get` reads one variable; main passes the process environment, a test passes what it needs.
    pub fn from_env(get: impl Fn(&str) -> Option<String>) -> Self {
        Self {
            addr: get("ADDR").unwrap_or_else(|| "0.0.0.0:8080".to_owned()),
        }
    }
}

pub struct Service {
    api: Server,
}

impl Service {
    /// Binds every listener before anything runs, so a bad or taken address fails at startup.
    pub async fn start(config: &Config) -> Result<Self, ServerError> {
        let create = CreateNote::new(MemoryNoteRepository::default(), SystemClock);
        let api = Server::bind(&config.addr, example_interfaces::router(Arc::new(create)), LIMITS).await?;
        Ok(Self { api })
    }

    pub fn api_addr(&self) -> io::Result<SocketAddr> {
        self.api.local_addr()
    }

    /// Serves until `token` is cancelled or a server fails; a failure of one stops the others.
    pub async fn run(self, token: &CancellationToken) -> Result<(), TaskErrors> {
        // The supervisor waits longer than the server drains, so it never cuts a drain short.
        let mut tasks = Tasks::new(token, LIMITS.shutdown_timeout + Duration::from_secs(5));
        let api = self.api;
        tasks.spawn(
            "api",
            move |token| async move { api.run(token).await.map_err(Into::into) },
        );
        tasks.wait().await
    }
}

#[cfg(test)]
mod tests;

//! The one way a service serves HTTP (ADR-0009). [`Limits`] has no default: every limit is chosen, and zero is refused
//! because it would mean none.

use std::io;
use std::net::SocketAddr;
use std::pin::pin;
use std::time::Duration;

use axum::Router;
use axum::http::StatusCode;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::timeout::TimeoutLayer;

/// What is bounded: the time a handler takes to produce a response, the request body, and the shutdown drain.
/// Request headers get hyper's own 30-second read timeout. Not bounded here yet: idle keep-alive connections and the
/// time to stream a response body (docs/deferred.md).
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// A handler still running after this gets 503 and is dropped.
    pub request_timeout: Duration,
    /// How long in-flight requests may finish after shutdown starts.
    pub shutdown_timeout: Duration,
    /// A larger body gets 413: up front from its length, or as it streams in.
    pub max_body_bytes: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("{0} must be greater than zero: zero would mean no limit")]
    ZeroLimit(&'static str),
    #[error("bind {addr}")]
    Bind { addr: String, source: io::Error },
    #[error("serve")]
    Serve(#[source] io::Error),
    #[error("in-flight requests did not finish within {0:?} of shutdown")]
    Drain(Duration),
}

pub struct Server {
    listener: TcpListener,
    router: Router,
    shutdown_timeout: Duration,
}

impl Server {
    /// Binds now, so a taken or malformed address fails at startup rather than when serving begins.
    pub async fn bind(addr: &str, router: Router, limits: Limits) -> Result<Self, ServerError> {
        let zero = [
            ("request_timeout", limits.request_timeout.is_zero()),
            ("shutdown_timeout", limits.shutdown_timeout.is_zero()),
            ("max_body_bytes", limits.max_body_bytes == 0),
        ];
        if let Some((name, _)) = zero.into_iter().find(|(_, is_zero)| *is_zero) {
            return Err(ServerError::ZeroLimit(name));
        }
        let listener = TcpListener::bind(addr).await.map_err(|source| ServerError::Bind {
            addr: addr.to_owned(),
            source,
        })?;
        // The first layer is the outermost: a panic anywhere below becomes a 500.
        let router = router.layer((
            CatchPanicLayer::new(),
            TimeoutLayer::with_status_code(StatusCode::SERVICE_UNAVAILABLE, limits.request_timeout),
            RequestBodyLimitLayer::new(limits.max_body_bytes),
        ));
        Ok(Self {
            listener,
            router,
            shutdown_timeout: limits.shutdown_timeout,
        })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    /// Serves until `token` is cancelled, then lets in-flight requests finish within the shutdown timeout.
    pub async fn run(self, token: CancellationToken) -> Result<(), ServerError> {
        let stop = token.clone();
        #[expect(
            clippy::disallowed_methods,
            reason = "the one place that serves, with every limit applied"
        )]
        let serve =
            axum::serve(self.listener, self.router).with_graceful_shutdown(async move { stop.cancelled().await });
        let mut serve = pin!(serve.into_future());
        tokio::select! {
            served = &mut serve => served.map_err(ServerError::Serve),
            () = token.cancelled() => match tokio::time::timeout(self.shutdown_timeout, serve).await {
                Ok(served) => served.map_err(ServerError::Serve),
                Err(_elapsed) => Err(ServerError::Drain(self.shutdown_timeout)),
            },
        }
    }
}

#[cfg(test)]
mod tests;

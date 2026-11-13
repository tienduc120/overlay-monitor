//! Serves this service. Process-wide things happen here and nowhere else: configuration is read, the subscriber is
//! installed, and the root cancellation token is made and cancelled on a shutdown signal.

use std::pin::pin;
use std::process::ExitCode;

use example_api::{Config, Service};
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> ExitCode {
    #[expect(clippy::disallowed_methods, reason = "the composition root installs the subscriber")]
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    #[expect(clippy::disallowed_methods, reason = "the composition root reads configuration once")]
    let config = Config::from_env(|key| std::env::var(key).ok());
    #[expect(clippy::disallowed_methods, reason = "the composition root makes the root token")]
    let token = CancellationToken::new();

    let service = match Service::start(&config).await {
        Ok(service) => service,
        Err(e) => {
            tracing::error!(error = ?e, "start");
            return ExitCode::FAILURE;
        }
    };
    tracing::info!(addr = ?service.api_addr().ok(), "listening");
    let mut running = pin!(service.run(&token));
    let stopped = tokio::select! {
        stopped = &mut running => stopped,
        () = shutdown_signal() => {
            token.cancel();
            running.await
        }
    };
    match stopped {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!(error = ?e, "stopped");
            ExitCode::FAILURE
        }
    }
}

/// Ctrl-C, and on Unix the SIGTERM Kubernetes sends before it kills a pod.
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        match signal(SignalKind::terminate()) {
            Ok(mut term) => tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = term.recv() => {}
            },
            Err(_) => ctrl_c_or_never().await,
        }
    }
    #[cfg(not(unix))]
    ctrl_c_or_never().await;
}

/// Without signal support the service runs until it fails: stopping at once would be worse.
async fn ctrl_c_or_never() {
    if tokio::signal::ctrl_c().await.is_err() {
        std::future::pending::<()>().await;
    }
}

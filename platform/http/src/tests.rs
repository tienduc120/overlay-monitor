use std::net::SocketAddr;
use std::time::Duration;

use axum::Router;
use axum::routing::{get, post};
use platform_task::Tasks;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpStream;
use tokio_util::sync::CancellationToken;

use super::{Limits, Server, ServerError};

fn root() -> CancellationToken {
    #[expect(clippy::disallowed_methods, reason = "a test is its own composition root")]
    CancellationToken::new()
}

fn limits() -> Limits {
    Limits {
        request_timeout: Duration::from_millis(200),
        shutdown_timeout: Duration::from_secs(5),
        max_body_bytes: 8,
    }
}

fn router() -> Router {
    Router::new()
        .route("/", get(|| async { "ok" }))
        .route("/echo", post(|body: String| async move { body }))
        .route("/slow", get(std::future::pending::<&'static str>))
        .route("/boom", get(boom))
}

async fn boom() -> &'static str {
    panic!("handler bug")
}

/// Sends one request over a raw connection and returns the status code.
async fn status(addr: SocketAddr, method: &str, path: &str, body: &str) -> u16 {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    response.split_whitespace().nth(1).unwrap().parse().unwrap()
}

/// Starts the server on port 0 and returns its address, the token that stops it, and the task set to wait on.
async fn start() -> (SocketAddr, CancellationToken, Tasks) {
    let server = Server::bind("127.0.0.1:0", router(), limits()).await.unwrap();
    let addr = server.local_addr().unwrap();
    let stop = root();
    let mut tasks = Tasks::new(&stop, Duration::from_secs(5));
    tasks.spawn("server", move |token| async move {
        server.run(token).await.map_err(Into::into)
    });
    (addr, stop, tasks)
}

#[tokio::test]
async fn a_zero_limit_is_refused_because_it_would_mean_none() {
    let zero = Limits {
        request_timeout: Duration::ZERO,
        ..limits()
    };

    let err = Server::bind("127.0.0.1:0", router(), zero).await.err().unwrap();

    assert!(matches!(err, ServerError::ZeroLimit("request_timeout")), "{err}");
}

#[tokio::test]
async fn it_serves_until_cancelled_and_then_stops_cleanly() {
    let (addr, stop, tasks) = start().await;

    assert_eq!(status(addr, "GET", "/", "").await, 200);

    stop.cancel();
    tasks.wait().await.unwrap();
}

#[tokio::test]
async fn a_handler_slower_than_the_request_timeout_gets_503() {
    let (addr, stop, tasks) = start().await;

    assert_eq!(status(addr, "GET", "/slow", "").await, 503);

    stop.cancel();
    tasks.wait().await.unwrap();
}

#[tokio::test]
async fn a_body_over_the_limit_gets_413() {
    let (addr, stop, tasks) = start().await;

    assert_eq!(
        status(addr, "POST", "/echo", "12345678").await,
        200,
        "at the limit is fine"
    );
    assert_eq!(status(addr, "POST", "/echo", "123456789").await, 413);

    stop.cancel();
    tasks.wait().await.unwrap();
}

#[tokio::test]
async fn a_panicking_handler_gets_500_and_the_server_keeps_serving() {
    let (addr, stop, tasks) = start().await;

    assert_eq!(status(addr, "GET", "/boom", "").await, 500);
    assert_eq!(status(addr, "GET", "/", "").await, 200);

    stop.cancel();
    tasks.wait().await.unwrap();
}

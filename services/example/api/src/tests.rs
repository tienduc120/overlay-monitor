use std::net::SocketAddr;

use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpStream;
use tokio_util::sync::CancellationToken;

use super::{Config, Service};

async fn post(addr: SocketAddr, body: &str) -> u16 {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!(
        "POST /notes HTTP/1.1\r\nHost: test\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
         Connection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    response.split_whitespace().nth(1).unwrap().parse().unwrap()
}

#[test]
fn configuration_defaults_to_a_public_port_and_reads_addr() {
    assert_eq!(Config::from_env(|_| None).addr, "0.0.0.0:8080");
    assert_eq!(
        Config::from_env(|k| (k == "ADDR").then(|| "127.0.0.1:9000".to_owned())).addr,
        "127.0.0.1:9000"
    );
}

#[tokio::test]
async fn the_assembled_service_serves_its_use_case_and_stops_when_cancelled() {
    let service = Service::start(&Config {
        addr: "127.0.0.1:0".to_owned(),
    })
    .await
    .unwrap();
    let addr = service.api_addr().unwrap();
    #[expect(clippy::disallowed_methods, reason = "a test is its own composition root")]
    let stop = CancellationToken::new();
    let running = tokio::join!(service.run(&stop), async {
        let body = r#"{"id":"n-1","name":"groceries"}"#;
        let statuses = (post(addr, body).await, post(addr, body).await);
        stop.cancel();
        statuses
    });

    let (stopped, statuses) = running;
    assert_eq!(statuses, (201, 409), "created, then the same id is a conflict");
    stopped.unwrap();
}

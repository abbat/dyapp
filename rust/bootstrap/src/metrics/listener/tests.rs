use super::*;
use crate::metrics::tests::service;

async fn start() -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = bind("127.0.0.1:0".parse().unwrap()).await.unwrap();
    let address = listener.local_addr().unwrap();
    (address, tokio::spawn(serve(listener, Arc::new(service()))))
}

async fn ask(address: SocketAddr, request: &[u8]) -> String {
    let mut stream = TcpStream::connect(address).await.unwrap();
    stream.write_all(request).await.unwrap();
    let mut reply = String::new();
    stream.read_to_string(&mut reply).await.unwrap();
    reply
}

#[tokio::test]
async fn a_scrape_gets_the_exposition() {
    let (address, _task) = start().await;
    let reply = ask(address, b"GET /metrics HTTP/1.1\r\nHost: localhost\r\n\r\n").await;
    let (head, body) = reply.split_once("\r\n\r\n").unwrap();
    assert!(head.starts_with("HTTP/1.1 200 OK\r\n"), "{head}");
    assert!(head.contains(&format!("\r\nContent-Type: {CONTENT_TYPE}\r\n")));
    assert!(head.contains(&format!("\r\nContent-Length: {}\r\n", body.len())));
    assert!(body.contains("\ndyapp_process_start_time_seconds "));
}

#[tokio::test]
async fn nothing_but_get_metrics_is_served() {
    let (address, _task) = start().await;
    for (request, status) in [
        (&b"GET / HTTP/1.1\r\n\r\n"[..], "404 Not Found"),
        (b"GET /metrics?x=1 HTTP/1.1\r\n\r\n", "404 Not Found"),
        (b"GET /metrics/ HTTP/1.0\r\n\r\n", "404 Not Found"),
        (b"POST /metrics HTTP/1.1\r\n\r\n", "405 Method Not Allowed"),
        (b"HEAD /metrics HTTP/1.1\r\n\r\n", "405 Method Not Allowed"),
        (b"GET /metrics HTTP/2\r\n\r\n", "400 Bad Request"),
        (b"GET  /metrics HTTP/1.1\r\n\r\n", "400 Bad Request"),
        (b"\r\n\r\n", "400 Bad Request"),
    ] {
        let reply = ask(address, request).await;
        assert!(
            reply.starts_with(&format!("HTTP/1.1 {status}\r\n")),
            "{reply}"
        );
        assert!(reply.contains("\r\nContent-Length: 0\r\n") && reply.ends_with("\r\n\r\n"));
        assert!(!reply.contains("dyapp_"));
    }
    let reply = ask(address, b"PUT /metrics HTTP/1.1\r\n\r\n").await;
    assert!(reply.contains("\r\nAllow: GET\r\n"));
}

#[tokio::test]
async fn an_oversized_head_is_refused() {
    let (address, _task) = start().await;
    // Exactly the cap without an end: the server reads it all, so the reply is not reset.
    let reply = ask(address, &[b'a'; HEAD_BYTES]).await;
    assert!(reply.starts_with("HTTP/1.1 431 "), "{reply}");
}

#[tokio::test]
async fn slow_clients_are_dropped_and_free_their_slots() {
    let (address, _task) = start().await;
    let mut idle = Vec::new();
    for _ in 0..CONNECTIONS {
        let mut stream = TcpStream::connect(address).await.unwrap();
        stream.write_all(b"GET /met").await.unwrap();
        idle.push(stream);
    }
    // Served once the deadline closes the idle connections.
    let scrape = ask(address, b"GET /metrics HTTP/1.1\r\n\r\n");
    let reply = tokio::time::timeout(10 * DEADLINE, scrape).await.unwrap();
    assert!(reply.starts_with("HTTP/1.1 200 OK\r\n"));
    for mut stream in idle {
        let mut rest = Vec::new();
        let closed = tokio::time::timeout(10 * DEADLINE, stream.read_to_end(&mut rest)).await;
        assert!(matches!(closed, Ok(Ok(0)) | Ok(Err(_))), "{closed:?}");
    }
}

#[tokio::test]
async fn a_taken_address_fails_and_a_stopped_listener_closes_the_port() {
    let (address, task) = start().await;
    let error = bind(address).await.unwrap_err().to_string();
    assert!(error.starts_with("metrics.listen 127.0.0.1:"), "{error}");
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(TcpStream::connect(address).await.is_err());
}

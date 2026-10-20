//! The loopback `GET /metrics` listener: one request per connection, nothing else served.

use super::render;
use crate::service::Service;
use std::net::SocketAddr;
use std::sync::Arc;
use std::task::Poll;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Semaphore;

/// Connections served at once; others wait in the accept queue.
const CONNECTIONS: usize = 4;
/// Largest request head; a longer one gets 431.
const HEAD_BYTES: usize = 8192;
/// Time a client has to send its request and take the reply.
const DEADLINE: Duration = Duration::from_millis(if cfg!(test) { 300 } else { 5000 });
const CONTENT_TYPE: &str = "text/plain; version=0.0.4; charset=utf-8";

/// Opens `addresses`. An address that cannot be opened, such as `[::1]` on a host without IPv6,
/// is skipped; the node does not start when one is taken by another process or none opens.
pub async fn bind(addresses: &[SocketAddr]) -> anyhow::Result<Vec<TcpListener>> {
    let mut listeners = Vec::new();
    for &address in addresses {
        match TcpListener::bind(address).await {
            Ok(listener) => listeners.push(listener),
            Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
                anyhow::bail!("metrics.port {address}: {error}")
            }
            Err(error) => tracing::warn!(%address, %error, "metrics not listening"),
        }
    }
    anyhow::ensure!(!listeners.is_empty(), "metrics.port: no address opened");
    Ok(listeners)
}

/// Answers scrapes on every listener until the task is dropped.
pub async fn serve(listeners: Vec<TcpListener>, service: Arc<Service>) {
    let slots = Arc::new(Semaphore::new(CONNECTIONS));
    // Polled in this task, so the ports close when it is dropped.
    let accept = || {
        std::future::poll_fn(|cx| {
            listeners
                .iter()
                .find_map(|listener| match listener.poll_accept(cx) {
                    Poll::Ready(accepted) => Some(Poll::Ready(accepted)),
                    Poll::Pending => None,
                })
                .unwrap_or(Poll::Pending)
        })
    };
    loop {
        let Ok(slot) = slots.clone().acquire_owned().await else {
            return;
        };
        let stream = match accept().await {
            Ok((stream, _)) => stream,
            Err(error) => {
                // Out of descriptors, say: pause instead of spinning.
                tracing::warn!(%error, "metrics connection not accepted");
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        };
        let service = service.clone();
        tokio::spawn(async move {
            let _ = tokio::time::timeout(DEADLINE, answer(stream, &service)).await;
            drop(slot);
        });
    }
}

async fn answer(mut stream: TcpStream, service: &Service) -> std::io::Result<()> {
    let mut head = vec![0; HEAD_BYTES];
    let mut len = 0;
    let status = loop {
        if len == HEAD_BYTES {
            break "431 Request Header Fields Too Large";
        }
        let read = stream.read(&mut head[len..]).await?;
        if read == 0 {
            return Ok(());
        }
        len += read;
        if head[..len].windows(4).any(|w| w == b"\r\n\r\n") {
            break route(&head[..len]);
        }
    };
    let (body, extra) = match status {
        "200 OK" => (render(service), ""),
        "405 Method Not Allowed" => (String::new(), "Allow: GET\r\n"),
        _ => (String::new(), ""),
    };
    let reply = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {CONTENT_TYPE}\r\nContent-Length: {}\r\n\
         Connection: close\r\n{extra}\r\n{body}",
        body.len()
    );
    stream.write_all(reply.as_bytes()).await?;
    stream.shutdown().await
}

/// The status for a complete request head.
fn route(head: &[u8]) -> &'static str {
    let line = head.split(|&b| b == b'\r').next().unwrap_or_default();
    let mut parts = line.split(|&b| b == b' ');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(method), Some(path), Some(b"HTTP/1.0" | b"HTTP/1.1"), None) => match (method, path) {
            (b"GET", b"/metrics") => "200 OK",
            (_, b"/metrics") => "405 Method Not Allowed",
            _ => "404 Not Found",
        },
        _ => "400 Bad Request",
    }
}

#[cfg(test)]
mod tests;

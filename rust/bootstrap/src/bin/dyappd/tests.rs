use super::*;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

fn server(
    answer: impl FnOnce(&mut TcpStream) + Send + 'static,
) -> (std::net::SocketAddr, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let task = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 1024];
        assert!(stream.read(&mut request).unwrap() > 0);
        answer(&mut stream);
    });
    (address, task)
}

fn response(total_bytes: usize) -> Vec<u8> {
    let head = b"HTTP/1.1 200 OK\r\n\r\n";
    let mut response = head.to_vec();
    response.resize(total_bytes, b'm');
    response
}

#[test]
fn scrape_preserves_valid_metrics_body() {
    let (address, task) = server(|stream| {
        stream
            .write_all(b"HTTP/1.1 200 OK\r\n\r\ndyapp_process_start_time_seconds 10\n")
            .unwrap();
    });
    let clock = Clock::system();
    let body = scrape_at(&address, clock.instant() + SCRAPE_DEADLINE, &clock).unwrap();
    assert_eq!(total(&body, "process_start_time_seconds", ""), 10);
    task.join().unwrap();
}

#[test]
fn scrape_accepts_the_byte_boundary() {
    let (address, task) = server(|stream| {
        for chunk in response(MAX_SCRAPE_BYTES).chunks(1024) {
            stream.write_all(chunk).unwrap();
        }
    });
    let clock = Clock::system();
    let body = scrape_at(&address, clock.instant() + SCRAPE_DEADLINE, &clock).unwrap();
    assert_eq!(
        body.len(),
        MAX_SCRAPE_BYTES - b"HTTP/1.1 200 OK\r\n\r\n".len()
    );
    task.join().unwrap();
}

#[test]
fn scrape_rejects_oversized_chunked_response() {
    let (address, task) = server(|stream| {
        for chunk in response(MAX_SCRAPE_BYTES + 1).chunks(1024) {
            stream.write_all(chunk).unwrap();
        }
    });
    let clock = Clock::system();
    assert_eq!(
        scrape_at(&address, clock.instant() + SCRAPE_DEADLINE, &clock),
        None
    );
    task.join().unwrap();
}

#[test]
fn scrape_uses_one_overall_deadline() {
    let (address, task) = server(|stream| {
        stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n").unwrap();
        let mut closed = [0];
        while stream.read(&mut closed).unwrap() != 0 {}
    });
    let clock = Clock::system();
    let started = clock.instant();
    assert_eq!(
        scrape_at(&address, started + Duration::from_millis(100), &clock),
        None
    );
    assert!(started.elapsed() < Duration::from_secs(2));
    task.join().unwrap();
}

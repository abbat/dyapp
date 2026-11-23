use super::*;
use std::future::ready;

fn network(rows: &[&str]) -> Network {
    Network {
        seeds: rows.iter().map(|row| (*row).to_owned()).collect(),
        name: String::new(),
    }
}

#[tokio::test]
async fn first_tries_transport_fallback_then_stops_before_later_rows() {
    let network = network(&["invalid row", " 127.0.0.1:1 ", "127.0.0.1:2"]);
    let mut called = Vec::new();
    let (nodes, error) = select(&network, Selection::First, |address| {
        called.push(address.to_string());
        ready(if called.len() == 2 {
            Ok(7)
        } else {
            Err("tcp refused")
        })
    })
    .await;
    assert_eq!(nodes, [7]);
    assert_eq!(error, Some("tcp refused"));
    assert_eq!(
        called,
        ["/ip4/127.0.0.1/tcp/1", "/ip4/127.0.0.1/udp/1/quic-v1"]
    );
}

#[tokio::test]
async fn per_row_keeps_order_and_skips_the_alternative_after_success() {
    let network = network(&["127.0.0.1:1", "invalid row", "127.0.0.1:2", "127.0.0.1:3"]);
    let mut called = Vec::new();
    let (nodes, error) = select(&network, Selection::PerRow, |address| {
        let address = address.to_string();
        called.push(address.clone());
        ready(match address.as_str() {
            "/ip4/127.0.0.1/tcp/1" => Ok(1),
            "/ip4/127.0.0.1/udp/2/quic-v1" => Ok(2),
            _ => Err(address),
        })
    })
    .await;
    assert_eq!(nodes, [1, 2]);
    assert_eq!(error.as_deref(), Some("/ip4/127.0.0.1/udp/3/quic-v1"));
    assert_eq!(
        called,
        [
            "/ip4/127.0.0.1/tcp/1",
            "/ip4/127.0.0.1/tcp/2",
            "/ip4/127.0.0.1/udp/2/quic-v1",
            "/ip4/127.0.0.1/tcp/3",
            "/ip4/127.0.0.1/udp/3/quic-v1",
        ]
    );
}

#[tokio::test]
async fn complete_failure_retains_the_last_attempt_without_dialing_invalid_rows() {
    let network = network(&["invalid row", "127.0.0.1:1", "still invalid"]);
    let mut calls = 0;
    let (nodes, error) = select(&network, Selection::First, |_| {
        calls += 1;
        ready(Err::<(), _>(calls))
    })
    .await;
    assert!(nodes.is_empty());
    assert_eq!((calls, error), (2, Some(2)));
}

#[tokio::test]
async fn empty_invalid_and_bad_configuration_have_distinct_results() {
    for network in [network(&[]), network(&["invalid row"])] {
        assert!(matches!(
            first(&network).await,
            Err(ConnectError::NotConnected)
        ));
        assert!(reachable(&network).await.unwrap().is_empty());
    }
    let mut invalid = network(&[]);
    invalid.name = " invalid".to_owned();
    assert!(matches!(
        first(&invalid).await,
        Err(ConnectError::Configuration(_))
    ));
    assert!(matches!(
        reachable(&invalid).await,
        Err(ConnectError::Configuration(_))
    ));
}

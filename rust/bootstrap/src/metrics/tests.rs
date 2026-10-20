use super::*;
use crate::storage::BootstrapStore;
use crate::NodeConfig;

pub(super) fn service() -> Service {
    let dir = format!("/tmp/ai/test-metrics-{}", uuid::Uuid::new_v4());
    let mut config = NodeConfig::default();
    config.storage.dir = dir.clone().into();
    Service::new(BootstrapStore::new(&dir).unwrap(), config)
}

fn value(text: &str, sample: &str) -> Option<u64> {
    text.lines()
        .find_map(|line| line.strip_prefix(sample)?.strip_prefix(' ')?.parse().ok())
}

fn samples(text: &str) -> usize {
    text.lines().filter(|line| !line.starts_with('#')).count()
}

#[test]
fn a_fresh_render_is_well_formed_and_has_no_snapshot() {
    let text = render(&service());
    let mut types: Vec<&str> = text.lines().filter(|l| l.starts_with("# TYPE ")).collect();
    let families = types.len();
    types.dedup();
    assert_eq!(types.len(), families);
    for line in text.lines().filter(|line| !line.starts_with('#')) {
        let (name, value) = line.rsplit_once(' ').unwrap();
        assert!(name.starts_with("dyapp_"), "{line}");
        assert!(name.bytes().all(|b| b.is_ascii_graphic()), "{line}");
        value.parse::<u64>().unwrap();
        // Labels come from the fixed tables only: no peer, key or address.
        let labels = name
            .split_once('{')
            .map_or("", |(_, l)| l.trim_end_matches('}'));
        for label in labels.split(',').filter(|l| !l.is_empty()) {
            let value = label.split_once('=').unwrap().1.trim_matches('"');
            let known: [&[&str]; 8] = [
                &PROTOCOLS,
                &OUTCOMES,
                &GUARDS,
                &STORES,
                &PHASES,
                &WireBytes::NAMES,
                &["in", "out"],
                &["postpone", "overflow", "drop_refill", "stop_the_world"],
            ];
            assert!(known.iter().any(|t| t.contains(&value)), "{line}");
        }
    }
    assert_eq!(value(&text, "dyapp_snapshot_timestamp_seconds"), Some(0));
    assert!(!text.contains("dyapp_storage_read_ok{"));
    assert!(value(&text, "dyapp_process_start_time_seconds").unwrap() > 0);
}

#[test]
fn counters_survive_the_hourly_status_and_scrapes_repeat() {
    let service = service();
    service.metrics.count(2, None);
    service
        .metrics
        .count(2, Some(outcome(&InboundFailure::ResponseOmission)));
    service.wire.add(4, dyapp_p2p_net::Dir::Out, 100);
    assert!(service.busy(0));
    let first = render(&service);
    // The hourly status lines take deltas; the counters stay.
    service.report();
    service.pow.take_counts();
    service.wire.take();
    let second = render(&service);
    assert_eq!(first, second);
    assert_eq!(
        value(&second, "dyapp_requests_total{protocol=\"mailbox\"}"),
        Some(1)
    );
    let omitted = "dyapp_request_outcomes_total{protocol=\"mailbox\",outcome=\"omitted\"}";
    assert_eq!(value(&second, omitted), Some(1));
    let media = "dyapp_wire_bytes_total{protocol=\"media\",direction=\"out\"}";
    assert_eq!(value(&second, media), Some(100));
    let traffic = "dyapp_refused_total{guard=\"traffic_bytes_per_second\"}";
    assert_eq!(value(&second, traffic), Some(1));
}

#[test]
fn a_delayed_reply_counts_once_and_activity_adds_no_series() {
    let service = service();
    let idle = samples(&render(&service));
    service.metrics.count(3, None);
    // The reply comes much later, after other requests.
    service.metrics.count(0, None);
    service.metrics.count(0, Some(0));
    service.metrics.count(3, Some(0));
    let text = render(&service);
    assert_eq!(samples(&text), idle);
    let answered = "dyapp_request_outcomes_total{protocol=\"media\",outcome=\"answered\"}";
    assert_eq!(value(&text, answered), Some(1));
}

#[test]
fn a_snapshot_reports_the_configured_stores() {
    let service = service();
    snapshot(&service);
    let text = render(&service);
    assert!(value(&text, "dyapp_snapshot_timestamp_seconds").unwrap() > 0);
    for store in ["profiles", "messages"] {
        let ok = format!("dyapp_storage_read_ok{{store=\"{store}\"}}");
        assert_eq!(value(&text, &ok), Some(1));
        let free = format!("dyapp_storage_free_bytes{{store=\"{store}\"}}");
        assert!(value(&text, &free).unwrap() > 0);
    }
    assert!(!text.contains("store=\"media\""));
}

#[test]
fn a_failed_read_reports_no_size() {
    let service = service();
    *service.metrics.snapshot.lock().unwrap() = Snapshot {
        at: 1,
        stores: [Usage::Failed, Usage::Read(5, 6), Usage::Off],
    };
    let text = render(&service);
    assert_eq!(
        value(&text, "dyapp_storage_read_ok{store=\"profiles\"}"),
        Some(0)
    );
    assert!(!text.contains("dyapp_storage_used_bytes{store=\"profiles\"}"));
    assert_eq!(
        value(&text, "dyapp_storage_used_bytes{store=\"messages\"}"),
        Some(5)
    );
}

#[test]
fn inbound_failures_map_to_outcomes() {
    let io = InboundFailure::Io(std::io::Error::other("reset"));
    assert_eq!(OUTCOMES[outcome(&io)], "io");
    assert_eq!(OUTCOMES[outcome(&InboundFailure::Timeout)], "timeout");
    assert_eq!(
        OUTCOMES[outcome(&InboundFailure::ConnectionClosed)],
        "closed"
    );
    assert_eq!(
        OUTCOMES[outcome(&InboundFailure::UnsupportedProtocols)],
        "unsupported"
    );
}

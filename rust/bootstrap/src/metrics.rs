//! Process-lifetime counters and periodic gauges, rendered in the Prometheus text format.
//!
//! Counters only grow while the process runs; the hourly status lines take their own deltas.
//! Label values come from fixed tables, never from peers, keys, hashes or addresses.

pub mod listener;

use crate::service::Service;
use dyapp_p2p_net::{Behaviour, BehaviourEvent, WireBytes};
use libp2p::request_response::{self, InboundFailure, Message};
use libp2p::swarm::{Swarm, SwarmEvent};
use std::fmt::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Served request-response protocols, in label order.
const PROTOCOLS: [&str; 6] = ["node", "profile", "mailbox", "media", "search", "inventory"];
/// How a served request ended: its response sent, or the inbound failure libp2p reports.
const OUTCOMES: [&str; 6] = [
    "answered",
    "timeout",
    "closed",
    "unsupported",
    "omitted",
    "io",
];
/// Service guards, in `Service` guard order.
const GUARDS: [&str; 6] = [
    "disk_profiles",
    "disk_messages",
    "disk_media",
    "memory_rate_limited",
    "memory_dropped",
    "traffic_bytes_per_second",
];
/// Stores with size gauges.
const STORES: [&str; 3] = ["profiles", "messages", "media"];
/// Search index phases, as in the status line.
const PHASES: [&str; 3] = ["refilling", "rebuilding", "serving"];

/// How often [`spawn_snapshots`] reads the store sizes.
pub const SNAPSHOT_EVERY: Duration = Duration::from_secs(300);

/// A store's last size read.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
enum Usage {
    /// The store is not configured.
    #[default]
    Off,
    /// The read failed; no size is reported rather than a healthy-looking zero.
    Failed,
    /// Bytes used by the store and free on its file system.
    Read(u64, u64),
}

/// The gauges read off the swarm loop.
#[derive(Clone, Copy, Default)]
struct Snapshot {
    /// Unix seconds of the read; 0 before the first.
    at: u64,
    stores: [Usage; 3],
}

/// Counters and gauges shared by the swarm loop, the store workers and the exposition.
pub struct Metrics {
    /// Unix seconds of the process start: a scraper sees counter resets by it.
    started: u64,
    /// Requests received per protocol.
    requests: [AtomicU64; PROTOCOLS.len()],
    /// Requests ended per protocol and outcome.
    outcomes: [[AtomicU64; OUTCOMES.len()]; PROTOCOLS.len()],
    connected_peers: AtomicU64,
    connections: AtomicU64,
    snapshot: Mutex<Snapshot>,
}

impl Default for Metrics {
    fn default() -> Self {
        Self {
            started: unix_now(),
            requests: Default::default(),
            outcomes: Default::default(),
            connected_peers: AtomicU64::new(0),
            connections: AtomicU64::new(0),
            snapshot: Mutex::default(),
        }
    }
}

impl Metrics {
    /// Counts a served request once when it arrives and once when it ends, so a reply sent long
    /// after the request counts once; updates the connection gauges on connection events.
    pub fn observe(&self, swarm: &Swarm<Behaviour>, event: &SwarmEvent<BehaviourEvent>) {
        match event {
            SwarmEvent::Behaviour(BehaviourEvent::Node(event)) => self.served(0, event),
            SwarmEvent::Behaviour(BehaviourEvent::Profile(event)) => self.served(1, event),
            SwarmEvent::Behaviour(BehaviourEvent::Mailbox(event)) => self.served(2, event),
            SwarmEvent::Behaviour(BehaviourEvent::Media(event)) => self.served(3, event),
            SwarmEvent::Behaviour(BehaviourEvent::Search(event)) => self.served(4, event),
            SwarmEvent::Behaviour(BehaviourEvent::Inventory(event)) => self.served(5, event),
            SwarmEvent::ConnectionEstablished { .. } | SwarmEvent::ConnectionClosed { .. } => {
                let counters = swarm.network_info().connection_counters().num_established();
                let peers = swarm.connected_peers().count() as u64;
                self.connected_peers.store(peers, Ordering::Relaxed);
                self.connections.store(counters.into(), Ordering::Relaxed);
            }
            _ => {}
        }
    }

    fn served<Req, Resp>(&self, protocol: usize, event: &request_response::Event<Req, Resp>) {
        match event {
            request_response::Event::Message {
                message: Message::Request { .. },
                ..
            } => self.count(protocol, None),
            request_response::Event::ResponseSent { .. } => self.count(protocol, Some(0)),
            request_response::Event::InboundFailure { error, .. } => {
                self.count(protocol, Some(outcome(error)));
            }
            _ => {}
        }
    }

    /// Counts a received request (`None`) or an outcome index.
    fn count(&self, protocol: usize, outcome: Option<usize>) {
        let counter = match outcome {
            None => &self.requests[protocol],
            Some(outcome) => &self.outcomes[protocol][outcome],
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }
}

/// The outcome index of an inbound failure. A dropped reply (a refused, failed or overloaded
/// request) is `omitted`.
fn outcome(error: &InboundFailure) -> usize {
    match error {
        InboundFailure::Timeout => 1,
        InboundFailure::ConnectionClosed => 2,
        InboundFailure::UnsupportedProtocols => 3,
        InboundFailure::ResponseOmission => 4,
        InboundFailure::Io(_) => 5,
    }
}

/// Reads the store sizes now and every [`SNAPSHOT_EVERY`] on the blocking pool, off the swarm
/// loop; stops with the runtime.
pub fn spawn_snapshots(service: &Arc<Service>) {
    let service = service.clone();
    tokio::spawn(async move {
        let mut every = tokio::time::interval(SNAPSHOT_EVERY);
        loop {
            every.tick().await;
            let service = service.clone();
            let _ = tokio::task::spawn_blocking(move || snapshot(&service)).await;
        }
    });
}

/// Reads the store sizes: blocking SQLite reads.
pub fn snapshot(service: &Service) {
    let read = |usage: crate::Result<(u64, u64)>| {
        usage.map_or(Usage::Failed, |(used, free)| Usage::Read(used, free))
    };
    let stores = [
        read(service.store.profiles_usage()),
        read(service.store.messages_usage()),
        service
            .media
            .as_ref()
            .map_or(Usage::Off, |m| read(m.usage())),
    ];
    if let Ok(mut snapshot) = service.metrics.snapshot.lock() {
        *snapshot = Snapshot {
            at: unix_now(),
            stores,
        };
    }
}

/// The exposition. Reads atomics, the last snapshot and the search index state only, never a
/// store or the swarm, and changes nothing, so scrapes may repeat.
pub fn render(service: &Service) -> String {
    let metrics = &service.metrics;
    let mut out = Text(String::with_capacity(8192));
    out.family(
        "process_start_time_seconds",
        "gauge",
        "Unix time the process started.",
    );
    out.sample("process_start_time_seconds", "", metrics.started);

    out.family(
        "requests_total",
        "counter",
        "Requests received per protocol.",
    );
    for (protocol, count) in PROTOCOLS.iter().zip(&metrics.requests) {
        out.sample(
            "requests_total",
            &format!("{{protocol=\"{protocol}\"}}"),
            load(count),
        );
    }
    out.family(
        "request_outcomes_total",
        "counter",
        "Requests ended per protocol: answered, or the failure that ended them.",
    );
    for (protocol, outcomes) in PROTOCOLS.iter().zip(&metrics.outcomes) {
        for (outcome, count) in OUTCOMES.iter().zip(outcomes) {
            let labels = format!("{{protocol=\"{protocol}\",outcome=\"{outcome}\"}}");
            out.sample("request_outcomes_total", &labels, load(count));
        }
    }
    out.family(
        "wire_bytes_total",
        "counter",
        "Payload bytes per protocol and direction; media out is media served.",
    );
    for (protocol, bytes) in WireBytes::NAMES.iter().zip(service.wire.totals().0) {
        for (direction, bytes) in ["in", "out"].iter().zip(bytes) {
            let labels = format!("{{protocol=\"{protocol}\",direction=\"{direction}\"}}");
            out.sample("wire_bytes_total", &labels, bytes);
        }
    }
    let (checks, forged) = service.pow.totals();
    out.family("pow_checks_total", "counter", "Proof-of-work checks run.");
    out.sample("pow_checks_total", "", checks);
    out.family(
        "pow_forged_total",
        "counter",
        "Proof-of-work claims found forged.",
    );
    out.sample("pow_forged_total", "", forged);
    out.family("refused_total", "counter", "Requests refused per guard.");
    for (guard, count) in GUARDS.iter().zip(service.refusals()) {
        out.sample("refused_total", &format!("{{guard=\"{guard}\"}}"), count);
    }

    out.family("connected_peers", "gauge", "Peers with an open connection.");
    out.sample("connected_peers", "", load(&metrics.connected_peers));
    out.family("connections", "gauge", "Established connections.");
    out.sample("connections", "", load(&metrics.connections));

    if let Some(index) = &service.search {
        let report = index.rebuild_report();
        out.family(
            "search_index_version",
            "gauge",
            "Active search index generation.",
        );
        out.sample("search_index_version", "", index.index_version().into());
        out.family("search_phase", "gauge", "1 for the search index phase.");
        for phase in PHASES {
            let labels = format!("{{phase=\"{phase}\"}}");
            out.sample("search_phase", &labels, u64::from(report.phase == phase));
        }
        out.family(
            "search_rebuild_events_total",
            "counter",
            "Search index rebuild aborts and forced completions.",
        );
        for (event, count) in [
            ("postpone", report.postpones),
            ("overflow", report.overflows),
            ("drop_refill", report.drop_refills),
            ("stop_the_world", report.stop_the_worlds),
        ] {
            let labels = format!("{{event=\"{event}\"}}");
            out.sample("search_rebuild_events_total", &labels, count);
        }
    }

    let snapshot = metrics.snapshot.lock().map(|s| *s).unwrap_or_default();
    out.family(
        "snapshot_timestamp_seconds",
        "gauge",
        "Unix time the storage gauges were read; 0 before the first read.",
    );
    out.sample("snapshot_timestamp_seconds", "", snapshot.at);
    out.family(
        "storage_read_ok",
        "gauge",
        "1 if the last size read of a store succeeded.",
    );
    for (store, usage) in STORES.iter().zip(snapshot.stores) {
        if snapshot.at > 0 && usage != Usage::Off {
            let ok = u64::from(usage != Usage::Failed);
            out.sample("storage_read_ok", &format!("{{store=\"{store}\"}}"), ok);
        }
    }
    for (name, help, pick) in [
        ("storage_used_bytes", "Bytes a store uses.", 0),
        (
            "storage_free_bytes",
            "Bytes free on a store's file system.",
            1,
        ),
    ] {
        out.family(name, "gauge", help);
        for (store, usage) in STORES.iter().zip(snapshot.stores) {
            if let Usage::Read(used, free) = usage {
                let value = [used, free][pick];
                out.sample(name, &format!("{{store=\"{store}\"}}"), value);
            }
        }
    }
    out.0
}

/// Prometheus text format 0.0.4, every name prefixed `dyapp_`.
struct Text(String);

impl Text {
    fn family(&mut self, name: &str, kind: &str, help: &str) {
        let _ = writeln!(
            self.0,
            "# HELP dyapp_{name} {help}\n# TYPE dyapp_{name} {kind}"
        );
    }

    fn sample(&mut self, name: &str, labels: &str, value: u64) {
        let _ = writeln!(self.0, "dyapp_{name}{labels} {value}");
    }
}

fn load(counter: &AtomicU64) -> u64 {
    counter.load(Ordering::Relaxed)
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests;

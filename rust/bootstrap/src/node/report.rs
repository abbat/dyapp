//! The hourly node status and state log lines.

use super::{Replicas, Watchers};
use crate::service::Service;
use dyapp_p2p_net::Behaviour;
use libp2p::Swarm;

/// The hourly status line: no peer or key identifiers, counts and sizes only.
#[expect(
    clippy::too_many_arguments,
    reason = "one argument per counter the swarm loop owns"
)]
pub(super) fn status(
    service: &Service,
    swarm: &Swarm<Behaviour>,
    known: usize,
    answered: u64,
    failed: u64,
    foreign_peers: u64,
    media_replica_holders: usize,
    replica_dropped: u64,
) {
    let bytes = |usage: crate::Result<(u64, u64)>| usage.map_or(0, |(used, _)| used);
    let (pow_checks, pow_forged) = service.pow.take_counts();
    let wire_bytes = service.wire.take();
    let rebuild = service.search.as_ref().map(|s| s.rebuild_report());
    tracing::info!(
        connected = swarm.connected_peers().count(),
        known,
        answered,
        failed,
        foreign_peers,
        profiles_bytes = bytes(service.store.profiles_usage()),
        messages_bytes = bytes(service.store.messages_usage()),
        media_bytes = service.media.as_ref().map_or(0, |m| bytes(m.usage())),
        media_replica_holders,
        replica_dropped,
        throttled_dials = swarm
            .behaviour()
            .pow_gate
            .as_ref()
            .map_or(0, |gate| gate.guard().take_denied()),
        denied_dials = service.dial_filter.take_denied(),
        pow_checks,
        pow_forged,
        newcomer_evictions = service.search.as_ref().map_or(0, |s| s.take_newcomer_evictions()),
        search_phase = rebuild.map_or("off", |r| r.phase),
        search_index_version = service.search.as_ref().map_or(0, |s| s.index_version()),
        rebuild_postpones = rebuild.map_or(0, |r| r.postpones),
        rebuild_overflows = rebuild.map_or(0, |r| r.overflows),
        rebuild_drop_refills = rebuild.map_or(0, |r| r.drop_refills),
        rebuild_stop_the_worlds = rebuild.map_or(0, |r| r.stop_the_worlds),
        wire_bytes = %wire_bytes,
        "node status"
    );
}

/// The in-memory state line, hourly and on SIGHUP: requests still pending, the largest per-connection
/// map against the open connections, and mailbox watches. Growth that idling does not undo is a leak.
pub(super) fn state(
    replicas: &Replicas,
    swarm: &Swarm<Behaviour>,
    per_connection: &[usize],
    watchers: &Watchers,
) {
    tracing::info!(
        pending = replicas.pending(),
        connection_entries = per_connection.iter().max().copied().unwrap_or(0),
        connections = swarm.network_info().connection_counters().num_connections(),
        watches = watchers.values().map(Vec::len).sum::<usize>(),
        trusted_peers = replicas.trust.score.len().max(replicas.trust.seen.len()),
        incoming_group_connections = swarm
            .behaviour()
            .incoming_groups
            .as_ref()
            .map_or(0, dyapp_p2p_net::IncomingGroupGuard::active_connections),
        incoming_groups = swarm
            .behaviour()
            .incoming_groups
            .as_ref()
            .map_or(0, dyapp_p2p_net::IncomingGroupGuard::active_groups),
        "node state"
    );
}

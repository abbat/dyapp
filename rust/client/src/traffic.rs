use super::WIRE;

/// `[in, out]` payload bytes of every client swarm since the first one started this process;
/// `None` before that. Excludes transport, encryption and gossip control traffic.
#[must_use]
pub fn traffic() -> Option<[u64; 2]> {
    let slots = WIRE.get()?.totals().0;
    let add = |a: u64, b: u64| a.saturating_add(b);
    let totals = slots
        .into_iter()
        .fold([0, 0], |[i, o], [a, b]| [add(i, a), add(o, b)]);
    Some(totals)
}

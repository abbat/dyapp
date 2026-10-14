//! Shared outbound dial policy; transport and swarm integration are separate steps.

use libp2p::multiaddr::Protocol;
use libp2p::Multiaddr;
use std::collections::{HashMap, VecDeque};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

const MAX_ACTIVE: u8 = 4;
const MAX_FAILURES: usize = 10;
const FAILURE_WINDOW: Duration = Duration::from_secs(60);

#[derive(Default)]
struct Group {
    active: u8,
    failures: VecDeque<Instant>,
}

struct Shared {
    enabled: bool,
    groups: Mutex<HashMap<IpAddr, Group>>,
    denied: AtomicU64,
}

impl Shared {
    fn groups(&self) -> MutexGuard<'_, HashMap<IpAddr, Group>> {
        self.groups
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }
}

/// Outbound IP policy, /24 or /48 attempt slots and sliding failure history, shared by clones.
/// Disabled mode bypasses IP/group checks; the separate peer-proof gate can still count refusals.
#[derive(Clone)]
pub struct DialGuard {
    shared: Arc<Shared>,
}

/// One admitted dial attempt. Dropping it releases its slot without recording a failure.
/// A slot is deliberately not cloneable.
pub struct DialSlot {
    shared: Arc<Shared>,
    group: Option<IpAddr>,
}

impl DialGuard {
    pub fn new(enabled: bool) -> Self {
        Self {
            shared: Arc::new(Shared {
                enabled,
                groups: Mutex::default(),
                denied: AtomicU64::new(0),
            }),
        }
    }

    /// Admits the first IP in an address; DNS-only/opaque addresses pass without a group slot.
    /// Runtime transports must place this below DNS to inspect the actual resolved destination.
    pub fn admit(&self, address: &Multiaddr, now: Instant) -> Option<DialSlot> {
        let slot = |group| DialSlot {
            shared: self.shared.clone(),
            group,
        };
        if !self.shared.enabled {
            return Some(slot(None));
        }
        let mut groups = self.shared.groups();
        prune(&mut groups, now);
        let Some(ip) = address.iter().find_map(|protocol| match protocol {
            Protocol::Ip4(ip) => Some(IpAddr::V4(ip)),
            Protocol::Ip6(ip) => Some(ip.to_canonical()),
            _ => None,
        }) else {
            return Some(slot(None));
        };
        if forbidden(ip) {
            self.refuse();
            return None;
        }
        let key = group(ip);
        let state = groups.entry(key).or_default();
        if state.active >= MAX_ACTIVE || state.failures.len() >= MAX_FAILURES {
            self.refuse();
            return None;
        }
        state.active += 1;
        Some(slot(Some(key)))
    }

    /// Removes expired histories globally, retaining groups with a live attempt.
    pub fn cleanup(&self, now: Instant) {
        prune(&mut self.shared.groups(), now);
    }

    pub(crate) fn refuse(&self) {
        self.shared.denied.fetch_add(1, Ordering::Relaxed);
    }

    /// Refusals since the preceding take, shared with clones and the future peer-proof gate.
    pub fn take_denied(&self) -> u64 {
        self.shared.denied.swap(0, Ordering::Relaxed)
    }
}

impl DialSlot {
    /// Records an actual transport error and releases the slot once. Cancellation just drops it.
    pub fn fail(self, now: Instant) {
        if let Some(key) = self.group {
            let mut groups = self.shared.groups();
            prune(&mut groups, now);
            groups.entry(key).or_default().failures.push_back(now);
        }
    }
}

impl Drop for DialSlot {
    fn drop(&mut self) {
        let Some(key) = self.group else { return };
        let mut groups = self.shared.groups();
        if let Some(state) = groups.get_mut(&key) {
            state.active = state.active.saturating_sub(1);
            if state.active == 0 && state.failures.is_empty() {
                groups.remove(&key);
            }
        }
    }
}

fn prune(groups: &mut HashMap<IpAddr, Group>, now: Instant) {
    groups.retain(|_, state| {
        // Completions on different threads can enter the lock out of timestamp order.
        state.failures.retain(|time| {
            now.checked_duration_since(*time)
                .is_none_or(|age| age < FAILURE_WINDOW)
        });
        state.active > 0 || !state.failures.is_empty()
    });
}

fn group(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, c, _] = ip.octets();
            IpAddr::V4(Ipv4Addr::new(a, b, c, 0))
        }
        IpAddr::V6(ip) => {
            let [a, b, c, ..] = ip.segments();
            IpAddr::V6(Ipv6Addr::new(a, b, c, 0, 0, 0, 0, 0))
        }
    }
}

/// The specified deny ranges, not a complete IANA global-unicast classifier.
fn forbidden(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, c, _] = ip.octets();
            a == 0
                || a == 10
                || a == 127
                || a >= 224
                || (a == 100 && (64..=127).contains(&b))
                || (a == 169 && b == 254)
                || (a == 172 && (16..=31).contains(&b))
                || (a == 192 && (b == 168 || (b == 0 && c == 0)))
                || (a == 198 && (18..=19).contains(&b))
        }
        IpAddr::V6(ip) => {
            let parts = ip.segments();
            ip.is_unspecified()
                || ip.is_loopback()
                || ip.is_multicast()
                || parts[0] & 0xfe00 == 0xfc00
                || parts[0] & 0xffc0 == 0xfe80
                || parts[..6] == [0x64, 0xff9b, 0, 0, 0, 0]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn address(ip: &str) -> Multiaddr {
        let ip: IpAddr = ip.parse().unwrap();
        format!("/ip{}/{ip}/tcp/1", if ip.is_ipv4() { 4 } else { 6 })
            .parse()
            .unwrap()
    }

    #[test]
    fn specified_address_ranges_and_boundaries() {
        let guard = DialGuard::new(true);
        let now = Instant::now();
        let mut denied = 0;
        for (ip, allowed) in [
            ("0.0.0.0", false),
            ("0.255.255.255", false),
            ("1.0.0.0", true),
            ("9.255.255.255", true),
            ("10.0.0.0", false),
            ("10.255.255.255", false),
            ("11.0.0.0", true),
            ("100.63.255.255", true),
            ("100.64.0.0", false),
            ("100.127.255.255", false),
            ("100.128.0.0", true),
            ("126.255.255.255", true),
            ("127.0.0.0", false),
            ("127.255.255.255", false),
            ("128.0.0.0", true),
            ("169.253.255.255", true),
            ("169.254.0.0", false),
            ("169.254.255.255", false),
            ("169.255.0.0", true),
            ("172.15.255.255", true),
            ("172.16.0.0", false),
            ("172.31.255.255", false),
            ("172.32.0.0", true),
            ("192.0.0.0", false),
            ("192.0.0.255", false),
            ("192.0.1.0", true),
            ("192.168.0.0", false),
            ("192.168.255.255", false),
            ("192.169.0.0", true),
            ("198.17.255.255", true),
            ("198.18.0.0", false),
            ("198.19.255.255", false),
            ("198.20.0.0", true),
            ("223.255.255.255", true),
            ("224.0.0.0", false),
            ("255.255.255.255", false),
            ("::", false),
            ("::1", false),
            ("::2", true),
            ("fbff::", true),
            ("fc00::", false),
            ("fdff::", false),
            ("fe00::", true),
            ("fe7f::", true),
            ("fe80::", false),
            ("febf::", false),
            ("fec0::", true),
            ("feff::", true),
            ("ff00::", false),
            ("ffff::", false),
            ("64:ff9a::", true),
            ("64:ff9b::", false),
            ("64:ff9b::ffff:ffff", false),
            ("64:ff9b:0:0:0:1::", true),
            ("64:ff9c::", true),
            ("::ffff:10.1.2.3", false),
            ("::ffff:9.1.2.3", true),
        ] {
            assert_eq!(guard.admit(&address(ip), now).is_some(), allowed, "{ip}");
            denied += u64::from(!allowed);
        }
        assert_eq!(guard.take_denied(), denied);
        assert_eq!(guard.take_denied(), 0);
        assert!(guard.shared.groups().is_empty());
    }

    #[test]
    fn mapped_ipv4_and_ipv6_masks_share_four_slots_and_drop_releases() {
        let guard = DialGuard::new(true);
        let clone = guard.clone();
        let now = Instant::now();
        let mut held = ["9.1.2.1", "::ffff:9.1.2.2", "9.1.2.3", "::ffff:9.1.2.4"]
            .map(|ip| guard.admit(&address(ip), now).unwrap())
            .into_iter()
            .collect::<Vec<_>>();
        assert!(clone.admit(&address("9.1.2.5"), now).is_none());
        assert!(clone.admit(&address("::ffff:9.1.2.6"), now).is_none());
        drop(clone.admit(&address("9.1.3.1"), now).unwrap());
        drop(held.pop());
        held.push(clone.admit(&address("9.1.2.5"), now).unwrap());
        drop(held);
        assert!(guard.shared.groups().is_empty());
        assert_eq!(clone.take_denied(), 2);
        assert_eq!(guard.take_denied(), 0);

        let held = [
            "2001:db8:1::1",
            "2001:db8:1:ffff::1",
            "2001:db8:1:8000::1",
            "2001:db8:1:1::2",
        ]
        .map(|ip| guard.admit(&address(ip), now).unwrap());
        assert!(guard.admit(&address("2001:db8:1:2::3"), now).is_none());
        drop(guard.admit(&address("2001:db8:2::1"), now).unwrap());
        drop(held);
        assert!(guard.shared.groups().is_empty());
    }

    #[test]
    fn failures_expire_exactly_at_a_minute_and_cancellation_is_not_a_failure() {
        let guard = DialGuard::new(true);
        let now = Instant::now();
        let addr = address("9.1.2.1");
        for i in 0..10 {
            let time = now + Duration::from_secs(i);
            guard.admit(&addr, time).unwrap().fail(time);
        }
        assert!(guard
            .admit(&addr, now + FAILURE_WINDOW - Duration::from_nanos(1))
            .is_none());
        drop(guard.admit(&addr, now + FAILURE_WINDOW).unwrap());
        assert_eq!(
            guard.shared.groups()[&group("9.1.2.1".parse().unwrap())]
                .failures
                .len(),
            9
        );
        assert_eq!(guard.take_denied(), 1);
        guard.cleanup(now + Duration::from_secs(69));
        assert!(guard.shared.groups().is_empty());
        for _ in 0..20 {
            drop(guard.admit(&addr, now + Duration::from_secs(70)).unwrap());
        }
        assert!(guard.shared.groups().is_empty());
        assert_eq!(guard.take_denied(), 0);
    }

    #[test]
    fn in_flight_errors_after_the_tenth_failure_are_recorded_and_histories_prune_out_of_order() {
        let guard = DialGuard::new(true);
        let now = Instant::now();
        let addr = address("9.1.2.1");
        for _ in 0..9 {
            guard.admit(&addr, now).unwrap().fail(now);
        }
        let held = std::array::from_fn::<_, 4, _>(|_| guard.admit(&addr, now).unwrap());
        for slot in held {
            slot.fail(now);
        }
        assert!(guard.admit(&addr, now).is_none());
        assert_eq!(
            guard.shared.groups()[&group("9.1.2.1".parse().unwrap())]
                .failures
                .len(),
            13
        );
        guard.cleanup(now + FAILURE_WINDOW);
        let first = guard.admit(&addr, now + FAILURE_WINDOW).unwrap();
        let second = guard.admit(&addr, now + FAILURE_WINDOW).unwrap();
        first.fail(now + Duration::from_secs(62));
        second.fail(now + Duration::from_secs(61));
        guard.cleanup(now + Duration::from_secs(121));
        assert_eq!(
            guard.shared.groups()[&group("9.1.2.1".parse().unwrap())]
                .failures
                .len(),
            1
        );
        guard.cleanup(now + Duration::from_secs(122));
        assert!(guard.shared.groups().is_empty());
    }

    #[test]
    fn cleanup_removes_one_off_groups_but_never_live_attempts() {
        let guard = DialGuard::new(true);
        let now = Instant::now();
        for i in 0..512 {
            guard
                .admit(&address(&format!("9.{}.{}.1", i / 256, i % 256)), now)
                .unwrap()
                .fail(now);
        }
        let held = guard.admit(&address("9.255.255.1"), now).unwrap();
        assert_eq!(guard.shared.groups().len(), 513);
        guard.cleanup(now + FAILURE_WINDOW);
        assert_eq!(guard.shared.groups().len(), 1);
        drop(held);
        assert!(guard.shared.groups().is_empty());
        assert_eq!(guard.take_denied(), 0);
    }

    #[test]
    fn twenty_concurrent_attempts_keep_four_slots_and_share_refusals() {
        let guard = DialGuard::new(true);
        let now = Instant::now();
        std::thread::scope(|scope| {
            let barrier = Arc::new(std::sync::Barrier::new(21));
            let (tx, rx) = std::sync::mpsc::channel();
            for _ in 0..20 {
                let (guard, barrier, tx) = (guard.clone(), barrier.clone(), tx.clone());
                scope.spawn(move || {
                    barrier.wait();
                    tx.send(guard.admit(&address("9.1.2.1"), now)).unwrap();
                });
            }
            barrier.wait();
            let held = rx.iter().take(20).collect::<Vec<_>>();
            assert_eq!(held.iter().filter(|slot| slot.is_some()).count(), 4);
            assert_eq!(guard.take_denied(), 16);
            drop(held);
        });
        assert!(guard.shared.groups().is_empty());
    }

    #[test]
    fn missing_ip_and_disabled_policy_do_not_reserve_or_refuse() {
        let now = Instant::now();
        let guard = DialGuard::new(true);
        for _ in 0..20 {
            guard
                .admit(&"/dns/localhost/tcp/1".parse().unwrap(), now)
                .unwrap()
                .fail(now);
        }
        assert!(guard.shared.groups().is_empty());
        let disabled = DialGuard::new(false);
        let held = (0..20)
            .map(|_| disabled.admit(&address("127.0.0.1"), now).unwrap())
            .collect::<Vec<_>>();
        for slot in held {
            slot.fail(now);
        }
        assert!(disabled.shared.groups().is_empty());
        assert_eq!(disabled.take_denied(), 0);
        assert_eq!(guard.take_denied(), 0);
    }
}

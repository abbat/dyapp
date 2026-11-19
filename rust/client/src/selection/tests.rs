use super::*;
use std::time::{Duration, Instant};

fn peer(id: u8) -> PeerId {
    let key = [id; 32];
    PeerId::from_bytes(
        &[0, 36, 0, 1, 0, 32]
            .into_iter()
            .chain(key)
            .collect::<Vec<_>>(),
    )
    .expect("ed25519 peer")
}

fn address(id: u8) -> Multiaddr {
    format!("/ip4/192.0.2.{id}/tcp/1").parse().unwrap()
}

fn register(policy: &mut Policy, id: u8, group: &str) -> PeerId {
    let peer = peer(id);
    policy.register(peer, group.into(), address(id));
    peer
}

fn observe(policy: &mut Policy, peer: PeerId, id: u8, cost_ms: u64, now: Instant) {
    for _ in 0..3 {
        policy.observe(
            peer,
            &address(id),
            Operation::Search,
            Outcome::Normal,
            Duration::from_millis(cost_ms),
            now,
        );
    }
}

fn observe_at(policy: &mut Policy, peer: PeerId, outcome: Outcome, now: Instant) {
    let cost = if outcome == Outcome::Normal {
        Duration::from_millis(100)
    } else {
        Duration::ZERO
    };
    policy.observe(peer, &address(1), Operation::Search, outcome, cost, now);
}

fn share(policy: &mut Policy, peer: PeerId, now: Instant) -> f64 {
    policy
        .shares(Operation::Search, now)
        .iter()
        .find_map(|(candidate, share)| (*candidate == peer).then_some(*share))
        .unwrap_or(0.0)
}

#[test]
fn caps_floors_and_group_relaxation_cover_every_pool_shape() {
    let now = Instant::now();
    let mut policy = Policy::new(0xd17);
    let fast = register(&mut policy, 1, "a");
    let medium = register(&mut policy, 2, "b");
    let slow = register(&mut policy, 3, "c");
    observe(&mut policy, fast, 1, 100, now);
    observe(&mut policy, medium, 2, 200, now);
    observe(&mut policy, slow, 3, 400, now);
    let shares = policy.shares(Operation::Search, now);
    assert_eq!(shares.len(), 3);
    assert!(shares
        .iter()
        .all(|(_, share)| (0.0999..=0.4001).contains(share)));
    assert!(share(&mut policy, fast, now) > share(&mut policy, slow, now));

    let mut two = Policy::new(0xd17);
    let quick = register(&mut two, 1, "a");
    let other = register(&mut two, 2, "b");
    observe(&mut two, quick, 1, 100, now);
    observe(&mut two, other, 2, 400, now);
    let quick_share = share(&mut two, quick, now);
    assert!(quick_share > 0.35 && quick_share <= 0.65);
    assert!((share(&mut two, other, now) - (1.0 - quick_share)).abs() < 0.0001);

    let mut uneven = Policy::new(0xd17);
    let lone = register(&mut uneven, 1, "a");
    let group = (2..=5)
        .map(|id| register(&mut uneven, id, "b"))
        .collect::<Vec<_>>();
    for (index, peer) in std::iter::once(lone)
        .chain(group.iter().copied())
        .enumerate()
    {
        observe(
            &mut uneven,
            peer,
            u8::try_from(index + 1).unwrap(),
            100,
            now,
        );
    }
    assert!((share(&mut uneven, lone, now) - 0.4).abs() < 0.0001);
    assert!(
        (group
            .iter()
            .map(|peer| share(&mut uneven, *peer, now))
            .sum::<f64>()
            - 0.6)
            .abs()
            < 0.0001
    );
}

#[test]
fn same_group_swarm_does_not_capture_more_than_half() {
    let now = Instant::now();
    let mut policy = Policy::new(0xd17);
    let swarm = (1..=3)
        .map(|id| register(&mut policy, id, "swarm"))
        .collect::<Vec<_>>();
    let honest = (4..=5)
        .map(|id| register(&mut policy, id, "honest"))
        .collect::<Vec<_>>();
    for (index, peer) in swarm.iter().chain(honest.iter()).enumerate() {
        observe(
            &mut policy,
            *peer,
            u8::try_from(index + 1).unwrap(),
            100,
            now,
        );
    }
    let captured = swarm
        .iter()
        .map(|peer| share(&mut policy, *peer, now))
        .sum::<f64>();
    assert!((captured - 0.5).abs() < 0.0001);
}

#[test]
fn slow_and_hard_outcomes_open_only_with_alternatives() {
    let now = Instant::now();
    let mut policy = Policy::new(0xd17);
    let slow = register(&mut policy, 1, "a");
    let healthy_a = register(&mut policy, 2, "b");
    let healthy_b = register(&mut policy, 3, "c");
    observe(&mut policy, healthy_a, 2, 100, now);
    observe(&mut policy, healthy_b, 3, 100, now);
    policy.shares(Operation::Search, now);
    for _ in 0..3 {
        policy.observe(
            slow,
            &address(1),
            Operation::Search,
            Outcome::Slow,
            Duration::from_secs(8),
            now,
        );
    }
    assert_eq!(policy.state(slow, now), State::Open);

    let mut sole = Policy::new(0xd17);
    let only = register(&mut sole, 1, "a");
    for _ in 0..10 {
        sole.observe(
            only,
            &address(1),
            Operation::Search,
            Outcome::Slow,
            Duration::from_secs(8),
            now,
        );
    }
    assert_eq!(sole.state(only, now), State::Degraded);
}

#[test]
fn recovery_backoff_survives_two_probes_and_resets_after_64_normal() {
    let start = Instant::now();
    let mut policy = Policy::new(0xd17);
    let bad = register(&mut policy, 1, "a");
    for offset in [0, 1] {
        observe_at(
            &mut policy,
            bad,
            Outcome::HardFault,
            start + Duration::from_secs(offset),
        );
    }
    assert_eq!(policy.state(bad, start), State::Open);
    let at = start + Duration::from_secs(61);
    assert_eq!(policy.state(bad, at), State::HalfOpen);
    for offset in [0, 1] {
        observe_at(
            &mut policy,
            bad,
            Outcome::Normal,
            at + Duration::from_secs(offset),
        );
    }
    assert_eq!(
        policy.state(bad, at + Duration::from_secs(2)),
        State::Sufficient
    );
    for offset in [3, 4] {
        observe_at(
            &mut policy,
            bad,
            Outcome::HardFault,
            at + Duration::from_secs(offset),
        );
    }
    assert_eq!(policy.state(bad, at + Duration::from_secs(4)), State::Open);
    assert_eq!(policy.open_count(bad), 2);
    let at = at + Duration::from_secs(120);
    for offset in 0..64 {
        observe_at(
            &mut policy,
            bad,
            Outcome::Normal,
            at + Duration::from_secs(offset),
        );
    }
    assert_eq!(policy.open_count(bad), 0);
}

#[test]
fn identity_probation_and_address_scoping_prevent_rotation_bypass() {
    let now = Instant::now();
    let mut policy = Policy::new(0xd17);
    let old = register(&mut policy, 1, "a");
    for _ in 0..2 {
        policy.observe(
            old,
            &address(1),
            Operation::Search,
            Outcome::HardFault,
            Duration::ZERO,
            now,
        );
    }
    let fresh = peer(2);
    policy.register(fresh, "a".into(), address(2));
    assert!(!policy.admitted(fresh));
    for _ in 0..3 {
        policy.observe(
            fresh,
            &address(2),
            Operation::Search,
            Outcome::Normal,
            Duration::from_millis(100),
            now,
        );
    }
    assert!(policy.admitted(fresh));

    policy.observe(
        fresh,
        &address(2),
        Operation::Search,
        Outcome::HardFault,
        Duration::ZERO,
        now,
    );
    assert!(!policy.address_open(fresh, &address(2)));
    assert_ne!(policy.state(fresh, now), State::Open);
    policy.observe(
        fresh,
        &address(2),
        Operation::Search,
        Outcome::Invalid,
        Duration::ZERO,
        now,
    );
    assert_eq!(policy.state(fresh, now), State::Open);
}

#[test]
fn comparative_utility_is_not_a_hard_fault() {
    let now = Instant::now();
    let mut policy = Policy::new(0xd17);
    let empty = register(&mut policy, 1, "a");
    for _ in 0..3 {
        policy.observe(
            empty,
            &address(1),
            Operation::Search,
            Outcome::ZeroYield,
            Duration::from_millis(100),
            now,
        );
    }
    assert_eq!(policy.raw_weight(empty), 1);
    assert_ne!(policy.state(empty, now), State::Open);
}

#[test]
fn rolling_mean_challenger_catches_adaptive_strikes() {
    let now = Instant::now();
    let mut policy = Policy::new(0xd17);
    let incumbent = register(&mut policy, 1, "a");
    for index in 0..16 {
        let cost = if index == 0 || index == 2 { 7490 } else { 100 };
        policy.observe(
            incumbent,
            &address(1),
            Operation::Search,
            Outcome::Normal,
            Duration::from_millis(cost),
            now + Duration::from_millis(index),
        );
    }
    let challenger = peer(2);
    policy.register(challenger, "b".into(), address(2));
    observe(&mut policy, challenger, 2, 100, now);
    assert!(policy.challenge(challenger, Operation::Search, now));
    assert!(!policy.admitted(incumbent));
    assert!(policy.admitted(challenger));
}

#[test]
fn fair_exploration_is_seeded_bounded_and_idle_free() {
    let now = Instant::now();
    let mut policy = Policy::new(0xd17);
    for id in 1..=5 {
        register(&mut policy, id, "incumbent");
    }
    for id in 6..=9 {
        register(&mut policy, id, "outside");
    }
    for _ in 0..2000 {
        policy.select(Operation::Search, 2, now);
    }
    assert_eq!(policy.exploration_count(), 100);
    for id in 6..=9 {
        assert_eq!(policy.explored(peer(id)), 25);
    }
    assert_eq!(Policy::new(1).exploration_count(), 0);
}

#[test]
fn targeting_herd_and_no_honest_candidates_do_not_invent_global_guarantees() {
    let now = Instant::now();
    let mut victim = Policy::new(1);
    let mut other = Policy::new(1);
    let same = register(&mut victim, 1, "a");
    register(&mut other, 1, "a");
    for _ in 0..2 {
        victim.observe(
            same,
            &address(1),
            Operation::Search,
            Outcome::HardFault,
            Duration::ZERO,
            now,
        );
    }
    observe(&mut other, same, 1, 100, now);
    assert_eq!(victim.state(same, now), State::Open);
    assert_eq!(other.state(same, now), State::Sufficient);

    let mut captured = Policy::new(1);
    for id in 1..=5 {
        let attacker = register(&mut captured, id, "attacker");
        observe(&mut captured, attacker, id, 7490, now);
    }
    assert_eq!(captured.shares(Operation::Search, now).len(), 5);
    assert!(!captured.challenge(peer(99), Operation::Search, now));
}

#[test]
fn operation_matrix_not_found_and_resume_cover_remaining_states() {
    let now = Instant::now();
    let mut policy = Policy::new(0xd17);
    for id in 1..=5 {
        register(&mut policy, id, "a");
    }
    for operation in [
        Operation::MailboxWrite,
        Operation::MailboxRead,
        Operation::MailboxCrossCheck,
        Operation::ProfilePublish,
        Operation::ProfileRead,
        Operation::Search,
        Operation::Media,
    ] {
        let expected = if operation == Operation::Media { 4 } else { 5 };
        assert_eq!(policy.shares(operation, now).len(), expected);
    }
    let first = peer(1);
    policy.observe(
        first,
        &address(1),
        Operation::Media,
        Outcome::NotFound,
        Duration::from_millis(100),
        now,
    );
    assert_eq!(policy.state(first, now), State::Sufficient);
    policy.observe(
        first,
        &address(1),
        Operation::Media,
        Outcome::Invalid,
        Duration::ZERO,
        now,
    );
    assert_eq!(policy.state(first, now), State::Open);
    policy.resume(now);
    assert_eq!(policy.state(first, now), State::HalfOpen);
}

#[test]
fn undersized_pool_refills_to_target() {
    let now = Instant::now();
    let mut policy = Policy::new(0xd17);
    for id in 1..=2 {
        register(&mut policy, id, "a");
    }
    assert_eq!(policy.shares(Operation::Search, now).len(), 2);
    let third = register(&mut policy, 3, "c");
    assert_eq!(policy.shares(Operation::Search, now).len(), 3);
    assert!(policy.admitted(third));
}

#[test]
fn undersized_pool_uses_ten_percent_exploration() {
    let now = Instant::now();
    let mut policy = Policy::new(0xd17);
    for id in 1..=3 {
        register(&mut policy, id, "a");
    }
    policy.shares(Operation::Search, now);
    policy.observe(
        peer(1),
        &address(1),
        Operation::Search,
        Outcome::Invalid,
        Duration::ZERO,
        now,
    );
    register(&mut policy, 4, "a");
    for _ in 0..10 {
        policy.select(Operation::Search, 1, now);
    }
    assert_eq!(policy.exploration_count(), 1);
}

#[test]
fn bounded_registry_evicts_stale_open_entry_for_a_new_candidate() {
    let now = Instant::now();
    let mut policy = Policy::new(0xd17);
    for id in 0..MAX_NODES {
        register(&mut policy, u8::try_from(id).unwrap(), "a");
    }
    let stale = peer(0);
    policy.observe(
        stale,
        &address(0),
        Operation::Search,
        Outcome::Invalid,
        Duration::ZERO,
        now,
    );
    let fresh = peer(u8::try_from(MAX_NODES).unwrap());
    policy.register(fresh, "b".into(), address(u8::try_from(MAX_NODES).unwrap()));
    assert_eq!(policy.nodes.len(), MAX_NODES);
    assert!(!policy.nodes.contains_key(&stale));
    assert!(policy.nodes.contains_key(&fresh));
}

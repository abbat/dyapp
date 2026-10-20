use super::*;

fn addr(i: usize) -> Multiaddr {
    format!("/ip4/198.51.{}.{}/tcp/9", i / 256, i % 256)
        .parse()
        .unwrap()
}

fn id(n: usize) -> ConnectionId {
    ConnectionId::new_unchecked(n)
}

/// Dials everything it is asked, numbering connections from `next`, and records the dials.
fn dialer<'a>(
    next: &'a mut usize,
    dials: &'a mut Vec<(PeerId, Multiaddr)>,
) -> impl FnMut(PeerId, &Multiaddr) -> Option<ConnectionId> + 'a {
    move |peer, address| {
        dials.push((peer, address.clone()));
        *next += 1;
        Some(id(*next))
    }
}

#[test]
fn peer_and_total_bounds_hold_with_active_dials() {
    let now = Instant::now();
    let mut claims = Claims::default();
    let peer = PeerId::random();
    for i in 0..PER_PEER {
        assert!(claims.claim(now, peer, addr(i)));
    }
    assert!(!claims.claim(now, peer, addr(99)));
    // A dial in flight still counts against the peer.
    let (mut next, mut dials) = (0, Vec::new());
    claims.start(now, dialer(&mut next, &mut dials));
    assert_eq!(dials.len(), 1);
    assert!(!claims.claim(now, peer, addr(99)));

    let mut i = 100;
    while claims.len() < TOTAL {
        assert!(claims.claim(now, PeerId::random(), addr(i)));
        i += 1;
    }
    assert!(!claims.claim(now, PeerId::random(), addr(i)));
}

#[test]
fn duplicate_claims_take_no_slot() {
    let now = Instant::now();
    let mut claims = Claims::default();
    let (a, b) = (PeerId::random(), PeerId::random());
    assert!(claims.claim(now, a, addr(1)));
    assert!(!claims.claim(now, a, addr(1)));
    assert!(!claims.claim(now, b, addr(1)));
    assert_eq!(claims.len(), 1);
    // Still pending while dialled.
    let (mut next, mut dials) = (0, Vec::new());
    claims.start(now, dialer(&mut next, &mut dials));
    assert!(!claims.claim(now, b, addr(1)));
}

#[test]
fn one_dial_per_peer_and_confirm_advances() {
    let now = Instant::now();
    let mut claims = Claims::default();
    let (a, b) = (PeerId::random(), PeerId::random());
    claims.claim(now, a, addr(1));
    claims.claim(now, a, addr(2));
    claims.claim(now, b, addr(3));
    let (mut next, mut dials) = (0, Vec::new());
    claims.start(now, dialer(&mut next, &mut dials));
    dials.sort_by_key(|(_, a)| a.to_string());
    assert_eq!(dials, vec![(a, addr(1)), (b, addr(3))]);
    // Nothing more while both dials are in flight.
    claims.start(now, dialer(&mut next, &mut dials));
    assert_eq!(dials.len(), 2);

    let first = if claims.active[&id(1)].0 == a {
        id(1)
    } else {
        id(2)
    };
    assert_eq!(claims.confirm(first, a), Some(addr(1)));
    claims.start(now, dialer(&mut next, &mut dials));
    assert_eq!(dials[2], (a, addr(2)));
}

#[test]
fn wrong_peer_stale_and_failed_dials_promote_nothing() {
    let now = Instant::now();
    let mut claims = Claims::default();
    let (a, b) = (PeerId::random(), PeerId::random());
    claims.claim(now, a, addr(1));
    claims.claim(now, a, addr(2));
    let (mut next, mut dials) = (0, Vec::new());
    claims.start(now, dialer(&mut next, &mut dials));
    // Another peer answered the dial: dropped, not promoted.
    assert_eq!(claims.confirm(id(1), b), None);
    assert_eq!(claims.confirm(id(1), a), None, "stale after the drop");
    assert_eq!(claims.confirm(id(42), a), None, "unknown connection");

    claims.start(now, dialer(&mut next, &mut dials));
    claims.fail(id(2));
    claims.fail(id(2));
    assert!(claims.is_empty());
}

#[test]
fn immediate_refusal_frees_the_slot_and_tries_the_next() {
    let now = Instant::now();
    let mut claims = Claims::default();
    let peer = PeerId::random();
    claims.claim(now, peer, addr(1));
    claims.claim(now, peer, addr(2));
    let mut tried = Vec::new();
    claims.start(now, |_, a| {
        tried.push(a.clone());
        (*a == addr(2)).then(|| id(7))
    });
    assert_eq!(tried, vec![addr(1), addr(2)]);
    assert_eq!(claims.len(), 1);
    assert_eq!(claims.confirm(id(7), peer), Some(addr(2)));
    // The refused address is cooling down too.
    assert!(!claims.claim(now, peer, addr(1)));
}

#[test]
fn cooldown_survives_disconnects_and_expires() {
    let now = Instant::now();
    let mut claims = Claims::default();
    let peer = PeerId::random();
    claims.claim(now, peer, addr(1));
    claims.claim(now, peer, addr(2));
    let (mut next, mut dials) = (0, Vec::new());
    claims.start(now, dialer(&mut next, &mut dials));
    claims.disconnected(&peer);
    assert_eq!(
        claims.len(),
        1,
        "the queued claim is dropped, the dial stays"
    );
    claims.fail(id(1));
    // addr(2) was never dialled; addr(1) cools down.
    assert!(claims.claim(now, peer, addr(2)));
    assert!(!claims.claim(now + COOLDOWN - Duration::from_secs(1), peer, addr(1)));
    assert!(claims.claim(now + COOLDOWN, peer, addr(1)));
}

#[test]
fn history_forgets_the_oldest() {
    let now = Instant::now();
    let mut claims = Claims::default();
    for i in 0..=HISTORY {
        claims.remember(now, addr(i));
    }
    assert_eq!(claims.dialled.len(), HISTORY);
    let peer = PeerId::random();
    assert!(claims.claim(now, peer, addr(0)), "forgotten");
    assert!(!claims.claim(now, peer, addr(1)));
}

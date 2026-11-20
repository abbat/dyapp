use super::*;

#[test]
fn production_weights_apply_each_health_guard_at_its_boundary() {
    let now = Instant::now();
    let mut policy = Policy::new(0xd17);
    let candidate = register(&mut policy, 1, "one");
    observe(&mut policy, candidate, 1, 100, now);
    for (confidence, degraded, zero_yield, open, expected) in [
        (0, false, 0, false, 1.0),
        (1, false, 0, false, 1.0),
        (2, false, 0, false, 1.0),
        (3, false, 0, false, 4.0),
        (4, false, 0, false, 4.0),
        (3, true, 0, false, 1.0),
        (4, true, 0, false, 1.0),
        (3, false, 2, false, 4.0),
        (3, false, 3, false, 1.0),
        (3, false, 4, false, 1.0),
        (3, false, 0, true, 1.0),
        (4, false, 0, true, 1.0),
    ] {
        let node = policy.nodes.get_mut(&candidate).unwrap();
        node.confidence = confidence;
        node.degraded = degraded;
        node.zero_yield = zero_yield;
        node.open_until = open.then_some(now);
        let weight = policy.weights(&[candidate]).into_iter().next().unwrap();
        assert!(
            (weight - expected).abs() < f64::EPSILON,
            "{confidence}/{degraded}/{zero_yield}/{open}: {weight} != {expected}"
        );
    }
}

#[test]
fn exploration_rate_changes_only_below_the_operation_pool_target() {
    let now = Instant::now();
    for (operation, target) in [(Operation::Search, 3_u8), (Operation::Media, 2)] {
        for incumbents in [target - 1, target] {
            let mut policy = Policy::new(0xd17);
            for id in 1..=incumbents {
                let registered = register(&mut policy, id, "incumbent");
                policy.nodes.get_mut(&registered).unwrap().incumbent = true;
            }
            let outside = register(&mut policy, target + 1, "outside");
            // A probationary candidate stays outside a nonempty pool until explored.
            policy.nodes.get_mut(&outside).unwrap().probation = true;
            assert_eq!(
                policy.pool(operation, None, now).len(),
                usize::from(incumbents)
            );
            for _ in 0..10 {
                policy.select(operation, 1, now);
            }
            assert_eq!(policy.exploration_count(), u64::from(incumbents < target));
        }
    }
}

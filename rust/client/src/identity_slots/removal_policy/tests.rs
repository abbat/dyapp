use super::*;

#[test]
fn cleanup_hint_preserves_every_existing_recovery_and_inventory_boundary() {
    for (reason, expected) in [
        (None, true),
        (Some(RecoveryReason::Missing), true),
        (Some(RecoveryReason::Incomplete), true),
        (Some(RecoveryReason::Deleting), true),
        (Some(RecoveryReason::Unavailable), false),
        (Some(RecoveryReason::Damaged), false),
        (Some(RecoveryReason::InvalidPointer), false),
    ] {
        assert_eq!(removal_allowed(Some("candidate"), reason, true), expected);
        assert!(!removal_allowed(Some(LEGACY), reason, true));
        assert!(!removal_allowed(None, reason, true));
        assert!(!removal_allowed(Some("candidate"), reason, false));
    }
}

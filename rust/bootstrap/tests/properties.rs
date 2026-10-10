//! Generated invariants complement fixed boundaries and historical wire fixtures.
use dyapp_identity::{Domain, Identity, SignedRecord};
use dyapp_messaging::{Message, MessageStatus};
use proptest::prelude::*;
use prost::Message as _;

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 64,
        failure_persistence: Some(Box::new(
            proptest::test_runner::FileFailurePersistence::Direct(
                "/tmp/ai/dyapp-wire-proptest-regressions.txt"
            )
        )),
        ..ProptestConfig::default()
    })]

    #[test]
    fn prop_signed_record_wire_preserves_signature_and_domain(
        secret in any::<[u8; 32]>(),
        payload in proptest::collection::vec(any::<u8>(), 0..1024),
        profile_domain in any::<bool>(),
    ) {
        let identity = Identity::from_secret(&secret);
        let (domain, other) = if profile_domain {
            (Domain::Profile, Domain::MediaKeep)
        } else {
            (Domain::MediaKeep, Domain::Profile)
        };
        let record = identity.sign(domain, payload.clone());
        let decoded = SignedRecord::decode(record.encode_to_vec().as_slice())?;
        prop_assert_eq!(&decoded, &record);
        prop_assert_eq!(&decoded.payload, &payload);
        prop_assert_eq!(decoded.verify(domain), Ok(identity.peer_id()));
        prop_assert!(decoded.verify(other).is_err());
        let mut tampered = decoded;
        tampered.payload.push(0);
        prop_assert!(tampered.verify(domain).is_err());
    }

    #[test]
    fn prop_message_json_preserves_all_fields(
        ids in proptest::collection::vec(".{0,32}", 3),
        text in ".{0,128}",
        timestamp in any::<u64>(),
        created_at in any::<i64>(),
        status_index in 0usize..5,
        payload in proptest::option::of(proptest::collection::vec(any::<u8>(), 0..128)),
        signature in proptest::option::of(proptest::collection::vec(any::<u8>(), 0..64)),
    ) {
        let statuses = [
            MessageStatus::Pending, MessageStatus::Sent, MessageStatus::Delivered,
            MessageStatus::Read, MessageStatus::Failed,
        ];
        let message = Message {
            id: ids[0].clone(),
            sender_id: ids[1].clone(),
            recipient_id: ids[2].clone(),
            text,
            lamport_timestamp: timestamp,
            created_at,
            status: statuses[status_index].clone(),
            encrypted_payload: payload,
            signature,
        };
        let decoded: Message = serde_json::from_slice(&serde_json::to_vec(&message)?)?;
        prop_assert_eq!(decoded.id, message.id);
        prop_assert_eq!(decoded.sender_id, message.sender_id);
        prop_assert_eq!(decoded.recipient_id, message.recipient_id);
        prop_assert_eq!(decoded.text, message.text);
        prop_assert_eq!(decoded.lamport_timestamp, message.lamport_timestamp);
        prop_assert_eq!(decoded.created_at, message.created_at);
        prop_assert_eq!(decoded.status, message.status);
        prop_assert_eq!(decoded.encrypted_payload, message.encrypted_payload);
        prop_assert_eq!(decoded.signature, message.signature);
    }
}

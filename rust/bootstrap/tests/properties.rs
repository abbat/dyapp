//! Generated invariants complement fixed boundaries and historical wire fixtures.
//!
//! Practice B3: every wire and storage type has a `decode(encode(x)) == x` property below;
//! `scripts/check-repository.py` fails when a protobuf or serde type is missing.
use dyapp_bootstrap::replication::ReplicationFragment;
use dyapp_identity::{Domain, Identity, SignedRecord};
use dyapp_messaging::{Message, MessageStatus, Timestamp};
use dyapp_p2p_net::proto::*;
use dyapp_profile::{DeviceCertificate, Photo, Profile};
use dyapp_video::{CodecFormat, CodecInfo, ICECandidate, ICEGatheringState};
use proptest::prelude::*;
use prost::Message as _;

fn config(name: &str) -> ProptestConfig {
    ProptestConfig {
        cases: 64,
        failure_persistence: Some(Box::new(
            proptest::test_runner::FileFailurePersistence::Direct(Box::leak(
                format!("/tmp/ai/dyapp-{name}-proptest-regressions.txt").into_boxed_str(),
            )),
        )),
        ..ProptestConfig::default()
    }
}

/// Protobuf messages: decoding the encoding gives the same value.
macro_rules! protobuf_round_trip {
    ($($name:ident: $type:ty),* $(,)?) => {
        proptest! {
            #![proptest_config(config("wire"))]
            $(
                #[test]
                fn $name(value in any::<$type>()) {
                    prop_assert_eq!(<$type>::decode(value.encode_to_vec().as_slice())?, value);
                }
            )*
        }
    };
}

/// Protobuf enums: the wire number and the schema name both map back.
macro_rules! enum_round_trip {
    ($($name:ident: $type:ty),* $(,)?) => {
        proptest! {
            #![proptest_config(config("wire"))]
            $(
                #[test]
                fn $name(value in any::<$type>()) {
                    prop_assert_eq!(<$type>::try_from(value as i32)?, value);
                    prop_assert_eq!(<$type>::from_str_name(value.as_str_name()), Some(value));
                }
            )*
        }
    };
}

/// Serde types: JSON decoding of the encoding gives the same value. `Debug` stands in for
/// `PartialEq`, which most of these types do not implement.
macro_rules! json_round_trip {
    ($($name:ident: $type:ty = $strategy:expr),* $(,)?) => {
        proptest! {
            #![proptest_config(config("json"))]
            $(
                #[test]
                fn $name(value in $strategy) {
                    let decoded: $type = serde_json::from_slice(&serde_json::to_vec(&value)?)?;
                    prop_assert_eq!(format!("{decoded:?}"), format!("{value:?}"));
                }
            )*
        }
    };
}

protobuf_round_trip! {
    signed_record: SignedRecord,
    profile: Profile,
    device_certificate: DeviceCertificate,
    photo: Photo,
    node_request: NodeRequest,
    info_request: InfoRequest,
    deny_list_request: DenyListRequest,
    turn_request: TurnRequest,
    node_response: NodeResponse,
    turn_credentials: TurnCredentials,
    deny_list: DenyList,
    node_info: NodeInfo,
    profile_request: ProfileRequest,
    get_profile: GetProfile,
    heartbeat: Heartbeat,
    profile_response: ProfileResponse,
    key_package_request: KeyPackageRequest,
    key_package_record: KeyPackageRecord,
    get_key_package: GetKeyPackage,
    key_package_response: KeyPackageResponse,
    mailbox_request: MailboxRequest,
    inventory: Inventory,
    envelopes: Envelopes,
    challenge_request: ChallengeRequest,
    envelope: Envelope,
    write_token: WriteToken,
    revoke: Revoke,
    fetch: Fetch,
    ack: Ack,
    mailbox_response: MailboxResponse,
    mailbox_push: MailboxPush,
    mailbox_push_response: MailboxPushResponse,
    media_request: MediaRequest,
    media_keep: MediaKeep,
    media_put: MediaPut,
    media_replica_put: MediaReplicaPut,
    media_manifest: MediaManifest,
    media_shard_put: MediaShardPut,
    media_shard_get: MediaShardGet,
    media_part: MediaPart,
    media_have: MediaHave,
    media_inventory: MediaInventory,
    get_media: GetMedia,
    media_attach: MediaAttach,
    media_release: MediaRelease,
    media_response: MediaResponse,
    condition: Condition,
    search_query: SearchQuery,
    search_get: SearchGet,
    search_request: SearchRequest,
    search_hit: SearchHit,
    search_response: SearchResponse,
    inventory_request: InventoryRequest,
    inventory_response: InventoryResponse,
    puzzle: Puzzle,
}

enum_round_trip! {
    status: Status,
    role: Role,
    op: Op,
    capability: Capability,
    envelope_class: EnvelopeClass,
}

json_round_trip! {
    message: Message = any::<Message>(),
    message_status: MessageStatus = any::<MessageStatus>(),
    timestamp: Timestamp = any::<Timestamp>(),
    codec_format: CodecFormat = any::<CodecFormat>(),
    codec_info: CodecInfo = any::<CodecInfo>(),
    ice_gathering_state: ICEGatheringState = any::<ICEGatheringState>(),
    ice_candidate: ICECandidate = any::<ICECandidate>(),
    replication_fragment: ReplicationFragment =
        (any::<usize>(), any::<usize>(), any::<usize>(), any::<Vec<u8>>()).prop_map(
            |(shard_id, total_shards, total_parity, data)| ReplicationFragment {
                shard_id, total_shards, total_parity, data,
            },
        ),
}

proptest! {
    #![proptest_config(config("wire"))]

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
}

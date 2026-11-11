//! Photo publication against real media ownership with controlled profile-reply boundaries.
use dyapp_bootstrap::media::{KeepSnapshot, KeepUpdate, MediaStore, Put};
use dyapp_client::{publish, store::Store, Error, Node};
use dyapp_identity::{sha256, Domain, Identity, SignedRecord};
use dyapp_p2p_net::proto::{self, media_request, profile_request, Status};
use dyapp_p2p_net::{build_configured_swarm, BehaviourEvent, Protocols, SwarmOptions};
use dyapp_profile::Profile;
use libp2p::futures::StreamExt;
use libp2p::identity::Keypair;
use libp2p::request_response::{Event, Message};
use libp2p::swarm::SwarmEvent;
use prost::Message as _;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[path = "profile_photo_race/transitions.rs"]
mod transitions;

fn path() -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../build/photo-races");
    std::fs::create_dir_all(&root).unwrap();
    root.join(uuid::Uuid::new_v4().to_string())
}

fn hashes(profile: &Profile) -> Vec<Vec<u8>> {
    profile
        .photos
        .iter()
        .flat_map(|photo| photo.full.iter().chain([&photo.thumbnail]).cloned())
        .collect()
}

fn assert_present(media: &MediaStore, hash: &[u8], bytes: &[u8]) {
    assert_eq!(
        media.get(hash).unwrap().as_deref(),
        Some(bytes),
        "newer published bytes must remain held"
    );
}

fn snapshot_reply(status: Status, snapshot: KeepSnapshot) -> proto::MediaResponse {
    proto::MediaResponse {
        status: status.into(),
        keep_state: Some(Box::new(proto::MediaKeepState {
            owner_present: snapshot.owner_present,
            version: snapshot.version,
            record: snapshot.record,
        })),
        ..Default::default()
    }
}

fn matches(state: &KeepSnapshot, expected: &proto::MediaKeepCondition) -> bool {
    let digest = state
        .record
        .as_ref()
        .map_or_else(Vec::new, |record| sha256(&record.encode_to_vec()).to_vec());
    expected.owner_present == state.owner_present
        && expected.version == state.version
        && expected.record_hash == digest
}

fn seed(media: &MediaStore, identity: &Identity, version: u64, hashes: Vec<Vec<u8>>) {
    let keep = proto::MediaKeep {
        version,
        hashes,
        time: 1,
    };
    let record = identity.sign(Domain::MediaKeep, keep.encode_to_vec());
    assert!(media
        .keep_signed(&identity.peer_id(), &keep, &record)
        .unwrap()
        .is_some());
}

type FinalizeHook = Box<dyn FnOnce(&mut Fixture) + Send>;

struct Fixture {
    owner: String,
    media: Arc<MediaStore>,
    held: SignedRecord,
    replacement: Option<SignedRecord>,
    keeps: Arc<Mutex<Vec<proto::MediaKeep>>>,
    refuse_publish_once: bool,
    before_finalize: Option<FinalizeHook>,
    quota: u64,
    reject_conditional: Option<Status>,
}

impl Fixture {
    fn profile(&mut self, request: proto::ProfileRequest) -> proto::ProfileResponse {
        let (status, record) = match request.request.unwrap() {
            profile_request::Request::Get(_) => {
                let record = self.held.clone();
                if let Some(replacement) = self.replacement.take() {
                    self.held = replacement;
                }
                (Status::Ok, Some(record))
            }
            profile_request::Request::Publish(record) => {
                if std::mem::take(&mut self.refuse_publish_once) {
                    return proto::ProfileResponse {
                        status: Status::Refused.into(),
                        ..Default::default()
                    };
                }
                if dyapp_profile::stored_version(&record).unwrap()
                    <= dyapp_profile::stored_version(&self.held).unwrap()
                {
                    (Status::Stale, Some(self.held.clone()))
                } else {
                    self.held = record;
                    (Status::Ok, None)
                }
            }
            other => panic!("unexpected profile request {other:?}"),
        };
        proto::ProfileResponse {
            status: status.into(),
            record,
            ..Default::default()
        }
    }

    fn conditional(&mut self, record: SignedRecord) -> proto::MediaResponse {
        if let Some(status) = self.reject_conditional {
            return proto::MediaResponse {
                status: status.into(),
                ..Default::default()
            };
        }
        assert!(record.verify(Domain::MediaConditionalKeep).is_ok());
        let intent = proto::MediaKeepIntent::decode(record.payload.as_slice()).unwrap();
        if intent.expected.is_some() && !intent.pending {
            if let Some(callback) = self.before_finalize.take() {
                callback(self);
            }
        }
        let current = self.media.keep_snapshot(&self.owner).unwrap();
        let Some(expected) = &intent.expected else {
            return snapshot_reply(Status::Ok, current);
        };
        if !matches(&current, expected) {
            return snapshot_reply(Status::Stale, current);
        }
        let keep = proto::MediaKeep::decode(record.payload.as_slice()).unwrap();
        self.keeps.lock().unwrap().push(keep.clone());
        match self
            .media
            .replace_keep(&self.owner, &current, &keep, &record)
            .unwrap()
        {
            KeepUpdate::Applied(missing) => proto::MediaResponse {
                keep_applied: true,
                missing,
                ..snapshot_reply(
                    Status::Ok,
                    KeepSnapshot {
                        owner_present: true,
                        version: keep.version,
                        record: Some(record),
                    },
                )
            },
            KeepUpdate::Conflict(current) => snapshot_reply(Status::Stale, current),
        }
    }

    fn media(&mut self, request: proto::MediaRequest) -> proto::MediaResponse {
        match request.request.unwrap() {
            media_request::Request::ConditionalKeep(record) => self.conditional(record),
            media_request::Request::Put(put) => {
                assert_eq!(dyapp_identity::hex(&put.owner), self.owner);
                let status = match self.media.put(&self.owner, &put.data, self.quota).unwrap() {
                    Put::Stored => Status::Ok,
                    Put::NotListed => Status::NotFound,
                    Put::OverQuota => Status::Full,
                    other @ Put::Invalid => panic!("unexpected media put {other:?}"),
                };
                proto::MediaResponse {
                    status: status.into(),
                    ..Default::default()
                }
            }
            media_request::Request::Get(get) => {
                let data = self.media.get(&get.hash).unwrap();
                proto::MediaResponse {
                    status: if data.is_some() {
                        Status::Ok
                    } else {
                        Status::NotFound
                    }
                    .into(),
                    total_size: data
                        .as_ref()
                        .map_or(0, |bytes| u64::try_from(bytes.len()).unwrap()),
                    data: data.unwrap_or_default(),
                    ..Default::default()
                }
            }
            other => panic!("unexpected media request {other:?}"),
        }
    }
}

async fn connect(fixture: Fixture) -> Node {
    let mut server = Box::new(
        build_configured_swarm(Keypair::generate_ed25519(), SwarmOptions::default()).unwrap(),
    );
    server
        .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
        .unwrap();
    let address = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let SwarmEvent::NewListenAddr { address, .. } = server.select_next_some().await {
                break address;
            }
        }
    })
    .await
    .expect("photo fixture readiness");
    tokio::spawn(serve(server, fixture));
    Node::connect(address, Arc::new(Protocols::public()))
        .await
        .unwrap()
}

async fn serve(mut server: Box<libp2p::Swarm<dyapp_p2p_net::Behaviour>>, mut fixture: Fixture) {
    loop {
        match server.select_next_some().await {
            SwarmEvent::Behaviour(BehaviourEvent::Profile(Event::Message {
                message:
                    Message::Request {
                        request, channel, ..
                    },
                ..
            })) => {
                let response = fixture.profile(request);
                server
                    .behaviour_mut()
                    .profile
                    .send_response(channel, response)
                    .unwrap();
            }
            SwarmEvent::Behaviour(BehaviourEvent::Media(Event::Message {
                message:
                    Message::Request {
                        request, channel, ..
                    },
                ..
            })) => {
                let response = fixture.media(request);
                server
                    .behaviour_mut()
                    .media
                    .send_response(channel, response)
                    .unwrap();
            }
            _ => {}
        }
    }
}

fn local() -> (Identity, Store, Profile) {
    let identity = Identity::generate();
    let mut store = Store::open(&path(), &identity).unwrap();
    store
        .add_profile_photo(&[0xff, 0xd8, 0xff, 1], &[0xff, 0xd8, 0xff, 2])
        .unwrap();
    let mut original = store.profile_draft().unwrap().unwrap();
    original.version = 5;
    store.save_profile_draft(&original).unwrap();
    (identity, store, original)
}

fn replacement(original: &Profile) -> (Profile, Vec<u8>) {
    let new_bytes = vec![0xff, 0xd8, 0xff, 3];
    let newer = Profile {
        version: 6,
        photos: vec![dyapp_profile::Photo {
            thumbnail: dyapp_identity::sha256(&new_bytes).to_vec(),
            full: vec![dyapp_identity::sha256(&new_bytes).to_vec()],
        }],
        ..original.clone()
    };
    (newer, new_bytes)
}

async fn race(publication: bool) {
    let (identity, store, original) = local();
    let (newer, new_bytes) = replacement(&original);
    let new_hash = newer.photos[0].thumbnail.clone();
    let media = Arc::new(MediaStore::open(&path()).unwrap());
    seed(&media, &identity, newer.version, hashes(&newer));
    assert_eq!(
        media
            .put(&identity.peer_id(), &new_bytes, 1_000_000)
            .unwrap(),
        Put::Stored
    );
    let keeps = Arc::new(Mutex::new(Vec::new()));
    let mut node = connect(Fixture {
        owner: identity.peer_id(),
        media: Arc::clone(&media),
        held: original.sign(&identity),
        replacement: Some(newer.sign(&identity)),
        keeps: Arc::clone(&keeps),
        refuse_publish_once: false,
        before_finalize: None,
        quota: 1_000_000,
        reject_conditional: None,
    })
    .await;
    let result = if publication {
        publish::publish(&store, &mut node, &identity, &original)
            .await
            .map(drop)
    } else {
        publish::refresh_photos(&store, &mut node, &identity, 100).await
    };
    if publication {
        assert!(matches!(
            result,
            Err(publish::Failure::Node(Error::Status(Status::Stale)))
        ));
    } else {
        assert!(
            result.is_ok(),
            "refresh must renew the union without losing newer references: {result:?}"
        );
    }
    assert_present(&media, &new_hash, &new_bytes);
    assert_eq!(
        keeps.lock().unwrap().len(),
        1,
        "a stale keep must not escalate its version"
    );
}

#[tokio::test]
async fn refresh_cannot_replace_a_newer_profiles_photos() {
    Box::pin(race(false)).await;
}

#[tokio::test]
async fn publication_cannot_release_newer_photos_while_retrying() {
    Box::pin(race(true)).await;
}

fn duplicate(identity: &Identity, store: &Store, original: &Profile, version: u64) -> Fixture {
    let media = Arc::new(MediaStore::open(&path()).unwrap());
    seed(&media, identity, version, hashes(original));
    for hash in hashes(original) {
        let bytes = store.profile_photo(&hash).unwrap().unwrap();
        assert_eq!(
            media.put(&identity.peer_id(), &bytes, 1_000_000).unwrap(),
            Put::Stored
        );
    }
    Fixture {
        owner: identity.peer_id(),
        media,
        held: original.sign(identity),
        replacement: None,
        keeps: Arc::new(Mutex::new(Vec::new())),
        refuse_publish_once: false,
        before_finalize: None,
        quota: 1_000_000,
        reject_conditional: None,
    }
}

async fn retry(local_blobs: bool) {
    let (identity, mut store, original) = local();
    let fixture = duplicate(&identity, &store, &original, 6);
    let media = Arc::clone(&fixture.media);
    let keeps = Arc::clone(&fixture.keeps);
    if !local_blobs {
        store = Store::open(&path(), &identity).unwrap();
        store.save_profile_draft(&original).unwrap();
    }
    let mut node = connect(fixture).await;
    let published = publish::publish(&store, &mut node, &identity, &original)
        .await
        .unwrap();
    assert_eq!(published.version, 6);
    assert_eq!(
        keeps.lock().unwrap().len(),
        2,
        "publication stages and finalizes without waiting for a new wall-clock second"
    );
    for hash in hashes(&published) {
        assert!(media.get(&hash).unwrap().is_some());
    }
}

#[tokio::test]
async fn exact_failed_keep_retries_without_clock_wait_or_version_escalation() {
    Box::pin(retry(true)).await;
}

#[tokio::test]
async fn stale_repair_can_fetch_verified_blobs_missing_from_the_local_store() {
    Box::pin(retry(false)).await;
}

#[tokio::test]
async fn changed_draft_recovers_after_failed_staging_and_live_refresh() {
    let (identity, store, original) = local();
    let mut fixture = duplicate(&identity, &store, &original, 6);
    fixture.refuse_publish_once = true;
    let media = Arc::clone(&fixture.media);
    let mut node = connect(fixture).await;
    let failure = publish::publish(&store, &mut node, &identity, &original).await;
    assert!(matches!(
        failure,
        Err(publish::Failure::Node(Error::Status(Status::Refused)))
    ));
    let record = media.keep_of(&identity.peer_id()).unwrap().unwrap();
    let pending = proto::MediaKeepIntent::decode(record.payload.as_slice()).unwrap();
    assert!(pending.pending);
    publish::refresh_photos(&store, &mut node, &identity, 100)
        .await
        .unwrap();
    let record = media.keep_of(&identity.peer_id()).unwrap().unwrap();
    let refreshed = proto::MediaKeepIntent::decode(record.payload.as_slice()).unwrap();
    assert!(refreshed.pending);
    assert_eq!(refreshed.intent_hash, pending.intent_hash);
    assert!(refreshed.version > pending.version);
    let (mut changed, bytes) = replacement(&original);
    changed.version = original.version;
    store.save_profile_blob(&bytes).unwrap();
    store.save_profile_draft(&changed).unwrap();
    let published = publish::publish(&store, &mut node, &identity, &changed)
        .await
        .unwrap();
    assert_eq!(published.version, 6);
    assert_eq!(published.photos, changed.photos);
    let record = media.keep_of(&identity.peer_id()).unwrap().unwrap();
    let finalized = proto::MediaKeepIntent::decode(record.payload.as_slice()).unwrap();
    assert!(!finalized.pending);
    assert_eq!(
        finalized.intent_hash,
        sha256(&published.sign(&identity).encode_to_vec())
    );
    assert!(finalized.version > refreshed.version);
    assert_eq!(media.get(&sha256(&bytes)).unwrap(), Some(bytes));
    for hash in hashes(&original) {
        assert!(media.get(&hash).unwrap().is_none());
    }
}

#[tokio::test]
async fn conditional_refresh_renews_retention_instead_of_only_repairing_bytes() {
    let (identity, store, original) = local();
    let fixture = duplicate(&identity, &store, &original, original.version);
    let keeps = Arc::clone(&fixture.keeps);
    let mut node = connect(fixture).await;
    let result = publish::refresh_photos(&store, &mut node, &identity, 100).await;
    result.unwrap();
    let keeps = keeps.lock().unwrap();
    assert_eq!(keeps.len(), 1);
    assert!(keeps[0].time > 1);
    assert_eq!(keeps[0].version, original.version + 1);
}

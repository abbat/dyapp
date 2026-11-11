//! Photo publication against real media ownership with controlled profile-reply boundaries.
use dyapp_bootstrap::media::{MediaStore, Put};
use dyapp_client::{publish, store::Store, Error, Node};
use dyapp_identity::{Identity, SignedRecord};
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

struct Fixture {
    owner: String,
    media: Arc<MediaStore>,
    held: SignedRecord,
    replacement: Option<SignedRecord>,
    keeps: Arc<Mutex<Vec<proto::MediaKeep>>>,
    seed_duplicate: bool,
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

    fn keep(&mut self, record: &SignedRecord) -> proto::MediaResponse {
        let keep = proto::MediaKeep::decode(record.payload.as_slice()).unwrap();
        self.keeps.lock().unwrap().push(keep.clone());
        if self.seed_duplicate {
            self.seed_duplicate = false;
            assert!(self
                .media
                .keep_signed(&self.owner, &keep, record)
                .unwrap()
                .is_some());
        }
        match self.media.keep_signed(&self.owner, &keep, record).unwrap() {
            Some(missing) => proto::MediaResponse {
                status: Status::Ok.into(),
                missing,
                ..Default::default()
            },
            None => proto::MediaResponse {
                status: Status::Stale.into(),
                ..Default::default()
            },
        }
    }

    fn media(&mut self, request: proto::MediaRequest) -> proto::MediaResponse {
        match request.request.unwrap() {
            media_request::Request::Keep(record) => self.keep(&record),
            media_request::Request::Put(put) => {
                assert_eq!(dyapp_identity::hex(&put.owner), self.owner);
                let status = match self.media.put(&self.owner, &put.data, 1_000_000).unwrap() {
                    Put::Stored => Status::Ok,
                    Put::NotListed => Status::NotFound,
                    other => panic!("unexpected media put {other:?}"),
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
            full: Vec::new(),
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
    media
        .keep(&identity.peer_id(), newer.version, &hashes(&newer), 1)
        .unwrap();
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
        seed_duplicate: false,
    })
    .await;
    let result = if publication {
        publish::publish(&store, &mut node, &identity, &original)
            .await
            .map(drop)
    } else {
        publish::refresh_photos(&store, &mut node, &identity, 100).await
    };
    assert!(
        matches!(
            result,
            Err(publish::Failure::Node(Error::Status(Status::NotFound)))
        ),
        "obsolete photos must fail current owner authorization: {result:?}"
    );
    assert_eq!(
        media.get(&new_hash).unwrap(),
        Some(new_bytes),
        "newer published bytes must remain held"
    );
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
    media
        .keep(&identity.peer_id(), version, &hashes(original), 1)
        .unwrap();
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
        seed_duplicate: true,
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
        1,
        "same signed-time retry does not replace its keep"
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
async fn repairing_bytes_does_not_report_a_refused_retention_refresh_as_success() {
    let (identity, store, original) = local();
    let fixture = duplicate(&identity, &store, &original, original.version);
    let keeps = Arc::clone(&fixture.keeps);
    let mut node = connect(fixture).await;
    let result = publish::refresh_photos(&store, &mut node, &identity, 100).await;
    assert!(matches!(
        result,
        Err(publish::Failure::Node(Error::Status(Status::Stale)))
    ));
    assert_eq!(keeps.lock().unwrap().len(), 1);
}

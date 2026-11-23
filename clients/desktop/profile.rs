//! The open identity's public profile: its draft in `client.db` of the slot folder, the publish
//! gate, the cancellable proof of work and the publish to the active network, over the client
//! core's `publish` module.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use dyapp_client::link::Code;
use dyapp_client::places;
use dyapp_client::publish::{self, Block, KEY_PACKAGES, SEARCH_NODES, WORK_STEP_TRIES};
use dyapp_client::search;
use dyapp_client::session::{self, Session};
use dyapp_client::store::{profile_photos, Store};
use dyapp_client::Node;
use dyapp_identity::network::{self, Network, Settings};
use dyapp_identity::Identity;
use dyapp_p2p_net::proto::Status;
use dyapp_p2p_net::Protocols;
use dyapp_profile::{CivilDate, Profile};

/// The current solve's token. Replacing or cancelling it never resets an older worker's token.
#[derive(Default)]
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub struct ProfileState(Mutex<Option<Arc<AtomicBool>>>);

impl ProfileState {
    /// Register a fresh solve and cancel the previous one, if it is still running.
    #[expect(unreachable_pub, reason = "Retained module API visibility")]
    pub fn begin(&self) -> Result<Arc<AtomicBool>, String> {
        let token = Arc::new(AtomicBool::new(false));
        let mut current = self.0.lock().map_err(|error| error.to_string())?;
        if let Some(previous) = current.replace(Arc::clone(&token)) {
            previous.store(true, Ordering::Relaxed);
        }
        Ok(token)
    }

    /// Signal the registered solve without changing a future solve's token.
    #[expect(unreachable_pub, reason = "Retained module API visibility")]
    pub fn cancel(&self) {
        let mut current = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(token) = current.take() {
            token.store(true, Ordering::Relaxed);
        }
    }
}

/// The editable public fields as the page sends and receives them.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub struct Draft {
    /// The optional public name.
    #[serde(default)]
    pub name: String,
    pub gender: u32,
    pub orientation: u32,
    pub relationship_goal: u32,
    pub role_preference: u32,
    pub career_ambition: u32,
    pub financial_philosophy: u32,
    pub employment_status: u32,
    pub fitness_level: u32,
    pub zodiac: u32,
    pub birth_year: Option<u32>,
    pub birth_month: Option<u32>,
    pub birth_day: Option<u32>,
    /// Whole centimetres, see [`dyapp_profile::valid_height`].
    #[serde(default)]
    pub height: Option<u32>,
    /// Plain text, see [`dyapp_profile::valid_about`].
    #[serde(default)]
    pub about: String,
    /// Canonical public tags, normalized before saving.
    #[serde(default)]
    pub interests: Vec<String>,
    #[serde(default)]
    pub smoking: u32,
    #[serde(default)]
    pub drinking: u32,
    #[serde(default)]
    pub tattoos: u32,
    #[serde(default)]
    pub marital_status: u32,
    #[serde(default)]
    pub pets: u32,
    /// ISO 3166-1 alpha-2 code from [`places::countries`], empty when unset.
    #[serde(default)]
    pub country: String,
    /// Decimal [`places::Place::id`] in `country`, empty when unset.
    #[serde(default)]
    pub place: String,
    /// Gender codes this profile seeks.
    #[serde(default)]
    pub seeking_genders: Vec<u32>,
    /// Lowest partner age sought, in years.
    #[serde(default)]
    pub seeking_age_from: Option<u32>,
    /// Highest partner age sought, in years.
    #[serde(default)]
    pub seeking_age_to: Option<u32>,
    /// A save drops the three fields above while this is on.
    #[serde(default)]
    pub not_looking: bool,
    /// The thumbnail hash of each photo, in order. The photo commands change them, not a save.
    #[serde(default, skip_deserializing)]
    pub photos: Vec<Vec<u8>>,
}

/// A place offered by the place picker.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub struct Place {
    pub id: u32,
    pub name: String,
    pub region: String,
}

impl From<places::Place> for Place {
    fn from(place: places::Place) -> Self {
        Self {
            id: place.id,
            name: place.name,
            region: place.region,
        }
    }
}

/// A profile picker option supplied by the shared Rust vocabulary.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub struct PickerOption {
    pub code: u32,
    pub key: &'static str,
}

/// A profile picker with stable keys for the page's localization catalogue.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub struct PickerScale {
    pub field: u32,
    pub key: &'static str,
    pub ordered: bool,
    pub choices: Vec<PickerOption>,
}

/// Profile scales in the order shown by the editor.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub fn scales() -> Vec<PickerScale> {
    dyapp_profile::SCALES
        .iter()
        .map(|scale| PickerScale {
            field: scale.field,
            key: scale.key,
            ordered: scale.ordered,
            choices: scale
                .options
                .iter()
                .map(|option| PickerOption {
                    code: option.code,
                    key: option.key,
                })
                .collect(),
        })
        .collect()
}

/// The saved draft with what the page shows beside it.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub struct View {
    pub draft: Draft,
    /// The last published version; 0 before the first publish.
    pub published: u64,
    /// Youngest and oldest possible age today; `None` without a birth year.
    pub ages: Option<(u32, u32)>,
    /// What blocks publishing besides the 18+ confirmation, as a [`code`].
    pub problem: Option<&'static str>,
    /// Minutes, rounded up, until the published profile held by the search window reaches
    /// search; `None` with nothing held.
    pub search_minutes: Option<u64>,
    /// Hidden from search by a tombstone until the next publish.
    pub hidden: bool,
    /// Opaque revision of the shown photo list; a remove or move must send it back.
    pub photo_revision: Vec<u8>,
    /// Core photo-count policy for the current editor view.
    pub max_profile_photos: usize,
    /// Core says local content remains different from the exact accepted publication.
    pub publication_pending: bool,
}

/// The page's name for a [`Block`].
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub fn code(block: Block) -> &'static str {
    match block {
        Block::NotAttested => "notAttested",
        Block::InvalidBirthDate => "invalidBirthDate",
        Block::Underage => "underage",
    }
}

#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub fn today() -> Result<CivilDate, String> {
    dyapp_client::clock::civil_date_at(SystemTime::now())
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Clock out of range".into())
}

pub(crate) fn open(dir: &Path, identity: &Identity) -> Result<Store, String> {
    Store::open(&dir.join("client.db"), identity).map_err(|e| e.to_string())
}

pub(crate) fn stored(store: &Store) -> Result<Profile, String> {
    let draft = store.profile_draft().map_err(|e| e.to_string())?;
    Ok(draft.unwrap_or_default())
}

impl From<&Profile> for Draft {
    fn from(profile: &Profile) -> Self {
        Self {
            gender: profile.gender,
            orientation: profile.orientation,
            relationship_goal: profile.relationship_goal,
            role_preference: profile.role_preference,
            career_ambition: profile.career_ambition,
            financial_philosophy: profile.financial_philosophy,
            employment_status: profile.employment_status,
            fitness_level: profile.fitness_level,
            zodiac: profile.zodiac,
            birth_year: profile.birth_year,
            birth_month: profile.birth_month,
            birth_day: profile.birth_day,
            height: profile.height,
            about: profile.about.clone(),
            interests: profile.interests.clone(),
            smoking: profile.smoking,
            drinking: profile.drinking,
            tattoos: profile.tattoos,
            marital_status: profile.marital_status,
            pets: profile.pets,
            name: profile.name.clone(),
            country: profile.country.clone(),
            place: profile.place.clone(),
            seeking_genders: profile.seeking_genders.clone(),
            seeking_age_from: profile.seeking_age_from,
            seeking_age_to: profile.seeking_age_to,
            not_looking: profile.not_looking,
            photos: profile
                .photos
                .iter()
                .map(|photo| photo.thumbnail.clone())
                .collect(),
        }
    }
}

pub(crate) fn view(
    profile: &Profile,
    today: CivilDate,
    store: Option<&Store>,
) -> Result<View, String> {
    let (search_minutes, hidden) = match store {
        Some(store) => (
            store
                .search_profile_minutes_left(unix_now()?)
                .map_err(|e| e.to_string())?,
            store.profile_hidden().map_err(|e| e.to_string())?,
        ),
        None => (None, false),
    };
    Ok(View {
        draft: Draft::from(profile),
        published: profile.version,
        ages: profile.possible_age_range(today),
        problem: publish::check(profile, true, today).err().map(code),
        search_minutes,
        hidden,
        photo_revision: profile_photos::revision(profile).to_vec(),
        max_profile_photos: profile_photos::MAX_PROFILE_PHOTOS,
        publication_pending: false,
    })
}

fn unix_now() -> Result<u64, String> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs())
}

fn blocked(block: Block) -> String {
    format!("blocked: {}", code(block))
}

/// The saved draft of the identity folder `dir`; empty before the first save.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub fn load(dir: &Path, identity: &Identity) -> Result<View, String> {
    match std::fs::symlink_metadata(dir.join("client.db")) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return view(&Profile::default(), today()?, None);
        }
        Err(error) => return Err(error.to_string()),
        Ok(_) => {}
    }
    let store = open(dir, identity)?;
    view(&stored(&store)?, today()?, Some(&store))
}

/// Canonical public tags shared by the desktop draft and field-19 search conditions.
pub(crate) fn canonical_interests(tags: &[String]) -> Result<Vec<String>, String> {
    dyapp_client::profile_editor::normalize_interests(tags).map_err(|error| error.to_string())
}

/// Saves the page's fields into the draft, keeping its version and proof-of-work progress.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub fn save(dir: &Path, identity: &Identity, draft: Draft) -> Result<View, String> {
    let interests = draft.validate()?;
    let mut store = open(dir, identity)?;
    let profile = store
        .update_profile_draft(|current| {
            *current = draft.into_profile(std::mem::take(current), interests);
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    view(&profile, today()?, Some(&store))
}

impl Draft {
    fn validate(&self) -> Result<Vec<String>, String> {
        let interests = canonical_interests(&self.interests)?;
        let editable = self
            .clone()
            .into_profile(Profile::default(), interests.clone());
        dyapp_client::profile_editor::validate(&editable).map_err(|error| match error {
            dyapp_client::profile_editor::DraftError::Seeking => "invalid profile seeking".into(),
            other => other.to_string(),
        })?;
        Ok(interests)
    }

    fn into_profile(self, mut base: Profile, interests: Vec<String>) -> Profile {
        let editable = Profile {
            name: self.name,
            gender: self.gender,
            orientation: self.orientation,
            relationship_goal: self.relationship_goal,
            role_preference: self.role_preference,
            career_ambition: self.career_ambition,
            financial_philosophy: self.financial_philosophy,
            employment_status: self.employment_status,
            fitness_level: self.fitness_level,
            zodiac: self.zodiac,
            birth_year: self.birth_year,
            birth_month: self.birth_month,
            birth_day: self.birth_day,
            height: self.height,
            about: self.about,
            smoking: self.smoking,
            drinking: self.drinking,
            tattoos: self.tattoos,
            marital_status: self.marital_status,
            pets: self.pets,
            country: self.country,
            place: self.place,
            seeking_genders: self.seeking_genders,
            seeking_age_from: self.seeking_age_from,
            seeking_age_to: self.seeking_age_to,
            not_looking: self.not_looking,
            ..Profile::default()
        };
        dyapp_client::profile_editor::apply(&editable, &mut base);
        base.interests = interests;
        base
    }
}

/// Checks the draft, then advances its proof of work of `bits`, saving after each step so a
/// later call resumes. Errors are `blocked: <code>`, `cancelled` or a storage message. The page
/// keeps the form busy meanwhile; the core merges proof/cursor progress without replacing fields.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub fn solve(
    cancel: &AtomicBool,
    dir: &Path,
    identity: &Identity,
    attested: bool,
    bits: u32,
) -> Result<(), String> {
    let mut store = open(dir, identity)?;
    let draft = stored(&store)?;
    publish::check(&draft, attested, today()?).map_err(blocked)?;
    loop {
        let work = store
            .prepare_profile_work(identity, bits, WORK_STEP_TRIES)
            .map_err(|e| e.to_string())?;
        let progress = store
            .commit_profile_work(&work)
            .map_err(|e| e.to_string())?;
        if progress.solved {
            return Ok(());
        }
        if cancel.swap(false, Ordering::Relaxed) {
            return Err("cancelled".into());
        }
    }
}

/// The first node of the active network that answers, over each row's TCP, then QUIC address.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub async fn connect(network: &Network) -> Result<Node, String> {
    dyapp_client::seed_connections::first(network)
        .await
        .map_err(|error| error.to_string())
}

/// Every answering node of the active network, at most one per seed row.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub async fn reachable(network: &Network) -> Result<Vec<Node>, String> {
    dyapp_client::seed_connections::reachable(network)
        .await
        .map_err(|error| error.to_string())
}

/// The page's code for a failed publish; anything else is shown as it is.
fn publish_code(failure: publish::Failure) -> String {
    match failure {
        publish::Failure::Node(dyapp_client::Error::Status(Status::Full)) => "full".into(),
        publish::Failure::Missed => "changed".into(),
        failure => failure.to_string(),
    }
}

/// Publishes the solved draft to the active network and checks the signed readback; see
/// `publish::publish`. Errors are `blocked: <code>`, `unsolved`, `notConnected` (no active
/// network or no answering node), `full` (the node's quota has no room for the photos) or a node
/// or storage message; each leaves the draft as it was.
/// The profile lists this device, made on the first publish under `device_key`, and its
/// `KeyPackages` follow, so peers can invite it to chats.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub async fn publish(
    root: &Path,
    dir: &Path,
    identity: &Identity,
    device_key: &Identity,
    attested: bool,
    bits: u32,
) -> Result<View, String> {
    let store = open(dir, identity)?;
    let attempt = store
        .prepare_profile_publication(identity)
        .map_err(|e| e.to_string())?;
    let today = today()?;
    publish::check(attempt.draft(), attested, today).map_err(blocked)?;
    if attempt.draft().pow_bits < bits {
        return Err("unsolved".into());
    }
    let active = network::load(dir, Settings::Active)
        .map_err(|e| e.to_string())?
        .ok_or("notConnected")?;
    let mut node = connect(&active).await?;
    let seal_key = crate::sealing_key(root, dir, &store)?;
    let mut session = Session::open_or_create_with_scores(
        store,
        identity,
        device_key,
        &seal_key,
        dyapp_client::contact_verification::scores(dir).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let published = session
        .publish_profile(&mut node, identity, device_key, attempt)
        .await
        .map_err(|error| match error {
            dyapp_client::session::Error::Publish(failure) => publish_code(failure),
            error => error.to_string(),
        })?
        .published;
    // One single-use batch per device: repeats would hit the node's quota and fail this save.
    // The last-resort package answers every invite after the batch runs out.
    if session
        .store()
        .last_resort()
        .map_err(|e| e.to_string())?
        .is_none()
    {
        session
            .publish_key_packages(&mut node, device_key, KEY_PACKAGES, &seal_key)
            .await
            .map_err(|e| e.to_string())?;
    }
    session
        .keep_last_resort(&mut node, device_key, dyapp_client::now(), &seal_key)
        .await
        .map_err(|e| e.to_string())?;
    send_held(session.store(), &mut node, &active, dir).await?;
    publication_view(session.store(), identity, &published, today)
}

fn publication_view(
    store: &Store,
    identity: &Identity,
    published: &Profile,
    today: CivilDate,
) -> Result<View, String> {
    let outcome = store
        .profile_publication_state(identity, published)
        .map_err(|error| error.to_string())?;
    let mut shown = view(&outcome.draft, today, Some(store))?;
    shown.published = outcome.published.version;
    shown.hidden = outcome.hidden;
    shown.publication_pending = outcome.pending;
    Ok(shown)
}

/// Hides the profile from search, keeping direct profile and `KeyPackage` lookup available.
/// The signed search tombstone remains pending until a search node accepts it.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub async fn hide(dir: &Path, identity: &Identity) -> Result<View, String> {
    let store = open(dir, identity)?;
    let draft = stored(&store)?;
    let active = network::load(dir, Settings::Active)
        .map_err(|e| e.to_string())?
        .ok_or("notConnected")?;
    let mut node = connect(&active).await?;
    let hidden = publish::hide(&store, identity, &draft).map_err(|e| e.to_string())?;
    send_held(&store, &mut node, &active, dir).await?;
    view(&hidden, today()?, Some(&store))
}

/// A row of the Devices list in Settings.
#[derive(serde::Serialize)]
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub struct Device {
    /// Mailbox key of the device.
    pub key: Vec<u8>,
    /// Whether it is this computer.
    pub this: bool,
}

/// The devices of the saved draft or, when `live`, of the newest profile any node holds, with this
/// computer's key `this`. The draft stands in when no node answers, and it is never replaced here,
/// as it may hold edits not yet published.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub async fn devices(
    dir: &Path,
    identity: &Identity,
    this: &[u8],
    live: bool,
) -> Result<Vec<Device>, String> {
    let draft = stored(&open(dir, identity)?)?;
    let held = match network::load(dir, Settings::Active) {
        Ok(Some(active)) if live => match connect(&active).await {
            Ok(mut node) => match dyapp_client::contact_verification::scores(dir) {
                Ok(scores) => {
                    let mut scores = scores.lock().await;
                    publish::newest_with_scores(&mut node, identity, &mut scores)
                        .await
                        .ok()
                        .flatten()
                }
                Err(_) => None,
            },
            Err(_) => None,
        },
        _ => None,
    };
    Ok(
        dyapp_client::device_directory::rows(&draft, held.as_ref(), this)
            .into_iter()
            .map(|device| Device {
                key: device.key,
                this: device.this,
            })
            .collect(),
    )
}

/// Closes the mailbox of the device with the key `device_key` and relists the profile without it;
/// see `publish::revoke_device`. Errors as in [`publish`]; a failed step may be retried.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub async fn revoke_device(
    dir: &Path,
    identity: &Identity,
    device_key: &[u8],
) -> Result<View, String> {
    let store = open(dir, identity)?;
    let draft = stored(&store)?;
    let active = network::load(dir, Settings::Active)
        .map_err(|e| e.to_string())?
        .ok_or("notConnected")?;
    let mut node = connect(&active).await?;
    let scores =
        dyapp_client::contact_verification::scores(dir).map_err(|error| error.to_string())?;
    let mut scores = scores.lock().await;
    let dropped = publish::revoke_device_with_scores(
        &store,
        &mut node,
        identity,
        &draft,
        device_key,
        &mut scores,
    )
    .await
    .map_err(publish_code)?;
    view(&dropped, today()?, Some(&store))
}

/// The link code in `text`: `bad` when it is none, `expired` when it is too old.
fn link_code(text: &str, now: u64) -> Result<Code, String> {
    dyapp_client::link::parse_input(text, now).map_err(|error| error.to_string())
}

/// Checks a pasted link code before the owner is asked to give that device full control.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub fn check_link_code(text: &str) -> Result<(), String> {
    link_code(text, unix_now()?).map(|_| ())
}

/// Sends this identity's key and local state to the device that showed the code in `text`, after
/// the owner confirmed it. Errors as in [`check_link_code`], or `notConnected` with no network.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub async fn send_link(dir: &Path, identity: &Identity, text: &str) -> Result<(), String> {
    let now = unix_now()?;
    let code = link_code(text, now)?;
    let state = open(dir, identity)?
        .link_state()
        .map_err(|e| e.to_string())?;
    let active = network::load(dir, Settings::Active)
        .map_err(|e| e.to_string())?
        .ok_or("notConnected")?;
    let mut node = connect(&active).await?;
    code.send(&mut node, identity, &state, now)
        .await
        .map_err(|e| e.to_string())
}

/// Publishes a final tombstone to profile storage before identity removal and holds it for search.
/// With no responding profile node, the caller offers Retry or Remove anyway.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub async fn withdraw(dir: &Path, identity: &Identity) -> Result<View, String> {
    let store = open(dir, identity)?;
    let draft = stored(&store)?;
    let active = network::load(dir, Settings::Active)
        .map_err(|e| e.to_string())?
        .ok_or("notConnected")?;
    let mut node = connect(&active).await?;
    let scores =
        dyapp_client::contact_verification::scores(dir).map_err(|error| error.to_string())?;
    let mut scores = scores.lock().await;
    let removed = publish::withdraw_with_scores(&store, &mut node, identity, &draft, &mut scores)
        .await
        .map_err(|e| e.to_string())?;
    drop(scores);
    send_held(&store, &mut node, &active, dir).await?;
    view(&removed, today()?, Some(&store))
}

/// Sends the published profile the search window holds once the window has ended, to up to
/// [`SEARCH_NODES`] search nodes `node` finds on `network`. Best effort: a profile no node took
/// stays held, and [`View::search_minutes`] says when to call [`send_search`].
async fn send_held(
    store: &Store,
    node: &mut Node,
    network: &Network,
    dir: &Path,
) -> Result<(), String> {
    if store
        .due_search_profile(unix_now()?)
        .map_err(|e| e.to_string())?
        .is_none()
    {
        return Ok(());
    }
    let protocols = Arc::new(Protocols::new(&network.name).map_err(|e| e.to_string())?);
    let scores =
        dyapp_client::contact_verification::scores(dir).map_err(|error| error.to_string())?;
    let mut scores = scores.lock().await;
    let mut reached =
        search::connect_search_nodes_with_scores(node, protocols, SEARCH_NODES, &mut scores).await;
    let _ =
        search::publish_due_with_scores(store, &mut reached.nodes, &mut scores, unix_now()?).await;
    Ok(())
}

/// Finishes the link of the identity in `dir` when one is pending; see `Session::finish_link`.
/// Errors are `notConnected`, `revoked` or a node or storage message, and the next pass retries.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub async fn finish_link(
    root: &Path,
    dir: &Path,
    identity: &Identity,
    device: &Identity,
) -> Result<(), String> {
    let store = open(dir, identity)?;
    if store.link_pending().map_err(|e| e.to_string())?.is_none() {
        return Ok(());
    }
    let active = network::load(dir, Settings::Active)
        .map_err(|e| e.to_string())?
        .ok_or("notConnected")?;
    let mut node = connect(&active).await?;
    let seal_key = crate::sealing_key(root, dir, &store)?;
    let now = unix_now()?;
    let scores = dyapp_client::contact_verification::scores(dir).map_err(|e| e.to_string())?;
    Session::finish_link_with_scores(
        store,
        &mut node,
        identity,
        device,
        session::LinkOptions::new(KEY_PACKAGES, now),
        &seal_key,
        scores,
    )
    .await
    .map_err(|error| match error {
        session::Error::Revoked => crate::chats::own::REVOKED.to_owned(),
        error => error.to_string(),
    })?;
    send_held(&open(dir, identity)?, &mut node, &active, dir).await
}

/// Sends a held profile whose search window has ended (see `send_held`) and returns the view.
/// Errors are `notConnected` (no active network or no answering node) or a storage message.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub async fn send_search(dir: &Path, identity: &Identity) -> Result<View, String> {
    let store = open(dir, identity)?;
    if store
        .due_search_profile(unix_now()?)
        .map_err(|e| e.to_string())?
        .is_some()
    {
        let active = network::load(dir, Settings::Active)
            .map_err(|e| e.to_string())?
            .ok_or("notConnected")?;
        let mut node = connect(&active).await?;
        send_held(&store, &mut node, &active, dir).await?;
    }
    view(&stored(&store)?, today()?, Some(&store))
}

#[cfg(test)]
#[path = "profile_tests.rs"]
mod tests;

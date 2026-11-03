//! The open identity's public profile: its draft in `client.db` of the slot folder, the publish
//! gate, the cancellable proof of work and the publish to the active network, over the client
//! core's `publish` module.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use dyapp_client::link::Code;
use dyapp_client::places;
use dyapp_client::publish::{self, Block};
use dyapp_client::search;
use dyapp_client::session::{self, Session};
use dyapp_client::store::Store;
use dyapp_client::Node;
use dyapp_identity::network::{self, Network, Settings};
use dyapp_identity::Identity;
use dyapp_p2p_net::proto::Status;
use dyapp_p2p_net::{seed, Protocols};
use dyapp_profile::{is_clean_text, CivilDate, Profile, MAX_FIELD_CHARS};

/// Profile proof of work a publish computes: the default search-node minimum, until clients
/// take it from node hints.
pub const POW_BITS: u32 = 11;
// Nonces between saves and cancellation checks: a fraction of a second of Argon2id.
const STEP: u64 = 8;
/// Search nodes a publish sends the profile to; they gossip it on.
const SEARCH_NODES: usize = 2;
/// Single-use KeyPackages each publish puts on the network for peers' first texts.
const KEY_PACKAGES: usize = 4;

/// The current solve's token. Replacing or cancelling it never resets an older worker's token.
#[derive(Default)]
pub struct ProfileState(Mutex<Option<Arc<AtomicBool>>>);

impl ProfileState {
    /// Register a fresh solve and cancel the previous one, if it is still running.
    pub fn begin(&self) -> Result<Arc<AtomicBool>, String> {
        let token = Arc::new(AtomicBool::new(false));
        let mut current = self.0.lock().map_err(|error| error.to_string())?;
        if let Some(previous) = current.replace(Arc::clone(&token)) {
            previous.store(true, Ordering::Relaxed);
        }
        Ok(token)
    }

    /// Signal the registered solve without changing a future solve's token.
    pub fn cancel(&self) {
        let mut current = self.0.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(token) = current.take() {
            token.store(true, Ordering::Relaxed);
        }
    }
}

/// The editable public fields as the page sends and receives them.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
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
    /// ISO 3166-1 alpha-2 code from [`places::countries`], empty when unset.
    #[serde(default)]
    pub country: String,
    /// Decimal [`places::Place::id`] in `country`, empty when unset.
    #[serde(default)]
    pub place: String,
    /// The thumbnail hash of each photo, in order. The photo commands change them, not a save.
    #[serde(default, skip_deserializing)]
    pub photos: Vec<Vec<u8>>,
}

/// A place offered by the place picker.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
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
pub struct PickerOption {
    pub code: u32,
    pub key: &'static str,
}

/// A profile picker with stable keys for the page's localization catalogue.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PickerScale {
    pub field: u32,
    pub key: &'static str,
    pub ordered: bool,
    pub choices: Vec<PickerOption>,
}

/// Profile scales in the order shown by the editor.
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
}

/// The page's name for a [`Block`].
pub fn code(block: Block) -> &'static str {
    match block {
        Block::NotAttested => "notAttested",
        Block::InvalidBirthDate => "invalidBirthDate",
        Block::Underage => "underage",
    }
}

pub fn today() -> Result<CivilDate, String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?;
    CivilDate::from_unix_days(now.as_secs() / 86_400).ok_or_else(|| "Clock out of range".into())
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
            name: profile.name.clone(),
            country: profile.country.clone(),
            place: profile.place.clone(),
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

/// Saves the page's fields into the draft, keeping its version and proof-of-work progress.
pub fn save(dir: &Path, identity: &Identity, draft: Draft) -> Result<View, String> {
    for (field, value) in [
        (dyapp_profile::Gender::FIELD, draft.gender),
        (dyapp_profile::Orientation::FIELD, draft.orientation),
        (
            dyapp_profile::RelationshipGoal::FIELD,
            draft.relationship_goal,
        ),
        (dyapp_profile::RolePreference::FIELD, draft.role_preference),
        (dyapp_profile::CareerAmbition::FIELD, draft.career_ambition),
        (
            dyapp_profile::FinancialPhilosophy::FIELD,
            draft.financial_philosophy,
        ),
        (
            dyapp_profile::EmploymentStatus::FIELD,
            draft.employment_status,
        ),
        (dyapp_profile::FitnessLevel::FIELD, draft.fitness_level),
        (dyapp_profile::Zodiac::FIELD, draft.zodiac),
    ] {
        if !dyapp_profile::scale(field)
            .expect("closed scale")
            .knows(value)
        {
            return Err("invalid profile scale".into());
        }
    }
    if !places::known(&draft.country, &draft.place) {
        return Err("invalid place".into());
    }
    let name = draft.name.trim();
    if name.chars().count() > MAX_FIELD_CHARS || !is_clean_text(name) {
        return Err("invalidName".into());
    }
    if !dyapp_profile::valid_height(draft.height) {
        return Err("invalidHeight".into());
    }
    let store = open(dir, identity)?;
    let profile = Profile {
        name: name.to_owned(),
        gender: draft.gender,
        orientation: draft.orientation,
        relationship_goal: draft.relationship_goal,
        role_preference: draft.role_preference,
        career_ambition: draft.career_ambition,
        financial_philosophy: draft.financial_philosophy,
        employment_status: draft.employment_status,
        fitness_level: draft.fitness_level,
        zodiac: draft.zodiac,
        birth_year: draft.birth_year,
        birth_month: draft.birth_month,
        birth_day: draft.birth_day,
        height: draft.height,
        country: draft.country,
        place: draft.place,
        ..stored(&store)?
    };
    store
        .save_profile_draft(&profile)
        .map_err(|e| e.to_string())?;
    view(&profile, today()?, Some(&store))
}

/// Checks the draft, then advances its proof of work of `bits`, saving after each step so a
/// later call resumes. Errors are `blocked: <code>`, `cancelled` or a storage message. The page
/// keeps the fields read-only meanwhile, since each step saves the draft it read.
pub fn solve(
    cancel: &AtomicBool,
    dir: &Path,
    identity: &Identity,
    attested: bool,
    bits: u32,
) -> Result<(), String> {
    let store = open(dir, identity)?;
    let mut draft = stored(&store)?;
    publish::check(&draft, attested, today()?).map_err(blocked)?;
    let key = identity.public_key();
    loop {
        let solved = publish::work(&mut draft, &key, bits, STEP);
        store
            .save_profile_draft(&draft)
            .map_err(|e| e.to_string())?;
        if solved {
            return Ok(());
        }
        if cancel.swap(false, Ordering::Relaxed) {
            return Err("cancelled".into());
        }
    }
}

/// The first node of the active network that answers, over each row's TCP, then QUIC address.
pub async fn connect(network: &Network) -> Result<Node, String> {
    let protocols = Arc::new(Protocols::new(&network.name).map_err(|e| e.to_string())?);
    let mut error = "notConnected".to_string();
    for row in &network.seeds {
        for address in seed::parse(row.trim()).into_iter().flatten() {
            match Node::connect(address, Arc::clone(&protocols)).await {
                Ok(node) => return Ok(node),
                Err(failed) => error = failed.to_string(),
            }
        }
    }
    Err(error)
}

/// Every answering node of the active network, at most one per seed row.
pub async fn reachable(network: &Network) -> Result<Vec<Node>, String> {
    let protocols = Arc::new(Protocols::new(&network.name).map_err(|e| e.to_string())?);
    let mut nodes = Vec::new();
    for row in &network.seeds {
        for address in seed::parse(row.trim()).into_iter().flatten() {
            if let Ok(node) = Node::connect(address, Arc::clone(&protocols)).await {
                nodes.push(node);
                break;
            }
        }
    }
    Ok(nodes)
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
/// KeyPackages follow, so peers can invite it to chats.
pub async fn publish(
    root: &Path,
    dir: &Path,
    identity: &Identity,
    device_key: &Identity,
    attested: bool,
    bits: u32,
) -> Result<View, String> {
    let store = open(dir, identity)?;
    let mut draft = stored(&store)?;
    let today = today()?;
    publish::check(&draft, attested, today).map_err(blocked)?;
    if draft.pow_bits < bits {
        return Err("unsolved".into());
    }
    let active = network::load(dir, Settings::Active)
        .map_err(|e| e.to_string())?
        .ok_or("notConnected")?;
    let mut node = connect(&active).await?;
    let seal_key = crate::sealing_key(root, dir, &store)?;
    let mut session = Session::open_or_create(store, identity, device_key, &seal_key)
        .map_err(|e| e.to_string())?;
    session
        .list_self(
            &mut draft,
            dyapp_client::owner_binding(device_key, &identity.public_key()),
        )
        .map_err(|e| e.to_string())?;
    let published = publish::publish(session.store(), &mut node, identity, &draft)
        .await
        .map_err(publish_code)?;
    session
        .store()
        .note_device_listed(published.version)
        .map_err(|e| e.to_string())?;
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
    send_held(session.store(), &mut node, &active).await?;
    view(&published, today, Some(session.store()))
}

/// Hides the profile from search, keeping direct profile and KeyPackage lookup available.
/// The signed search tombstone remains pending until a search node accepts it.
pub async fn hide(dir: &Path, identity: &Identity) -> Result<View, String> {
    let store = open(dir, identity)?;
    let draft = stored(&store)?;
    let active = network::load(dir, Settings::Active)
        .map_err(|e| e.to_string())?
        .ok_or("notConnected")?;
    let mut node = connect(&active).await?;
    let hidden = publish::hide(&store, identity, &draft).map_err(|e| e.to_string())?;
    send_held(&store, &mut node, &active).await?;
    view(&hidden, today()?, Some(&store))
}

/// A row of the Devices list in Settings.
#[derive(serde::Serialize)]
pub struct Device {
    /// Mailbox key of the device.
    pub key: Vec<u8>,
    /// Whether it is this computer.
    pub this: bool,
}

/// The devices of the saved draft or, when `live`, of the newest profile any node holds, with this
/// computer's key `this`. The draft stands in when no node answers, and it is never replaced here,
/// as it may hold edits not yet published.
pub async fn devices(
    dir: &Path,
    identity: &Identity,
    this: &[u8],
    live: bool,
) -> Result<Vec<Device>, String> {
    let draft = stored(&open(dir, identity)?)?;
    let held = match network::load(dir, Settings::Active) {
        Ok(Some(active)) if live => match connect(&active).await {
            Ok(mut node) => publish::newest(&mut node, identity)
                .await
                .ok()
                .flatten(),
            Err(_) => None,
        },
        _ => None,
    };
    let profile = held
        .filter(|held| held.version > draft.version)
        .unwrap_or(draft);
    Ok(publish::device_keys(&profile)
        .into_iter()
        .map(|key| Device {
            this: key == this,
            key,
        })
        .collect())
}

/// Closes the mailbox of the device with the key `device_key` and relists the profile without it;
/// see `publish::revoke_device`. Errors as in [`publish`]; a failed step may be retried.
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
    let dropped = publish::revoke_device(&store, &mut node, identity, &draft, device_key)
        .await
        .map_err(publish_code)?;
    view(&dropped, today()?, Some(&store))
}

/// The link code in `text`: `bad` when it is none, `expired` when it is too old.
fn link_code(text: &str, now: u64) -> Result<Code, String> {
    let text = text.trim();
    Code::parse(text, 0).map_err(|_| "bad")?;
    Ok(Code::parse(text, now).map_err(|_| "expired")?)
}

/// Checks a pasted link code before the owner is asked to give that device full control.
pub fn check_link_code(text: &str) -> Result<(), String> {
    link_code(text, unix_now()?).map(|_| ())
}

/// Sends this identity's key and local state to the device that showed the code in `text`, after
/// the owner confirmed it. Errors as in [`check_link_code`], or `notConnected` with no network.
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
pub async fn withdraw(dir: &Path, identity: &Identity) -> Result<View, String> {
    let store = open(dir, identity)?;
    let draft = stored(&store)?;
    let active = network::load(dir, Settings::Active)
        .map_err(|e| e.to_string())?
        .ok_or("notConnected")?;
    let mut node = connect(&active).await?;
    let removed = publish::withdraw(&store, &mut node, identity, &draft)
        .await
        .map_err(|e| e.to_string())?;
    send_held(&store, &mut node, &active).await?;
    view(&removed, today()?, Some(&store))
}

/// Sends the published profile the search window holds once the window has ended, to up to
/// [`SEARCH_NODES`] search nodes `node` finds on `network`. Best effort: a profile no node took
/// stays held, and [`View::search_minutes`] says when to call [`send_search`].
async fn send_held(store: &Store, node: &mut Node, network: &Network) -> Result<(), String> {
    if store
        .due_search_profile(unix_now()?)
        .map_err(|e| e.to_string())?
        .is_none()
    {
        return Ok(());
    }
    let protocols = Arc::new(Protocols::new(&network.name).map_err(|e| e.to_string())?);
    let mut reached = search::connect_search_nodes(node, protocols, SEARCH_NODES).await;
    let _ = search::publish_due(store, &mut reached.nodes, unix_now()?).await;
    Ok(())
}

/// Finishes the link of the identity in `dir` when one is pending; see `Session::finish_link`.
/// Errors are `notConnected`, `revoked` or a node or storage message, and the next pass retries.
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
    Session::finish_link(store, &mut node, identity, device, KEY_PACKAGES, &seal_key, now)
        .await
        .map_err(|error| match error {
            session::Error::Revoked => crate::chats::own::REVOKED.to_owned(),
            error => error.to_string(),
        })?;
    send_held(&open(dir, identity)?, &mut node, &active).await
}

/// Sends a held profile whose search window has ended (see `send_held`) and returns the view.
/// Errors are `notConnected` (no active network or no answering node) or a storage message.
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
        send_held(&store, &mut node, &active).await?;
    }
    view(&stored(&store)?, today()?, Some(&store))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_code_is_told_apart_from_an_expired_one_and_from_other_text() {
        let code = Code {
            public: [1; 32],
            mailbox: [2; 32],
            expires_at: 100,
        };
        let text = format!(" {}\n", code.text());
        assert_eq!(link_code(&text, 99), Ok(code));
        assert_eq!(link_code(&text, 100), Err("expired".to_string()));
        assert_eq!(link_code("dyapp-contact:1:ab", 99), Err("bad".to_string()));
    }

    fn temp_dir() -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("dyapp-desktop-profile-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn woman(year: Option<u32>) -> Draft {
        Draft {
            gender: dyapp_profile::Gender::Woman.code(),
            birth_year: year,
            birth_month: Some(2),
            ..Draft::default()
        }
    }

    #[test]
    fn every_public_field_can_remain_unspecified() {
        let (dir, owner) = (temp_dir(), Identity::generate());
        let empty = save(&dir, &owner, Draft::default()).unwrap();
        assert_eq!(empty.draft, Draft::default());
        assert_eq!((empty.ages, empty.problem), (None, None));
        assert_eq!(load(&dir, &owner).unwrap(), empty);
        let cancel = AtomicBool::new(false);
        assert_eq!(
            solve(&cancel, &dir, &owner, false, 4),
            Err("blocked: notAttested".into())
        );
        assert_eq!(solve(&cancel, &dir, &owner, true, 4), Ok(()));
        assert_eq!(load(&dir, &owner).unwrap().draft, Draft::default());
    }

    #[test]
    fn a_draft_survives_reopening_with_its_ages_and_problem() {
        let (dir, alice) = (temp_dir(), Identity::generate());
        let empty = load(&dir, &alice).unwrap();
        assert_eq!(empty.draft, Draft::default());
        assert_eq!(empty.problem, None);
        assert!(!dir.join("client.db").exists());
        let saved = save(&dir, &alice, woman(Some(1990))).unwrap();
        assert!(saved.ages.is_some_and(|(youngest, _)| youngest >= 30));
        assert_eq!((saved.published, saved.problem), (0, None));
        assert_eq!(load(&dir, &alice).unwrap(), saved);
        let yearless = save(&dir, &alice, woman(None)).unwrap();
        assert_eq!((yearless.ages, yearless.problem), (None, None));
        let child = save(&dir, &alice, woman(Some(2025))).unwrap();
        assert_eq!(child.problem, Some("underage"));
    }

    #[test]
    fn a_public_name_is_trimmed_and_refused_with_hidden_symbols_or_too_long() {
        let (dir, alice) = (temp_dir(), Identity::generate());
        let named = |name: String| Draft {
            name,
            ..woman(None)
        };
        let saved = save(&dir, &alice, named("  Anna  ".into())).unwrap();
        assert_eq!(saved.draft.name, "Anna");
        assert_eq!(load(&dir, &alice).unwrap().draft.name, "Anna");
        for name in ["x".repeat(MAX_FIELD_CHARS + 1), "An\u{202E}na".into()] {
            let error = save(&dir, &alice, named(name));
            assert_eq!(error, Err("invalidName".into()));
        }
        assert_eq!(load(&dir, &alice).unwrap().draft.name, "Anna");
    }

    #[test]
    fn height_is_saved_cleared_and_refused_out_of_range() {
        let (dir, alice) = (temp_dir(), Identity::generate());
        let tall = |height| Draft {
            height,
            ..woman(None)
        };
        save(&dir, &alice, tall(Some(180))).unwrap();
        assert_eq!(load(&dir, &alice).unwrap().draft.height, Some(180));
        for height in [49, 273] {
            let error = save(&dir, &alice, tall(Some(height)));
            assert_eq!(error, Err("invalidHeight".into()));
        }
        assert_eq!(load(&dir, &alice).unwrap().draft.height, Some(180));
        save(&dir, &alice, tall(None)).unwrap();
        assert_eq!(load(&dir, &alice).unwrap().draft.height, None);
    }

    #[test]
    fn only_a_picked_place_is_saved_and_it_survives_reopening() {
        let (dir, alice) = (temp_dir(), Identity::generate());
        let at = |country: &str, place: &str| Draft {
            country: country.into(),
            place: place.into(),
            ..woman(None)
        };
        let berlin = save(&dir, &alice, at("DE", "2950159")).unwrap();
        assert_eq!(load(&dir, &alice).unwrap().draft, berlin.draft);
        assert_eq!(berlin.draft.place, "2950159");
        assert!(save(&dir, &alice, at("DE", "")).is_ok());
        for (country, place) in [
            ("XX", ""),
            ("de", ""),
            ("", "2950159"),
            ("FR", "2950159"),
            ("DE", "02950159"),
            ("DE", "Berlin"),
        ] {
            let error = save(&dir, &alice, at(country, place));
            assert_eq!(error, Err("invalid place".into()), "{country} {place}");
        }
        assert_eq!(load(&dir, &alice).unwrap().draft.country, "DE");
    }

    #[test]
    fn the_puzzle_is_gated_resumes_after_cancel_and_edits_keep_it() {
        let (dir, alice) = (temp_dir(), Identity::generate());
        let cancel = AtomicBool::new(false);
        let run = |attested, bits| solve(&cancel, &dir, &alice, attested, bits);
        save(&dir, &alice, woman(None)).unwrap();
        assert_eq!(run(false, 4), Err("blocked: notAttested".into()));
        // 30 bits cannot be found in one step of eight nonces.
        cancel.store(true, Ordering::Relaxed);
        assert_eq!(run(true, 30), Err("cancelled".into()));
        let store = open(&dir, &alice).unwrap();
        let progress = stored(&store).unwrap();
        assert_eq!((progress.pow_nonce, progress.pow_bits), (STEP, 0));
        assert_eq!(run(true, 4), Ok(()));
        save(&dir, &alice, woman(Some(1990))).unwrap();
        let solved = stored(&store).unwrap();
        assert_eq!((solved.pow_bits, solved.birth_year), (4, Some(1990)));
    }

    #[tokio::test]
    async fn the_view_says_when_a_held_profile_reaches_search() {
        let (dir, alice) = (temp_dir(), Identity::generate());
        assert_eq!(load(&dir, &alice).unwrap().search_minutes, None);
        save(&dir, &alice, woman(None)).unwrap();
        assert_eq!(load(&dir, &alice).unwrap().search_minutes, None);
        // Nothing is due, so nothing needs a network.
        assert!(send_search(&dir, &alice).await.is_ok());
        let store = open(&dir, &alice).unwrap();
        let signed = |version| {
            Profile {
                version,
                ..Profile::default()
            }
            .sign(&alice)
        };
        store.hold_search_profile(&signed(1)).unwrap();
        assert_eq!(load(&dir, &alice).unwrap().search_minutes, Some(0));
        assert_eq!(send_search(&dir, &alice).await, Err("notConnected".into()));
        store
            .search_profile_sent(&signed(1), unix_now().unwrap())
            .unwrap();
        assert_eq!(load(&dir, &alice).unwrap().search_minutes, None);
        store.hold_search_profile(&signed(2)).unwrap();
        assert_eq!(load(&dir, &alice).unwrap().search_minutes, Some(10));
    }

    #[tokio::test]
    async fn publishing_needs_a_solved_puzzle_and_an_active_network() {
        let (dir, alice) = (temp_dir(), Identity::generate());
        let device = Identity::generate();
        save(&dir, &alice, woman(None)).unwrap();
        assert_eq!(
            publish(&dir, &dir, &alice, &device, true, 4).await,
            Err("unsolved".into())
        );
        solve(&AtomicBool::new(false), &dir, &alice, true, 4).unwrap();
        assert_eq!(
            publish(&dir, &dir, &alice, &device, false, 4).await,
            Err("blocked: notAttested".into())
        );
        assert_eq!(
            publish(&dir, &dir, &alice, &device, true, 4).await,
            Err("notConnected".into())
        );
    }

    #[test]
    fn each_solve_has_a_distinct_cancellable_token() {
        let state = ProfileState::default();
        let first = state.begin().unwrap();
        let second = state.begin().unwrap();
        assert!(first.load(Ordering::Relaxed));
        assert!(!second.load(Ordering::Relaxed));
        state.cancel();
        assert!(second.load(Ordering::Relaxed));
        let third = state.begin().unwrap();
        assert!(!third.load(Ordering::Relaxed));
    }
}

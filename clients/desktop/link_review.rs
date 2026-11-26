#![expect(unreachable_pub, reason = "Generated Tauri wrapper reexports")]
//! Owner-reviewed linked-device profile recovery over the shared core.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use dyapp_client::publish::{self, AdoptionConfirmError, AdoptionField, AdoptionSource};
use dyapp_client::session::{self, Session};
use dyapp_client::store::{self, Store};
use dyapp_identity::network::{self, Network, Settings};
use dyapp_identity::{Identity, SignedRecord};
use zeroize::Zeroizing;

use crate::chats::Sessions;
use crate::identity_runtime::IdentityState;
use crate::slots::{self, Slot};

const REVIEW_REQUIRED: &str = "reviewRequired";

macro_rules! fields {
    ($($name:ident),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        pub enum Field { $($name),+ }

        impl From<AdoptionField> for Field {
            fn from(field: AdoptionField) -> Self {
                match field { $(AdoptionField::$name => Self::$name),+ }
            }
        }
        impl From<Field> for AdoptionField {
            fn from(field: Field) -> Self {
                match field { $(Field::$name => Self::$name),+ }
            }
        }
    };
}
fields! {
    Name, Location, Income, HasKids, WantsMoreKids, Interests, Photos, Education,
    Profession, Company, Industry, CareerStage, WorkLifeBalance, Birth, Gender,
    Orientation, RelationshipGoal, RolePreference, CareerAmbition, FinancialPhilosophy,
    EmploymentStatus, FitnessLevel, Zodiac, Height, About, Smoking, Drinking, Tattoos,
    MaritalStatus, Pets, Seeking,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    Local,
    Remote,
}
impl From<Source> for AdoptionSource {
    fn from(source: Source) -> Self {
        match source {
            Source::Local => Self::Local,
            Source::Remote => Self::Remote,
        }
    }
}

#[derive(Clone, Copy, Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Choice {
    field: Field,
    source: Source,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotoReference {
    thumbnail: Vec<u8>,
    full: Vec<Vec<u8>>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewProfile {
    version: String,
    fields: crate::profile::Draft,
    income_from: Option<String>,
    income_to: Option<String>,
    has_kids: Option<bool>,
    wants_more_kids: Option<bool>,
    interests: Vec<String>,
    photos: Vec<PhotoReference>,
    education_level: u32,
    profession: String,
    company: String,
    industry: String,
    career_stage: String,
    work_life_balance: String,
}
impl From<&dyapp_profile::Profile> for ReviewProfile {
    fn from(profile: &dyapp_profile::Profile) -> Self {
        Self {
            version: profile.version.to_string(),
            fields: crate::profile::Draft::from(profile),
            income_from: profile.income_from.map(|value| value.to_string()),
            income_to: profile.income_to.map(|value| value.to_string()),
            has_kids: profile.has_kids,
            wants_more_kids: profile.wants_more_kids,
            interests: profile.interests.clone(),
            photos: profile
                .photos
                .iter()
                .map(|photo| PhotoReference {
                    thumbnail: photo.thumbnail.clone(),
                    full: photo.full.clone(),
                })
                .collect(),
            education_level: profile.education_level,
            profession: profile.profession.clone(),
            company: profile.company.clone(),
            industry: profile.industry.clone(),
            career_stage: profile.career_stage.clone(),
            work_life_balance: profile.work_life_balance.clone(),
        }
    }
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    review_id: String,
    local: ReviewProfile,
    remote: ReviewProfile,
    differences: Vec<Field>,
    hidden: bool,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Saved {
    published: ReviewProfile,
    draft: ReviewProfile,
    hidden: bool,
    pending: bool,
}
impl From<&store::ProfilePublicationOutcome> for Saved {
    fn from(outcome: &store::ProfilePublicationOutcome) -> Self {
        Self {
            published: ReviewProfile::from(&outcome.published),
            draft: ReviewProfile::from(&outcome.draft),
            hidden: outcome.hidden,
            pending: outcome.pending,
        }
    }
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotoPixels {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}
impl From<dyapp_client::photo::Rgba> for PhotoPixels {
    fn from(photo: dyapp_client::photo::Rgba) -> Self {
        Self {
            width: photo.width,
            height: photo.height,
            pixels: photo.pixels,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Binding {
    slot: String,
    dir: PathBuf,
    owner: String,
    device: String,
    seal_digest: [u8; 32],
}

struct Admitted {
    session: Session,
    identity: Identity,
    device: Identity,
    seal: Zeroizing<[u8; 32]>,
    binding: Binding,
    network: Network,
}

struct Review {
    binding: Binding,
    certificate: SignedRecord,
    network: Network,
    adoption: publish::AdoptionReview,
}
impl Review {
    fn snapshot(&self, id: u64) -> Snapshot {
        Snapshot {
            review_id: id.to_string(),
            local: ReviewProfile::from(self.adoption.local_profile()),
            remote: ReviewProfile::from(self.adoption.remote_profile()),
            differences: self
                .adoption
                .differences()
                .into_iter()
                .map(Into::into)
                .collect(),
            hidden: self.adoption.hidden(),
        }
    }
}

#[derive(Default)]
struct ReviewState {
    next: AtomicU64,
    current: Mutex<Option<(u64, Arc<Review>)>>,
}
impl ReviewState {
    fn lock(&self) -> MutexGuard<'_, Option<(u64, Arc<Review>)>> {
        self.current.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn insert(&self, review: Review) -> (u64, Arc<Review>) {
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let review = Arc::new(review);
        *self.lock() = Some((id, Arc::clone(&review)));
        (id, review)
    }

    fn get(&self, id: &str) -> Result<Arc<Review>, String> {
        let id = id.parse::<u64>().map_err(|_| REVIEW_REQUIRED.to_owned())?;
        match &*self.lock() {
            Some((current, review)) if *current == id => Ok(Arc::clone(review)),
            _ => Err(REVIEW_REQUIRED.into()),
        }
    }

    fn dismiss(&self, id: &str) {
        let Ok(id) = id.parse::<u64>() else {
            return;
        };
        let mut current = self.lock();
        if current.as_ref().is_some_and(|(current, _)| *current == id) {
            *current = None;
        }
    }
}

#[derive(Default)]
pub struct Reviews(Arc<ReviewState>);
impl Reviews {
    pub fn clear(&self) {
        *self.0.lock() = None;
    }
}

fn unix_now() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())
        .map(|duration| duration.as_secs())
}

fn existing_key(slot: &Slot, device: bool) -> Result<Identity, String> {
    let (path, name, message) = if device {
        (
            slot.device_path(),
            slot.device_name(),
            "Device key is missing; MLS state cannot be reset",
        )
    } else {
        (
            slot.seal_path(),
            slot.seal_name(),
            "Identity sealing key is missing or state is unreadable",
        )
    };
    crate::load_key(slot, &path, &name)?.ok_or_else(|| message.into())
}

fn admit(root: &Path, dir: &Path, identity: &Identity) -> Result<Admitted, String> {
    let slot = slots::list(root)?
        .into_iter()
        .find(|slot| slot.dir == dir && slot.state == slots::State::Ready)
        .ok_or("Unknown identity")?;
    let device = existing_key(&slot, true)?;
    let seal = Zeroizing::new(*existing_key(&slot, false)?.secret());
    let store =
        Store::open_existing(&dir.join("client.db"), identity).map_err(|e| e.to_string())?;
    let scores = dyapp_client::contact_verification::scores(dir).map_err(|e| e.to_string())?;
    let session = Session::open_with_scores(store, &seal, scores).map_err(session_error)?;
    let active = network::load(dir, Settings::Active)
        .map_err(|e| e.to_string())?
        .ok_or("notConnected")?;
    let binding = Binding {
        slot: slot.id,
        dir: dir.to_path_buf(),
        owner: identity.peer_id(),
        device: device.peer_id(),
        seal_digest: dyapp_identity::sha256(&seal[..]),
    };
    Ok(Admitted {
        session,
        identity: Identity::from_secret(&identity.secret()),
        device,
        seal,
        binding,
        network: active,
    })
}

fn store_error(error: store::Error) -> String {
    match error {
        store::Error::LinkStateReviewRequired => "privateStateReviewRequired".into(),
        store::Error::LinkEnrollmentReviewRequired => "enrollmentAuthorizationRequired".into(),
        store::Error::LinkStateChanged | store::Error::ProfileCompletionSuperseded => {
            REVIEW_REQUIRED.into()
        }
        error => error.to_string(),
    }
}

fn publish_error(error: publish::Failure) -> String {
    match error {
        publish::Failure::AdoptionReviewRequired => REVIEW_REQUIRED.into(),
        publish::Failure::Store(error) => store_error(error),
        error => error.to_string(),
    }
}

fn session_error(error: session::Error) -> String {
    match error {
        session::Error::Revoked => "revoked".into(),
        session::Error::Publish(error) => publish_error(error),
        session::Error::Store(error) => store_error(error),
        error => error.to_string(),
    }
}

fn confirm_error(error: AdoptionConfirmError) -> String {
    match error {
        AdoptionConfirmError::Selection(_) => "invalidSelection".into(),
        AdoptionConfirmError::Profile(_) => "invalidProfile".into(),
        AdoptionConfirmError::Credential(error) => session_error(*error),
        AdoptionConfirmError::DeviceNotListed => "deviceNotListed".into(),
        AdoptionConfirmError::Recovery(error) => publish_error(error),
    }
}

fn photo_error(error: publish::AdoptionPhotoError) -> String {
    match error {
        publish::AdoptionPhotoError::Index | publish::AdoptionPhotoError::Reference => {
            "invalidPhoto".into()
        }
        publish::AdoptionPhotoError::Store(error) => store_error(error),
        publish::AdoptionPhotoError::Unavailable => "photoUnavailable".into(),
    }
}

fn check_device(admitted: &Admitted, review: &Review) -> Result<(), String> {
    if admitted.binding != review.binding
        || admitted.session.certificate().map_err(session_error)? != review.certificate
        || admitted.network != review.network
    {
        return Err(REVIEW_REQUIRED.into());
    }
    Ok(())
}

fn check_current(admitted: &Admitted, review: &Review) -> Result<(), String> {
    check_device(admitted, review)?;
    if !review
        .adoption
        .is_current(admitted.session.store(), &admitted.identity)
        .map_err(store_error)?
    {
        return Err(REVIEW_REQUIRED.into());
    }
    Ok(())
}

fn choices(choices: Vec<Choice>) -> Vec<(AdoptionField, AdoptionSource)> {
    choices
        .into_iter()
        .map(|choice| (choice.field.into(), choice.source.into()))
        .collect()
}

#[tauri::command]
pub async fn prepare_link_review(
    app: tauri::AppHandle,
    state: tauri::State<'_, IdentityState>,
    sessions: tauri::State<'_, Sessions>,
    reviews: tauri::State<'_, Reviews>,
) -> Result<Snapshot, String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active().clone();
    let root = crate::config_dir(&app)?;
    let sessions = Arc::clone(&sessions.0);
    let reviews = Arc::clone(&reviews.0);
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        let _session = sessions.lock().map_err(|error| error.to_string())?;
        let admitted = admit(&root, &dir, &identity)?;
        let certificate = admitted.session.certificate().map_err(session_error)?;
        let scores = admitted.session.scores();
        let mut scores = tauri::async_runtime::block_on(scores.lock());
        let mut node =
            tauri::async_runtime::block_on(crate::profile::connect_at(&dir, &admitted.network))?;
        let review = tauri::async_runtime::block_on(publish::review_adoption_with_scores(
            admitted.session.store(),
            &mut node,
            &identity,
            &mut scores,
        ))
        .map_err(publish_error)?;
        drop(scores);
        let review = Review {
            binding: admitted.binding,
            certificate,
            network: admitted.network,
            adoption: review,
        };
        let (id, review) = reviews.insert(review);
        Ok(review.snapshot(id))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri extracts command state"
)]
pub fn preview_link_review(
    reviews: tauri::State<'_, Reviews>,
    review_id: String,
    selected: Vec<Choice>,
) -> Result<ReviewProfile, String> {
    let review = reviews.0.get(&review_id)?;
    review
        .adoption
        .select_fields(&choices(selected))
        .map(|profile| ReviewProfile::from(&profile))
        .map_err(|_| "invalidSelection".into())
}

#[tauri::command]
pub async fn confirm_link_review(
    app: tauri::AppHandle,
    state: tauri::State<'_, IdentityState>,
    sessions: tauri::State<'_, Sessions>,
    reviews: tauri::State<'_, Reviews>,
    review_id: String,
    selected: Vec<Choice>,
) -> Result<Saved, String> {
    let review = reviews.0.get(&review_id)?;
    let selected = choices(selected);
    let lease = state.lease()?;
    let (identity, dir) = lease.active().clone();
    let root = crate::config_dir(&app)?;
    let sessions = Arc::clone(&sessions.0);
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        let _session = sessions.lock().map_err(|error| error.to_string())?;
        let admitted = admit(&root, &dir, &identity)?;
        check_current(&admitted, &review)?;
        let scores = admitted.session.scores();
        let mut scores = tauri::async_runtime::block_on(scores.lock());
        let mut node =
            tauri::async_runtime::block_on(crate::profile::connect_at(&dir, &review.network))?;
        tauri::async_runtime::block_on(publish::confirm_adoption_review_with_scores(
            &admitted.session,
            &mut node,
            &identity,
            &review.adoption,
            &selected,
            &mut scores,
        ))
        .map(|outcome| Saved::from(&outcome))
        .map_err(confirm_error)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn link_review_photo(
    app: tauri::AppHandle,
    state: tauri::State<'_, IdentityState>,
    sessions: tauri::State<'_, Sessions>,
    reviews: tauri::State<'_, Reviews>,
    review_id: String,
    source: Source,
    index: u32,
) -> Result<PhotoPixels, String> {
    let review = reviews.0.get(&review_id)?;
    let lease = state.lease()?;
    let (identity, dir) = lease.active().clone();
    let root = crate::config_dir(&app)?;
    let sessions = Arc::clone(&sessions.0);
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        let _session = sessions.lock().map_err(|error| error.to_string())?;
        let admitted = admit(&root, &dir, &identity)?;
        check_current(&admitted, &review)?;
        let index = usize::try_from(index).map_err(|error| error.to_string())?;
        let source = source.into();
        let cached = review
            .adoption
            .cached_thumbnail(admitted.session.store(), source, index)
            .map_err(photo_error)?;
        let decoded = if let Some(decoded) = cached {
            decoded
        } else {
            let mut nodes = tauri::async_runtime::block_on(crate::profile::reachable_at(
                &dir,
                &review.network,
            ))?;
            tauri::async_runtime::block_on(
                review.adoption.fetch_thumbnail(&mut nodes, source, index),
            )
            .map_err(photo_error)?
        };
        check_current(&admitted, &review)?;
        Ok(PhotoPixels::from(decoded))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn resume_link_review(
    app: tauri::AppHandle,
    state: tauri::State<'_, IdentityState>,
    sessions: tauri::State<'_, Sessions>,
    reviews: tauri::State<'_, Reviews>,
    review_id: String,
) -> Result<bool, String> {
    let review = reviews.0.get(&review_id)?;
    let lease = state.lease()?;
    let (identity, dir) = lease.active().clone();
    let root = crate::config_dir(&app)?;
    let sessions = Arc::clone(&sessions.0);
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        let _session = sessions.lock().map_err(|error| error.to_string())?;
        let admitted = admit(&root, &dir, &identity)?;
        check_device(&admitted, &review)?;
        let Some(pending) = admitted
            .session
            .store()
            .link_pending()
            .map_err(store_error)?
        else {
            return Ok(false);
        };
        if pending != review.adoption.pending_link_state() {
            return Err(REVIEW_REQUIRED.into());
        }
        review
            .adoption
            .remember_device_membership(&admitted.session, &identity)
            .map_err(session_error)?;
        let Admitted {
            session,
            device,
            seal,
            ..
        } = admitted;
        let mut node =
            tauri::async_runtime::block_on(crate::profile::connect_at(&dir, &review.network))?;
        tauri::async_runtime::block_on(session.resume_link(
            &mut node,
            &identity,
            &device,
            publish::KEY_PACKAGES,
            &seal,
            unix_now()?,
        ))
        .map_err(session_error)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri extracts command state"
)]
pub fn dismiss_link_review(reviews: tauri::State<'_, Reviews>, review_id: String) {
    reviews.0.dismiss(&review_id);
}

#[cfg(test)]
#[path = "link_review_tests.rs"]
mod tests;

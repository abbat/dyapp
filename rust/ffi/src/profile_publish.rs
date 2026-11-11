//! Read-only eligibility for publishing the owner's stored profile draft.

use crate::profile_draft::database;
use crate::slot_operation::SlotWorker;
use crate::{load_profile_draft, CoreError, IdentityKey, MobileIdentityRegistry};
use dyapp_client::publish::{self, Block};
use dyapp_client::store::Store;
use dyapp_profile::{CivilDate, Profile};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn today() -> Result<CivilDate, CoreError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| CoreError::Failed(error.to_string()))?;
    CivilDate::from_unix_days(now.as_secs() / 86_400)
        .ok_or_else(|| CoreError::Failed("invalid UTC date".into()))
}

pub(crate) fn block_at(
    profile: &Profile,
    attested: bool,
    today: CivilDate,
) -> Option<&'static str> {
    publish::check(profile, attested, today)
        .err()
        .map(|block| match block {
            Block::NotAttested => "notAttested",
            Block::InvalidBirthDate => "invalidBirthDate",
            Block::Underage => "underage",
        })
}

/// Return a stable blocking reason, or none if this identity's saved draft is publishable today.
/// Attestation is supplied fresh for each attempt; this read never creates a draft or database.
#[uniffi::export]
///
/// # Errors
///
/// Returns an error if loading the draft fails or the current UTC date is unsupported.
pub fn profile_publish_block(
    dir: String,
    key: Arc<IdentityKey>,
    attested: bool,
) -> Result<Option<String>, CoreError> {
    let mut profile = Profile::default();
    if let Some(draft) = load_profile_draft(dir, key)? {
        draft.apply_to(&mut profile);
    }
    Ok(block_at(&profile, attested, today()?).map(str::to_owned))
}

const STEP_TRIES: u64 = 8;

/// One persisted proof-work step; both flags false means more work remains.
#[derive(Clone, uniffi::Record)]
pub struct ProfileWorkStep {
    /// True after the required proof has been solved.
    pub solved: bool,
    /// True when the owner cancelled before a solved result.
    pub cancelled: bool,
    /// Saved proof nonce; the next attempt starts here when more work remains.
    pub next_nonce: u64,
}

/// Cancellable profile proof work. Create a new handle for each retry after cancellation.
#[derive(uniffi::Object)]
pub struct ProfileSolve {
    dir: String,
    key: Arc<IdentityKey>,
    cancelled: AtomicBool,
    serial: Mutex<()>,
    binding: Option<SlotWorker>,
}

#[uniffi::export]
impl ProfileSolve {
    /// Prepare proof work for this identity without changing its saved draft.
    #[uniffi::constructor]
    ///
    /// # Errors
    ///
    /// Returns an error if the identity directory is not an existing absolute directory
    /// or is a symlink.
    pub fn new(dir: String, key: Arc<IdentityKey>) -> Result<Arc<Self>, CoreError> {
        Self::create(dir, key, None)
    }

    /// Bind this worker to a verified slot; each step reopens its identity under a core lease.
    /// An idle handle does not prevent switching or removal; an active cancelled step stays protected.
    ///
    /// # Errors
    /// Returns registry admission failures without creating a database or changing selection.
    #[uniffi::constructor]
    pub fn for_slot(
        registry: Arc<MobileIdentityRegistry>,
        id: String,
        expected_peer: String,
    ) -> Result<Arc<Self>, CoreError> {
        let binding = SlotWorker::new(registry, id, expected_peer);
        let operation = binding.operation()?;
        Self::create(operation.slot().directory, operation.key(), Some(binding))
    }

    /// Try eight nonces and persist progress; call off the UI thread until solved or cancelled.
    /// The shared 18+ gate is checked again for every step using the current UTC day.
    ///
    /// # Errors
    ///
    /// Returns an error if locking, opening or reading the draft, checking the
    /// current UTC date, or persisting progress fails, or the publish gate blocks it.
    pub fn run_step(&self, attested: bool) -> Result<ProfileWorkStep, CoreError> {
        self.step(attested, dyapp_profile::required_pow_bits())
    }

    /// Request cancellation; the current short step finishes and saves before stopping.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

impl ProfileSolve {
    fn create(
        dir: String,
        key: Arc<IdentityKey>,
        binding: Option<SlotWorker>,
    ) -> Result<Arc<Self>, CoreError> {
        database(&dir)?;
        Ok(Arc::new(Self {
            dir,
            key,
            cancelled: AtomicBool::new(false),
            serial: Mutex::new(()),
            binding,
        }))
    }

    fn step(&self, attested: bool, bits: u32) -> Result<ProfileWorkStep, CoreError> {
        let _operation = self
            .binding
            .as_ref()
            .map(SlotWorker::operation)
            .transpose()?;
        let _serial = self
            .serial
            .lock()
            .map_err(|error| CoreError::Failed(error.to_string()))?;
        let path = database(&self.dir)?.join("client.db");
        let store = Store::open(&path, &self.key.identity)
            .map_err(|error| CoreError::Failed(error.to_string()))?;
        let mut draft = store
            .profile_draft()
            .map_err(|error| CoreError::Failed(error.to_string()))?
            .unwrap_or_default();
        if self.cancelled.load(Ordering::Acquire) {
            return Ok(ProfileWorkStep {
                solved: false,
                cancelled: true,
                next_nonce: draft.pow_nonce,
            });
        }
        if let Some(reason) = block_at(&draft, attested, today()?) {
            return Err(CoreError::Failed(format!("blocked: {reason}")));
        }
        let solved = publish::work(
            &mut draft,
            &self.key.identity.public_key(),
            bits,
            STEP_TRIES,
        );
        store
            .save_profile_draft(&draft)
            .map_err(|error| CoreError::Failed(error.to_string()))?;
        Ok(ProfileWorkStep {
            solved,
            cancelled: !solved && self.cancelled.load(Ordering::Acquire),
            next_nonce: draft.pow_nonce,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn directory(tag: &str) -> String {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../build/native-profile-work")
            .join(tag)
            .join(IdentityKey::generate().peer_id());
        std::fs::create_dir_all(&dir).unwrap();
        dir.to_str().unwrap().to_owned()
    }

    #[test]
    fn empty_profile_needs_only_fresh_attestation_without_creating_a_database() {
        let dir = directory("gate");
        let key = IdentityKey::generate();
        assert_eq!(
            profile_publish_block(dir.clone(), key.clone(), false).unwrap(),
            Some("notAttested".into())
        );
        assert_eq!(profile_publish_block(dir.clone(), key, true).unwrap(), None);
        assert!(!Path::new(&dir).join("client.db").exists());
    }

    #[test]
    fn yearless_parts_need_no_age_guess_but_an_underage_year_blocks() {
        let today = CivilDate::new(2025, 1, 1).unwrap();
        let yearless = Profile {
            birth_month: Some(2),
            birth_day: Some(29),
            ..Profile::default()
        };
        assert_eq!(block_at(&yearless, true, today), None);
        let underage = Profile {
            birth_year: Some(2010),
            ..Profile::default()
        };
        assert_eq!(block_at(&underage, true, today), Some("underage"));
        assert_eq!(block_at(&underage, false, today), Some("notAttested"));
        let impossible = Profile {
            birth_year: Some(1900),
            birth_month: Some(2),
            birth_day: Some(29),
            ..Profile::default()
        };
        assert_eq!(block_at(&impossible, true, today), Some("invalidBirthDate"));
    }

    #[test]
    fn cancelled_work_preserves_nonce_and_new_handle_resumes() {
        let dir = directory("solve");
        let key = Arc::new(IdentityKey {
            identity: dyapp_identity::Identity::from_secret(&[7; 32]),
        });
        crate::save_profile_draft(
            dir.clone(),
            key.clone(),
            crate::PublicProfileDraft {
                gender: 1,
                ..Default::default()
            },
        )
        .unwrap();
        let first = ProfileSolve::new(dir.clone(), key.clone()).unwrap();
        let step = first.step(true, 64).unwrap();
        assert!(!step.solved);
        assert!(step.next_nonce > 0);
        first.cancel();
        let stopped = first.step(true, 64).unwrap();
        assert!(stopped.cancelled);
        assert_eq!(stopped.next_nonce, step.next_nonce);
        let stored = Store::open(&Path::new(&dir).join("client.db"), &key.identity)
            .unwrap()
            .profile_draft()
            .unwrap()
            .unwrap();
        assert_eq!((stored.gender, stored.pow_nonce), (1, step.next_nonce));

        let resumed = ProfileSolve::new(dir, key).unwrap();
        // A zero-bit target accepts the next nonce, without a probabilistic retry budget.
        let progress = resumed.step(true, 0).unwrap();
        assert!(progress.next_nonce >= step.next_nonce);
        assert!(progress.solved);
    }
}

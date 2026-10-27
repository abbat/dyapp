//! The open identity's public profile: its draft in `client.db` of the slot folder, the publish
//! gate, the cancellable proof of work and the publish to the active network, over the client
//! core's `publish` module.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use dyapp_client::publish::{self, Block};
use dyapp_client::store::Store;
use dyapp_client::Node;
use dyapp_identity::network::{self, Network, Settings};
use dyapp_identity::Identity;
use dyapp_p2p_net::{seed, Protocols};
use dyapp_profile::{CivilDate, Profile};

/// Profile proof of work a publish computes: the default search-node minimum, until clients
/// take it from node hints.
pub const POW_BITS: u32 = 11;
// Nonces between saves and cancellation checks: a fraction of a second of Argon2id.
const STEP: u64 = 8;

/// Stops a running [`solve`]; shared with the blocking thread that runs it.
#[derive(Default)]
pub struct ProfileState(pub Arc<AtomicBool>);

/// The editable public fields as the page sends and receives them.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
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

fn open(dir: &Path, identity: &Identity) -> Result<Store, String> {
    Store::open(&dir.join("client.db"), identity).map_err(|e| e.to_string())
}

fn stored(store: &Store) -> Result<Profile, String> {
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
        }
    }
}

fn view(profile: &Profile, today: CivilDate) -> View {
    View {
        draft: Draft::from(profile),
        published: profile.version,
        ages: profile.possible_age_range(today),
        problem: publish::check(profile, true, today).err().map(code),
    }
}

fn blocked(block: Block) -> String {
    format!("blocked: {}", code(block))
}

/// The saved draft of the identity folder `dir`; empty before the first save.
pub fn load(dir: &Path, identity: &Identity) -> Result<View, String> {
    match std::fs::symlink_metadata(dir.join("client.db")) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(view(&Profile::default(), today()?));
        }
        Err(error) => return Err(error.to_string()),
        Ok(_) => {}
    }
    Ok(view(&stored(&open(dir, identity)?)?, today()?))
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
        if value > dyapp_profile::scale(field).expect("closed scale").max {
            return Err("invalid profile scale".into());
        }
    }
    let store = open(dir, identity)?;
    let profile = Profile {
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
        ..stored(&store)?
    };
    store
        .save_profile_draft(&profile)
        .map_err(|e| e.to_string())?;
    Ok(view(&profile, today()?))
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

/// Publishes the solved draft to the active network and checks the signed readback; see
/// `publish::publish`. Errors are `blocked: <code>`, `unsolved`, `notConnected` (no active
/// network or no answering node) or a node or storage message; each leaves the draft as it was.
pub async fn publish(
    dir: &Path,
    identity: &Identity,
    attested: bool,
    bits: u32,
) -> Result<View, String> {
    let store = open(dir, identity)?;
    let draft = stored(&store)?;
    let today = today()?;
    publish::check(&draft, attested, today).map_err(blocked)?;
    if draft.pow_bits < bits {
        return Err("unsolved".into());
    }
    let active = network::load(dir, Settings::Active)
        .map_err(|e| e.to_string())?
        .ok_or("notConnected")?;
    let mut node = connect(&active).await?;
    let published = publish::publish(&store, &mut node, identity, &draft)
        .await
        .map_err(|e| e.to_string())?;
    Ok(view(&published, today))
}

#[cfg(test)]
mod tests {
    use super::*;

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
    async fn publishing_needs_a_solved_puzzle_and_an_active_network() {
        let (dir, alice) = (temp_dir(), Identity::generate());
        save(&dir, &alice, woman(None)).unwrap();
        assert_eq!(publish(&dir, &alice, true, 4).await, Err("unsolved".into()));
        solve(&AtomicBool::new(false), &dir, &alice, true, 4).unwrap();
        assert_eq!(
            publish(&dir, &alice, false, 4).await,
            Err("blocked: notAttested".into())
        );
        assert_eq!(
            publish(&dir, &alice, true, 4).await,
            Err("notConnected".into())
        );
    }
}

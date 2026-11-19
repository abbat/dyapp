//! The newest profile the client holds for each owner. A node can serve an older signed profile
//! that still lists a removed device; [`Profiles::accept`] refuses any version below one already
//! seen, and [`newest`] asks several replicas, keeps the highest valid version and scores the nodes
//! that served a lower one. The app stores
//! the held profiles so the guard survives a restart.

use crate::scores::{self, Scores};
use crate::selection::Operation;
use crate::{Error, Node};
use dyapp_profile::VerifiedProfile;

/// Held profile authority per owner; an observed account tombstone is terminal.
#[derive(Debug, Default, Clone)]
pub struct Profiles(Vec<VerifiedProfile>);

impl Profiles {
    /// Keep newer ordinary authority, preserving the held record at equal versions.
    /// An authorized account tombstone dominates any live fork, including a higher or equal
    /// version held first. Once held, it refuses later ordinary versions.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Rollback`] for older records that do not establish account deletion,
    /// or [`Error::Deleted`]
    /// for a newer version after a held tombstone. Returns [`Error::Input`] for search-only
    /// or ambiguous legacy withdrawals, which never become account or MLS device authority.
    pub fn accept(&mut self, profile: VerifiedProfile) -> Result<&VerifiedProfile, Error> {
        profile
            .check_storage_authorization()
            .map_err(|_| Error::Input("search withdrawal is not ordinary profile authority"))?;
        let Some(index) = self
            .0
            .iter()
            .position(|held| held.peer_id == profile.peer_id)
        else {
            self.0.push(profile);
            return self.0.last().ok_or(Error::Rollback);
        };
        let held = self.0.get_mut(index).ok_or(Error::Rollback)?;
        // An authorized final account deletion cannot be hidden by an earlier live fork.
        if profile.profile.deleted && !held.profile.deleted {
            *held = profile;
            return Ok(held);
        }
        if profile.profile.version < held.profile.version {
            return Err(Error::Rollback);
        }
        if profile.profile.version > held.profile.version {
            if held.profile.deleted {
                return Err(Error::Deleted);
            }
            *held = profile;
        }
        Ok(held)
    }

    /// The held profile of the owner with hex Peer ID `peer_id`.
    #[must_use]
    pub fn get(&self, peer_id: &str) -> Option<&VerifiedProfile> {
        self.0.iter().find(|held| held.peer_id == peer_id)
    }

    /// Every held profile, for the MLS checks in [`crate::chat`].
    #[must_use]
    pub fn all(&self) -> &[VerifiedProfile] {
        &self.0
    }
}

/// Select observed profile authority among `nodes`, failing only when none returns a valid record.
/// A final account tombstone dominates every live fork; within each kind the highest version
/// wins. A failing node or one serving authority below the selected record loses score.
///
/// # Errors
///
/// Returns the last request or reply-validation error if no node returns a valid
/// profile. With no nodes, returns the not-found status.
pub async fn newest(
    scores: &mut Scores,
    nodes: &mut [Node],
    peer_id: &str,
) -> Result<VerifiedProfile, Error> {
    let now = crate::now();
    let mut served = Vec::new();
    let mut last = Error::Status(dyapp_p2p_net::proto::Status::NotFound);
    for node in nodes {
        match node.get_profile(peer_id).await {
            Ok(profile) => served.push((node.peer(), profile)),
            Err(error) => {
                if scores::faulty(&error) {
                    scores.fault(node.peer(), now, Operation::ProfileRead);
                }
                last = error;
            }
        }
    }
    // Final account deletion dominates an otherwise higher signed live fork.
    let rank = |profile: &VerifiedProfile| (profile.profile.deleted, profile.profile.version);
    let Some(best) = served.iter().map(|(_, held)| rank(held)).max() else {
        return Err(last);
    };
    for (peer, held) in &served {
        if rank(held) < best {
            scores.fault(*peer, now, Operation::ProfileRead);
        }
    }
    served
        .into_iter()
        .map(|(_, held)| held)
        .find(|held| rank(held) == best)
        .ok_or(last)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dyapp_identity::Identity;
    use dyapp_profile::Profile;

    fn signed(owner: &Identity, version: u64) -> VerifiedProfile {
        let profile = Profile {
            version,
            ..Default::default()
        };
        dyapp_profile::verify(&profile.sign(owner)).unwrap()
    }

    #[test]
    fn an_older_profile_is_refused_and_a_newer_one_replaces_it() {
        let alice = Identity::try_generate().unwrap();
        let bob = Identity::try_generate().unwrap();
        let mut profiles = Profiles::default();
        assert_eq!(
            profiles.accept(signed(&alice, 3)).unwrap().profile.version,
            3
        );
        profiles.accept(signed(&bob, 1)).unwrap();
        assert!(matches!(
            profiles.accept(signed(&alice, 2)),
            Err(Error::Rollback)
        ));
        assert_eq!(
            profiles.accept(signed(&alice, 3)).unwrap().profile.version,
            3
        );
        assert_eq!(
            profiles.accept(signed(&alice, 5)).unwrap().profile.version,
            5
        );
        assert_eq!(profiles.get(&alice.peer_id()).unwrap().profile.version, 5);
        assert_eq!(profiles.all().len(), 2);
    }

    #[test]
    fn authorized_account_deletion_dominates_higher_and_equal_cached_live_forks() {
        let owner = Identity::from_secret(&[2; 32]);
        let source = Profile {
            version: 1,
            ..Default::default()
        }
        .sign(&owner);
        let deletion = Profile {
            previous: Some(dyapp_profile::ProfileCondition::observed(Some(&source)).unwrap()),
            ..Profile::tombstone(2, 0, 0)
        };
        let terminal = dyapp_profile::verify(&deletion.sign(&owner)).unwrap();
        for version in [2, 3] {
            let mut profiles = Profiles::default();
            profiles.accept(signed(&owner, version)).unwrap();
            assert!(profiles.accept(terminal.clone()).unwrap().profile.deleted);
            assert!(matches!(
                profiles.accept(signed(&owner, 4)),
                Err(Error::Deleted)
            ));
            assert!(profiles.get(&owner.peer_id()).unwrap().profile.deleted);
        }
    }

    #[test]
    fn an_older_search_withdrawal_never_becomes_terminal_account_authority() {
        let owner = Identity::from_secret(&[2; 32]);
        let mut profiles = Profiles::default();
        profiles.accept(signed(&owner, 3)).unwrap();
        let withdrawal =
            dyapp_profile::verify_search(&Profile::search_tombstone(2, 0, 0).sign(&owner)).unwrap();
        assert!(matches!(profiles.accept(withdrawal), Err(Error::Input(_))));
        assert!(!profiles.get(&owner.peer_id()).unwrap().profile.deleted);
        assert_eq!(
            profiles.accept(signed(&owner, 4)).unwrap().profile.version,
            4
        );
    }

    #[test]
    fn a_tombstone_is_final() {
        let alice = Identity::try_generate().unwrap();
        let mut profiles = Profiles::default();
        profiles.accept(signed(&alice, 3)).unwrap();
        let source = Profile {
            version: 3,
            ..Default::default()
        }
        .sign(&alice);
        let deletion = Profile {
            previous: Some(dyapp_profile::ProfileCondition::observed(Some(&source)).unwrap()),
            ..Profile::tombstone(4, 0, 0)
        };
        let tombstone = dyapp_profile::verify(&deletion.sign(&alice)).unwrap();
        assert!(profiles.accept(tombstone).unwrap().profile.deleted);
        assert!(matches!(
            profiles.accept(signed(&alice, 5)),
            Err(Error::Deleted)
        ));
        assert!(profiles.get(&alice.peer_id()).unwrap().profile.deleted);
    }
}

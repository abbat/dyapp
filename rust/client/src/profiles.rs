//! The newest profile the client holds for each owner. A node can serve an older signed profile
//! that still lists a removed device; [`Profiles::accept`] refuses any version below one already
//! seen, and [`newest`] asks several replicas and keeps the highest valid version. The app stores
//! the held profiles so the guard survives a restart.

use crate::{Error, Node};
use dyapp_profile::VerifiedProfile;

/// The newest profile seen per owner.
#[derive(Debug, Default, Clone)]
pub struct Profiles(Vec<VerifiedProfile>);

impl Profiles {
    /// Keeps `profile` if it is newer than the one held for its owner. An equal version keeps the
    /// held one; a lower version is refused with [`Error::Rollback`].
    pub fn accept(&mut self, profile: VerifiedProfile) -> Result<&VerifiedProfile, Error> {
        let Some(index) = self
            .0
            .iter()
            .position(|held| held.peer_id == profile.peer_id)
        else {
            self.0.push(profile);
            return Ok(&self.0[self.0.len() - 1]);
        };
        let held = &mut self.0[index];
        if profile.profile.version < held.profile.version {
            return Err(Error::Rollback);
        }
        if profile.profile.version > held.profile.version {
            *held = profile;
        }
        Ok(held)
    }

    /// The held profile of the owner with hex Peer ID `peer_id`.
    pub fn get(&self, peer_id: &str) -> Option<&VerifiedProfile> {
        self.0.iter().find(|held| held.peer_id == peer_id)
    }

    /// Every held profile, for the MLS checks in [`crate::chat`].
    pub fn all(&self) -> &[VerifiedProfile] {
        &self.0
    }
}

/// The highest valid version of `peer_id`'s profile among `nodes`, e.g. its replicas; fails
/// only if none of them returns one.
pub async fn newest(nodes: &mut [Node], peer_id: &str) -> Result<VerifiedProfile, Error> {
    let mut best: Option<VerifiedProfile> = None;
    let mut last = Error::Status(dyapp_p2p_net::proto::Status::NotFound);
    for node in nodes {
        match node.get_profile(peer_id).await {
            Ok(profile) => {
                if best
                    .as_ref()
                    .is_none_or(|held| profile.profile.version > held.profile.version)
                {
                    best = Some(profile);
                }
            }
            Err(error) => last = error,
        }
    }
    best.ok_or(last)
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
        let alice = Identity::generate();
        let bob = Identity::generate();
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
}

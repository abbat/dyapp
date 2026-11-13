//! Verified source guarding is independent of whether consent covers fields or only devices.
use super::{Profile, ProfilePublication};

pub(super) enum Source {
    Unchecked,
    Missing,
    Present(Box<Profile>),
}

impl ProfilePublication {
    // Only shared core calls this after an ordinary verified read. The editor baseline stays
    // frozen; its directory never supplies authorization for an editable-field publication.
    pub(crate) fn fields_from(&mut self, source: Option<&Profile>) {
        let authority = source.cloned().unwrap_or_default();
        self.request.devices.clone_from(&authority.devices);
        self.request
            .device_owners
            .clone_from(&authority.device_owners);
        if authority.pow_bits > self.request.pow_bits {
            (self.request.pow_nonce, self.request.pow_bits) =
                (authority.pow_nonce, authority.pow_bits);
        }
        self.source = source.map_or(Source::Missing, |profile| {
            Source::Present(Box::new(profile.clone()))
        });
        self.directory_only = false;
    }

    pub(crate) fn source_checked(&self) -> bool {
        !matches!(self.source, Source::Unchecked)
    }

    // Semantic source comparison only; this is not a server CAS token or raw wire-record digest.
    pub(crate) fn source_matches(&self, held: Option<&Profile>, next: &Profile) -> bool {
        match &self.source {
            Source::Unchecked => true,
            Source::Missing => held.is_none() || held == Some(next),
            Source::Present(source) => held == Some(source.as_ref()) || held == Some(next),
        }
    }
}

//! Search invocation and projection over the shared core search policy.

use super::{
    failed, is_partial, FoundProfile, ProfileSearchResult, ProfileSearcher, SearchCondition,
    SearchEmployer,
};
use crate::profile_network::{active_network, connect, on_network_stack};
use crate::{CoreError, IdentityKey};
use dyapp_client::compatibility;
use dyapp_client::profiles::Profiles;
use dyapp_client::search::{self, Employer, SearchFailure, SearchResults};
use dyapp_client::store::Store;
use dyapp_identity::network::Network;
use dyapp_p2p_net::{proto, Protocols};
use std::sync::Arc;

const SEARCH_NODES: usize = 3;

#[uniffi::export]
impl ProfileSearcher {
    /// Search up to `limit` hits per node (0 means 50); no conditions asks for a sample, and
    /// `employer` then narrows the verified results on this device. Each call connects again, so
    /// retry is a new call; without an answering active network it fails with `notConnected`.
    ///
    /// # Errors
    ///
    /// Returns an error if locking or storage fails, saved settings are invalid,
    /// no active network answers, or the network worker fails. Individual search
    /// node failures are reported in the result's failures and partial flag.
    /// Slot-bound workers also reject a foreign key before database access.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned arguments across the native language boundary"
    )]
    pub fn search(
        &self,
        key: Arc<IdentityKey>,
        conditions: Vec<SearchCondition>,
        employer: SearchEmployer,
        limit: u32,
    ) -> Result<ProfileSearchResult, CoreError> {
        let _operation = self.admit(Some(&key))?;
        self.execute(&key, conditions, employer, limit)
    }

    /// Search this bound identity with its reopened key; hold no other identity lease.
    ///
    /// # Errors
    /// Returns an error for an unbound worker or failed admission, storage or network work.
    pub fn search_slot(
        &self,
        conditions: Vec<SearchCondition>,
        employer: SearchEmployer,
        limit: u32,
    ) -> Result<ProfileSearchResult, CoreError> {
        let operation = self.bound_operation()?;
        self.execute(&operation.key(), conditions, employer, limit)
    }
}

impl ProfileSearcher {
    fn execute(
        &self,
        key: &IdentityKey,
        conditions: Vec<SearchCondition>,
        employer: SearchEmployer,
        limit: u32,
    ) -> Result<ProfileSearchResult, CoreError> {
        let mut cache = self.cache.lock().map_err(failed)?;
        let store = Store::open(&self.dir.join("client.db"), &key.identity).map_err(failed)?;
        let mut hidden = store.blocked_peers().map_err(failed)?;
        hidden.extend(store.passed_peers().map_err(failed)?);
        let own = store.profile_draft().map_err(failed)?.unwrap_or_default();
        let network = active_network(&self.dir)?;
        let conditions: Vec<proto::Condition> = conditions.into_iter().map(Into::into).collect();
        let (offered, mut failures, mut results) =
            self.query(&network, &conditions, limit, &cache, &hidden)?;
        Employer::from(employer).retain(&mut results.profiles);
        compatibility::rank(&own, &mut results.profiles);
        let profiles = results
            .profiles
            .into_iter()
            .map(|found| {
                let profile = FoundProfile::from(&found);
                // An older or equal version is already cached.
                let _ = cache.accept(found);
                profile
            })
            .collect();
        let partial = is_partial(results.partial, &failures);
        failures.extend(results.failures);
        Ok(ProfileSearchResult {
            offered,
            profiles,
            partial,
            failures: failures.into_iter().map(Into::into).collect(),
        })
    }

    fn query(
        &self,
        network: &Network,
        conditions: &[proto::Condition],
        limit: u32,
        cache: &Profiles,
        hidden: &[String],
    ) -> Result<(Vec<u32>, Vec<SearchFailure>, SearchResults), CoreError> {
        on_network_stack(|| {
            self.runtime.block_on(async {
                let protocols = Arc::new(Protocols::new(&network.name).map_err(failed)?);
                let mut entry = connect(network).await?;
                let mut reached =
                    search::connect_search_nodes(&mut entry, protocols, SEARCH_NODES).await;
                let results = search::search(
                    &mut reached.nodes,
                    conditions,
                    limit,
                    dyapp_profile::required_pow_bits(),
                    cache,
                    hidden,
                )
                .await;
                Ok((reached.offered, reached.failures, results))
            })
        })
    }
}

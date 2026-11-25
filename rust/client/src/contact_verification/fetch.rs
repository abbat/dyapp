//! Highest-observed ordinary profile selection for a contact comparison.
use crate::{profiles, scores::Scores};
use dyapp_identity::network::Network;
use dyapp_profile::VerifiedProfile;
use std::path::Path;

/// Why the active network could not supply an observed confirmation profile.
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    /// No configured seed answered.
    #[error("notConnected")]
    NotConnected,
    /// Answering seeds supplied no valid profile.
    #[error("unavailable")]
    Unavailable,
    /// Active network configuration is invalid.
    #[error("{0}")]
    Network(String),
}

/// Fetch signed authority from all reachable seed rows, with account deletion terminal.
/// Try each row's transport alternatives until one answers, matching desktop confirmation.
/// A terminal account tombstone dominates live forks; otherwise the highest observed version
/// wins. This does not prove that no node concealed a later record.
/// The caller must capture a context ticket before this work and recheck it afterwards.
///
/// # Errors
/// Returns invalid network configuration, no reachable seeds, or no available valid profile.
pub async fn fetch(
    dir: &Path,
    network: &Network,
    peer: &str,
    scores: &mut Scores,
) -> Result<VerifiedProfile, FetchError> {
    let mut nodes = crate::seed_connections::reachable_cached(network, dir)
        .await
        .map_err(|error| FetchError::Network(error.to_string()))?;
    if nodes.is_empty() {
        return Err(FetchError::NotConnected);
    }
    profiles::newest(scores, &mut nodes, peer)
        .await
        .map_err(|_| FetchError::Unavailable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn absent_or_invalid_seed_rows_cannot_supply_confirmation_authority() {
        let dir = dyapp_test_support::test_dir().unwrap();
        for seeds in [vec![], vec!["invalid seed".into()]] {
            let network = Network {
                seeds,
                name: String::new(),
            };
            assert!(matches!(
                fetch(dir.path(), &network, "unused", &mut Scores::default()).await,
                Err(FetchError::NotConnected)
            ));
        }
    }
}

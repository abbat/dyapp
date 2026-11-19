//! Highest-observed ordinary profile selection for a contact comparison.
use crate::{profiles, scores::Scores, Node};
use dyapp_identity::network::Network;
use dyapp_p2p_net::{seed, Protocols};
use dyapp_profile::VerifiedProfile;
use std::sync::Arc;

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
    network: &Network,
    peer: &str,
    scores: &mut Scores,
) -> Result<VerifiedProfile, FetchError> {
    let protocols = Arc::new(
        Protocols::new(&network.name).map_err(|error| FetchError::Network(error.to_string()))?,
    );
    let mut nodes = Vec::new();
    for row in &network.seeds {
        for address in seed::parse(row.trim()).into_iter().flatten() {
            if let Ok(node) = Node::connect(address, Arc::clone(&protocols)).await {
                nodes.push(node);
                break;
            }
        }
    }
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
        for seeds in [vec![], vec!["invalid seed".into()]] {
            let network = Network {
                seeds,
                name: String::new(),
            };
            assert!(matches!(
                fetch(&network, "unused", &mut Scores::default()).await,
                Err(FetchError::NotConnected)
            ));
        }
    }
}

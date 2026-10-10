use crate::error::{BootstrapError, Result};
use reed_solomon_erasure::galois_8::ReedSolomon;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationFragment {
    pub shard_id: usize,
    pub total_shards: usize,
    pub total_parity: usize,
    pub data: Vec<u8>,
}

pub struct Replication {
    data_shards: usize,
    parity_shards: usize,
}

impl Replication {
    pub fn new(data_shards: usize, parity_shards: usize) -> Result<Self> {
        if data_shards + parity_shards > 256 {
            return Err(BootstrapError::ReplicationError(
                "Too many shards (max 256)".to_string(),
            ));
        }

        Ok(Self {
            data_shards,
            parity_shards,
        })
    }

    pub fn new_with_replication_factor(replication_factor: usize) -> Result<Self> {
        match replication_factor {
            1 => Self::new(1, 0),
            2 => Self::new(1, 1),
            3 => Self::new(2, 1),
            4 => Self::new(3, 1),
            _ => Err(BootstrapError::ReplicationError(
                "Unsupported replication factor".to_string(),
            )),
        }
    }

    pub fn encode(&self, data: &[u8]) -> Result<Vec<ReplicationFragment>> {
        let rs = ReedSolomon::new(self.data_shards, self.parity_shards)
            .map_err(|e| BootstrapError::ReplicationError(format!("{:?}", e)))?;

        let shard_size = data.len().div_ceil(self.data_shards);
        let mut shards: Vec<Vec<u8>> =
            vec![vec![0; shard_size]; self.data_shards + self.parity_shards];

        for (i, chunk) in data.chunks(shard_size).enumerate() {
            shards[i][..chunk.len()].copy_from_slice(chunk);
        }

        rs.encode(&mut shards)
            .map_err(|e| BootstrapError::ReplicationError(format!("{:?}", e)))?;

        Ok(shards
            .into_iter()
            .enumerate()
            .map(|(id, data)| ReplicationFragment {
                shard_id: id,
                total_shards: self.data_shards,
                total_parity: self.parity_shards,
                data,
            })
            .collect())
    }

    pub fn decode(
        &self,
        fragments: &[ReplicationFragment],
        original_length: usize,
    ) -> Result<Vec<u8>> {
        if fragments.len() < self.data_shards {
            return Err(BootstrapError::ReplicationError(
                "Not enough shards to decode".to_string(),
            ));
        }

        let rs = ReedSolomon::new(self.data_shards, self.parity_shards)
            .map_err(|e| BootstrapError::ReplicationError(format!("{:?}", e)))?;

        let mut shards: Vec<Option<Vec<u8>>> = vec![None; self.data_shards + self.parity_shards];

        for fragment in fragments {
            if fragment.shard_id < shards.len() {
                shards[fragment.shard_id] = Some(fragment.data.clone());
            }
        }

        rs.reconstruct(&mut shards)
            .map_err(|e| BootstrapError::ReplicationError(format!("{:?}", e)))?;

        let mut result = Vec::with_capacity(original_length);
        for shard in shards.iter().take(self.data_shards) {
            result.extend_from_slice(shard.as_ref().ok_or_else(|| {
                BootstrapError::ReplicationError("Reconstructed shard missing".to_string())
            })?);
        }

        result.truncate(original_length);
        Ok(result)
    }

    pub fn replication_factor(&self) -> usize {
        (self.data_shards + self.parity_shards).div_ceil(self.data_shards)
    }

    pub fn fault_tolerance(&self) -> usize {
        self.parity_shards
    }
}

pub const MEDIA_OBJECT_BYTES: usize = 1 << 20;
pub const MEDIA_MAX_BYTES: usize = 6 * MEDIA_OBJECT_BYTES;
pub const MEDIA_PARITY: usize = 4;

pub trait MediaManifestExt {
    fn data_shards(&self) -> Option<usize>;
    fn shard_size(&self) -> Option<usize>;
    fn accepts(&self, index: usize, data: &[u8]) -> bool;
    fn billed_bytes(&self) -> Option<u64>;
}

impl MediaManifestExt for dyapp_p2p_net::proto::MediaManifest {
    /// Derives the codec from protocol constants, rejecting untrusted shapes before allocation.
    fn data_shards(&self) -> Option<usize> {
        let length = usize::try_from(self.length).ok()?;
        let k = length.div_ceil(MEDIA_OBJECT_BYTES);
        (self.hash.len() == 32
            && (1..=MEDIA_MAX_BYTES).contains(&length)
            && self.shard_hashes.len() == k + MEDIA_PARITY
            && self.shard_hashes.iter().all(|h| h.len() == 32))
        .then_some(k)
    }

    fn shard_size(&self) -> Option<usize> {
        Some((self.length as usize).div_ceil(self.data_shards()?))
    }

    fn accepts(&self, index: usize, data: &[u8]) -> bool {
        self.shard_size() == Some(data.len())
            && self
                .shard_hashes
                .get(index)
                .is_some_and(|h| h.as_slice() == dyapp_identity::sha256(data))
    }

    fn billed_bytes(&self) -> Option<u64> {
        let k = self.data_shards()? as u64;
        Some((self.length * (k + MEDIA_PARITY as u64)).div_ceil(k))
    }
}

/// Returns the unique manifest and bounded fragments of a nonempty protocol-sized blob.
pub fn media_encode(data: &[u8]) -> Result<(dyapp_p2p_net::proto::MediaManifest, Vec<Vec<u8>>)> {
    if !(1..=MEDIA_MAX_BYTES).contains(&data.len()) {
        return Err(BootstrapError::ReplicationError(
            "media length outside 1..6 MiB".into(),
        ));
    }
    let k = data.len().div_ceil(MEDIA_OBJECT_BYTES);
    let fragments = Replication::new(k, MEDIA_PARITY)?.encode(data)?;
    let shards: Vec<_> = fragments.into_iter().map(|f| f.data).collect();
    Ok((
        dyapp_p2p_net::proto::MediaManifest {
            hash: dyapp_identity::sha256(data).to_vec(),
            length: data.len() as u64,
            shard_hashes: shards
                .iter()
                .map(|s| dyapp_identity::sha256(s).to_vec())
                .collect(),
        },
        shards,
    ))
}

pub fn media_decode(
    manifest: &dyapp_p2p_net::proto::MediaManifest,
    shards: &[(usize, Vec<u8>)],
) -> Result<Vec<u8>> {
    let k = manifest
        .data_shards()
        .ok_or_else(|| BootstrapError::ReplicationError("invalid manifest".into()))?;
    let mut fragments = Vec::new();
    for (index, data) in shards {
        if !manifest.accepts(*index, data)
            || fragments
                .iter()
                .any(|f: &ReplicationFragment| f.shard_id == *index)
        {
            return Err(BootstrapError::ReplicationError(
                "invalid or duplicate shard".into(),
            ));
        }
        fragments.push(ReplicationFragment {
            shard_id: *index,
            total_shards: k,
            total_parity: MEDIA_PARITY,
            data: data.clone(),
        });
    }
    let data = Replication::new(k, MEDIA_PARITY)?.decode(&fragments, manifest.length as usize)?;
    if dyapp_identity::sha256(&data).as_slice() != manifest.hash {
        return Err(BootstrapError::ReplicationError(
            "decoded media hash mismatch".into(),
        ));
    }
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_layout_boundaries_and_four_lost_shards() {
        for (size, k) in [
            (MEDIA_OBJECT_BYTES, 1),
            (MEDIA_OBJECT_BYTES + 1, 2),
            (MEDIA_MAX_BYTES, 6),
        ] {
            let data: Vec<_> = (0..size).map(|i| (i % 251) as u8).collect();
            let (manifest, shards) = media_encode(&data).unwrap();
            assert_eq!(manifest.data_shards(), Some(k));
            assert_eq!(
                manifest.billed_bytes(),
                Some((size as u64 * (k as u64 + 4)).div_ceil(k as u64))
            );
            assert!(shards.iter().all(|s| s.len() <= MEDIA_OBJECT_BYTES));
            let surviving: Vec<_> = shards.into_iter().enumerate().skip(4).collect();
            assert_eq!(media_decode(&manifest, &surviving).unwrap(), data);
            assert!(media_decode(&manifest, &surviving[..k - 1]).is_err());
        }
        assert!(media_encode(&[]).is_err());
        assert!(media_encode(&vec![0; MEDIA_MAX_BYTES + 1]).is_err());
    }

    #[test]
    fn media_rejects_bad_shapes_shards_and_decoded_hashes() {
        let (mut manifest, shards) = media_encode(b"proof").unwrap();
        let mut bad = manifest.clone();
        bad.length = u64::MAX;
        assert_eq!(bad.data_shards(), None);
        assert!(!manifest.accepts(10, &shards[0]));
        let mut altered = shards[0].clone();
        altered[0] ^= 1;
        assert!(media_decode(&manifest, &[(0, altered)]).is_err());
        assert!(
            media_decode(&manifest, &[(0, shards[0].clone()), (0, shards[0].clone())]).is_err()
        );
        manifest.hash[0] ^= 1;
        assert!(media_decode(&manifest, &[(0, shards[0].clone())]).is_err());
    }

    #[test]
    fn test_replication_creation() {
        let rep = Replication::new(2, 1).unwrap();
        assert_eq!(rep.replication_factor(), 2);
        assert_eq!(rep.fault_tolerance(), 1);
    }

    #[test]
    fn test_replication_from_factor() {
        let rep = Replication::new_with_replication_factor(3).unwrap();
        assert_eq!(rep.data_shards, 2);
        assert_eq!(rep.parity_shards, 1);
    }

    #[test]
    fn test_encode_and_decode() {
        let rep = Replication::new(2, 1).unwrap();
        let data = b"hello world test data";

        let fragments = rep.encode(data).unwrap();
        assert_eq!(fragments.len(), 3); // 2 data + 1 parity

        let decoded = rep.decode(&fragments, data.len()).unwrap();
        assert_eq!(&decoded, data);
    }

    #[test]
    fn test_decode_with_missing_shard() {
        let rep = Replication::new(2, 1).unwrap();
        let data = b"test";

        let mut fragments = rep.encode(data).unwrap();
        fragments.remove(0); // Remove one shard

        let decoded = rep.decode(&fragments, data.len()).unwrap();
        assert_eq!(&decoded, data);
    }

    #[test]
    fn test_fault_tolerance() {
        let rep2 = Replication::new(2, 1).unwrap();
        assert_eq!(rep2.fault_tolerance(), 1); // Tolerate 1 failure

        let rep3 = Replication::new(3, 2).unwrap();
        assert_eq!(rep3.fault_tolerance(), 2); // Tolerate 2 failures
    }
}

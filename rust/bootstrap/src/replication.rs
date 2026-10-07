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

#[cfg(test)]
mod tests {
    use super::*;

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

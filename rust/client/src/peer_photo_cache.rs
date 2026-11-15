//! Bounded in-memory peer media, admitted only in the captured profile/network context.

use crate::contact_verification::{Context, Generation};
use std::collections::VecDeque;

// About 6 MiB of thumbnails and 4 MiB of full images at the existing media byte limits.
const THUMBNAILS: usize = 256;
const FULL_PHOTOS: usize = 8;
type Entry = (Vec<u8>, Vec<u8>);

fn touch(entries: &mut VecDeque<Entry>, hash: &[u8]) -> Option<Vec<u8>> {
    let at = entries.iter().position(|(held, _)| held == hash)?;
    let entry = entries.remove(at)?;
    let bytes = entry.1.clone();
    entries.push_back(entry);
    Some(bytes)
}

/// Hash-addressed media for one generation; never persists data or retains an identity lease.
/// Callers validate media hashes, encoding and byte/frame limits before admission.
#[derive(Default)]
pub struct Cache {
    generation: Option<Generation>,
    thumbnails: VecDeque<Entry>,
    full: VecDeque<Entry>,
}

impl Cache {
    fn synchronize(&mut self, context: &Context) {
        if self
            .generation
            .as_ref()
            .is_none_or(|held| !context.is_current(held))
        {
            self.thumbnails.clear();
            self.full.clear();
            self.generation = Some(context.generation());
        }
    }

    /// Read current-context bytes and forget media from a superseded context.
    /// The caller keeps the context's activation guard through this operation.
    #[must_use]
    pub fn get(&mut self, context: &Context, hash: &[u8]) -> Option<Vec<u8>> {
        self.synchronize(context);
        touch(&mut self.thumbnails, hash).or_else(|| touch(&mut self.full, hash))
    }

    /// Well-formed requested hashes not held in the current generation.
    #[must_use]
    pub fn missing(&mut self, context: &Context, hashes: &[Vec<u8>]) -> Vec<[u8; 32]> {
        hashes
            .iter()
            .filter(|hash| self.get(context, hash).is_none())
            .filter_map(|hash| hash.as_slice().try_into().ok())
            .collect()
    }

    /// Admit already-validated bytes only while the fetch's captured generation is current.
    /// A refused old/foreign result leaves any newer cached media untouched.
    /// The caller keeps the context's activation guard through this operation.
    #[must_use]
    pub fn put(
        &mut self,
        context: &Context,
        generation: &Generation,
        hash: Vec<u8>,
        bytes: Vec<u8>,
        full: bool,
    ) -> bool {
        if !context.is_current(generation) {
            return false;
        }
        self.synchronize(context);
        let (entries, limit) = if full {
            (&mut self.full, FULL_PHOTOS)
        } else {
            (&mut self.thumbnails, THUMBNAILS)
        };
        entries.retain(|(held, _)| *held != hash);
        if entries.len() == limit {
            entries.pop_front();
        }
        entries.push_back((hash, bytes));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(index: usize) -> Vec<u8> {
        u32::try_from(index).unwrap().to_be_bytes().to_vec()
    }

    #[test]
    fn the_cache_stays_within_its_limits_and_drops_the_least_recently_used() {
        let context = Context::default();
        let generation = context.generation();
        for (full, limit) in [(false, THUMBNAILS), (true, FULL_PHOTOS)] {
            let mut cache = Cache::default();
            for index in 0..limit {
                assert!(cache.put(&context, &generation, hash(index), hash(index), full));
            }
            assert_eq!(cache.get(&context, &hash(0)), Some(hash(0)));
            assert!(cache.put(&context, &generation, hash(limit), hash(limit), full));
            assert!(cache.put(
                &context,
                &generation,
                hash(limit + 1),
                hash(limit + 1),
                full
            ));
            assert_eq!(cache.thumbnails.len() + cache.full.len(), limit);
            assert_eq!(cache.get(&context, &hash(0)), Some(hash(0)));
            assert_eq!(cache.get(&context, &hash(1)), None);
            assert_eq!(cache.get(&context, &hash(2)), None);
        }
    }

    #[test]
    fn clearing_context_hides_both_media_kinds_and_allows_fresh_admission() {
        let mut context = Context::default();
        let old = context.generation();
        let mut cache = Cache::default();
        assert!(cache.put(&context, &old, hash(1), vec![2], false));
        assert!(cache.put(&context, &old, hash(3), vec![4], true));
        context.clear();
        assert_eq!(cache.get(&context, &hash(1)), None);
        assert_eq!(cache.get(&context, &hash(3)), None);
        assert!(cache.put(&context, &context.generation(), hash(5), vec![6], true));
        assert_eq!(cache.get(&context, &hash(5)), Some(vec![6]));
    }

    #[test]
    fn missing_hashes_ignore_malformed_requests_and_invalidate_cached_hits() {
        let mut context = Context::default();
        let mut cache = Cache::default();
        let held = vec![1; 32];
        let absent = vec![2; 32];
        assert!(cache.put(
            &context,
            &context.generation(),
            held.clone(),
            vec![3],
            false
        ));
        let hashes = [held, absent, vec![4]];
        assert_eq!(cache.missing(&context, &hashes), vec![[2; 32]]);
        context.clear();
        assert_eq!(cache.missing(&context, &hashes), vec![[1; 32], [2; 32]]);
    }

    #[test]
    fn stale_and_foreign_completions_preserve_newer_cached_bytes() {
        let mut context = Context::default();
        let old = context.generation();
        context.clear();
        context.clear();
        let current = context.generation();
        let mut cache = Cache::default();
        assert!(cache.put(&context, &current, hash(1), vec![2], false));
        for refused in [old, Context::default().generation()] {
            assert!(!cache.put(&context, &refused, hash(1), vec![9], false));
            assert_eq!(cache.get(&context, &hash(1)), Some(vec![2]));
        }
    }
}

//! Canonical places: GeoNames cities1000 IDs per country (CC BY 4.0, see NOTICE). A profile
//! place outside this set is indexed under its country only.
//!
//! The data is one block per country, sorted: the country code (2 bytes), the body length
//! (LEB128) and the body, the sorted IDs as LEB128 deltas (written by
//! `scripts/geonames-places.py`).

static PLACES: &[u8] = include_bytes!("../data/places.bin");

/// `place` when `(country, place)` is a known GeoNames ID, else `""` (the country bucket).
/// Only the canonical decimal text counts: `"+2950159"` and `"02950159"` do not.
pub fn canonical<'a>(country: &str, place: &'a str) -> &'a str {
    let known = place
        .parse::<u32>()
        .ok()
        .filter(|id| id.to_string() == place)
        .is_some_and(|id| {
            blocks()
                .find(|(c, _)| c.as_slice() == country.as_bytes())
                .is_some_and(|(_, body)| ids(body).find(|&i| i >= id) == Some(id))
        });
    if known {
        place
    } else {
        ""
    }
}

/// Every place of `country`, ascending.
#[cfg(test)]
pub(crate) fn of(country: &str) -> Vec<u32> {
    blocks()
        .find(|(c, _)| c.as_slice() == country.as_bytes())
        .map_or(vec![], |(_, body)| ids(body).collect())
}

/// Reads one LEB128 number off the front of `b`.
fn leb128(b: &mut &[u8]) -> Option<u32> {
    let mut n = 0u32;
    for shift in (0..35).step_by(7) {
        let (&byte, rest) = b.split_first()?;
        *b = rest;
        n |= u32::from(byte & 0x7f).checked_shl(shift)?;
        if byte & 0x80 == 0 {
            return Some(n);
        }
    }
    None
}

/// The (country, body) blocks in order.
fn blocks() -> impl Iterator<Item = ([u8; 2], &'static [u8])> {
    let mut data = PLACES;
    std::iter::from_fn(move || {
        let (country, mut rest) = data.split_first_chunk::<2>()?;
        let len = usize::try_from(leb128(&mut rest)?).ok()?;
        let (body, next) = rest.split_at_checked(len)?;
        data = next;
        Some((*country, body))
    })
}

/// The IDs of one block body, ascending.
fn ids(mut body: &[u8]) -> impl Iterator<Item = u32> + '_ {
    let mut id = 0u32;
    std::iter::from_fn(move || {
        id = id.checked_add(leb128(&mut body)?)?;
        Some(id)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_is_sorted_and_known() {
        let blocks: Vec<_> = blocks().collect();
        assert!(blocks.is_sorted_by(|a, b| a.0 < b.0));
        assert_eq!(
            blocks.last().map(|(_, body)| body.as_ptr_range().end),
            Some(PLACES.as_ptr_range().end),
            "trailing bytes"
        );
        let mut n = 0;
        for (_, body) in &blocks {
            let ids: Vec<_> = ids(body).collect();
            assert!(ids.is_sorted_by(|a, b| a < b));
            n += ids.len();
        }
        assert!((100_000..200_000).contains(&n), "{n} places");
        assert_eq!(canonical("DE", "2950159"), "2950159");
    }

    #[test]
    fn unknown_places_fall_back_to_the_country() {
        assert_eq!(canonical("FR", "2950159"), "");
        assert_eq!(canonical("DE", "4294967295"), "");
        assert_eq!(canonical("DE", "berlin"), "");
        assert_eq!(canonical("DE", "+2950159"), "");
        assert_eq!(canonical("DE", "02950159"), "");
        assert_eq!(canonical("", "2950159"), "");
        assert_eq!(canonical("DE", ""), "");
    }
}

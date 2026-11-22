//! Search-control prerequisites shared by native and desktop presentation adapters.
use dyapp_p2p_net::search_results::{SEEKING_AGE, SEEKING_GENDERS};

/// Offered fields whose search controls can be displayed with their prerequisites.
///
/// A selected place needs the country picker; mutual search needs both seeking fields.
/// Preserve all other offered fields, order and duplicates. This is presentation admission,
/// not query validation: independently offered location conditions retain their wire contract.
#[must_use]
pub fn visible_filter_fields(offered: impl AsRef<[u32]>) -> Vec<u32> {
    let offered = offered.as_ref();
    let country = offered.contains(&4);
    let mutual = offered.contains(&SEEKING_GENDERS) && offered.contains(&SEEKING_AGE);
    offered
        .iter()
        .copied()
        .filter(|field| match *field {
            20 => country,
            SEEKING_GENDERS | SEEKING_AGE => mutual,
            _ => true,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incomplete_compound_controls_are_hidden() {
        for fields in [vec![], vec![20], vec![SEEKING_GENDERS], vec![SEEKING_AGE]] {
            assert!(visible_filter_fields(fields).is_empty());
        }
        assert_eq!(visible_filter_fields([20, 4]), vec![20, 4]);
        assert_eq!(
            visible_filter_fields([SEEKING_AGE, SEEKING_GENDERS]),
            vec![SEEKING_AGE, SEEKING_GENDERS]
        );
    }

    #[test]
    fn ordinary_fields_and_wire_order_are_preserved() {
        assert_eq!(
            visible_filter_fields([1001, 46, 19, 4, 53, 20, 9999, 4]),
            vec![1001, 46, 19, 4, 20, 9999, 4]
        );
        let all = [54, 1001, 20, 4, 53, 46, 19, 20];
        assert_eq!(visible_filter_fields(all), all);
    }
}

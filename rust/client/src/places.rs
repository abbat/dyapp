//! Place names for the profile place picker: GeoNames cities15000 with English and Russian names
//! (CC BY 4.0, see NOTICE), a subset of the node's canonical places. Profile field `place` holds
//! the decimal [`Place::id`].
//!
//! `data/places.z` is zlib-compressed text written by `tools/scripts/geonames-names.py`; its
//! docstring describes the lines. It is inflated once, on first use, and scanned per call.

use std::sync::OnceLock;

static DATA: &[u8] = include_bytes!("../data/places.z");

/// One place in the UI language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    /// GeoNames ID.
    pub id: u32,
    /// Place name.
    pub name: String,
    /// First-level region (state, oblast), empty when GeoNames has none.
    pub region: String,
}

/// The place `id` of `country`, named in `lang` (`"ru"`, else English).
pub fn place(country: &str, id: u32, lang: &str) -> Option<Place> {
    rows()
        .find(|r| r.country == country && r.id == id)
        .map(|r| r.place(lang))
}

/// At most `limit` places of `country` whose English or Russian name starts with `prefix`,
/// ignoring case, largest first; an empty prefix lists the largest places.
pub fn search(country: &str, prefix: &str, lang: &str, limit: usize) -> Vec<Place> {
    let prefix = prefix.trim().to_lowercase();
    let mut found: Vec<_> = rows()
        .filter(|r| r.country == country)
        .filter(|r| {
            [r.en, r.ru]
                .iter()
                .any(|n| !n.is_empty() && n.to_lowercase().starts_with(&prefix))
        })
        .collect();
    found.sort_by_key(|r| std::cmp::Reverse(r.size));
    found.iter().take(limit).map(|r| r.place(lang)).collect()
}

/// The countries that have places, in code order.
pub fn countries() -> Vec<&'static str> {
    let mut found: Vec<_> = rows().map(|r| r.country).collect();
    found.dedup();
    found
}

/// Whether profile `country` and `place` are each empty or offered by the picker; a place needs
/// its country and is the canonical decimal ID.
pub fn known(country: &str, place: &str) -> bool {
    (country.is_empty() || countries().contains(&country))
        && (place.is_empty()
            || place.parse::<u32>().is_ok_and(|id| {
                id.to_string() == place && self::place(country, id, "en").is_some()
            }))
}

struct Row {
    country: &'static str,
    id: u32,
    size: u8,
    region: &'static str,
    en: &'static str,
    ru: &'static str,
}

impl Row {
    fn place(&self, lang: &str) -> Place {
        let region = regions()
            .lines()
            .find_map(|line| {
                let mut f = line.split('\t');
                let (country, code) = f.next()?.split_once('.')?;
                if country != self.country || code != self.region {
                    return None;
                }
                Some(named(f.next()?, f.next()?, lang))
            })
            .unwrap_or_default();
        Place {
            id: self.id,
            name: named(self.en, self.ru, lang).into(),
            region: region.into(),
        }
    }
}

fn named<'a>(en: &'a str, ru: &'a str, lang: &str) -> &'a str {
    if lang == "ru" && !ru.is_empty() {
        ru
    } else {
        en
    }
}

/// The inflated text; empty if the embedded data is broken, which the tests rule out.
fn text() -> &'static str {
    static TEXT: OnceLock<String> = OnceLock::new();
    TEXT.get_or_init(|| {
        miniz_oxide::inflate::decompress_to_vec_zlib(DATA)
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
            .unwrap_or_default()
    })
}

fn regions() -> &'static str {
    text().split_once("\n\n").map_or("", |(r, _)| r)
}

/// Every place, by country and ID.
fn rows() -> impl Iterator<Item = Row> {
    let places = text().split_once("\n\n").map_or("", |(_, p)| p);
    let mut last = ("", 0u32);
    places.lines().filter_map(move |line| {
        let mut f = line.split('\t');
        let country = f.next()?;
        let delta: u32 = f.next()?.parse().ok()?;
        let id = if country == last.0 {
            last.1.checked_add(delta)?
        } else {
            delta
        };
        last = (country, id);
        Some(Row {
            country,
            id,
            size: f.next()?.parse().ok()?,
            region: f.next()?,
            en: f.next()?,
            ru: f.next()?,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_line_is_a_place_in_id_order() {
        let lines = text().split_once("\n\n").map(|(_, p)| p.lines().count());
        let rows: Vec<_> = rows().collect();
        assert_eq!(Some(rows.len()), lines);
        assert!((30_000..40_000).contains(&rows.len()), "{}", rows.len());
        assert!(rows
            .windows(2)
            .all(|w| (w[0].country, w[0].id) < (w[1].country, w[1].id)));
        assert!(rows.iter().all(|r| !r.en.is_empty() && r.en != r.ru));
    }

    #[test]
    fn names_follow_the_language() {
        let berlin = place("DE", 2950159, "ru").expect("Berlin");
        assert_eq!(berlin.name, "Берлин");
        let name = |lang| place("DE", 2950159, lang).map(|p| p.name);
        assert_eq!(name("en").as_deref(), Some("Berlin"));
        assert_eq!(name("de").as_deref(), Some("Berlin"));
        // DE.16; other countries have a .16 region and DE has earlier ones.
        assert_eq!(berlin.region, "Берлин");
        let region = place("DE", 2950159, "en").map(|p| p.region);
        assert_eq!(region.as_deref(), Some("State of Berlin"));
        assert_eq!(place("FR", 2950159, "en"), None);
        assert_eq!(place("DE", 1, "en"), None);
    }

    #[test]
    fn search_matches_either_name_largest_first() {
        let first = |prefix, lang| search("RU", prefix, lang, 1).pop().map(|p| p.name);
        assert_eq!(first("мос", "ru").as_deref(), Some("Москва"));
        assert_eq!(first(" MOS", "ru").as_deref(), Some("Москва"));
        assert_eq!(first("Mos", "en").as_deref(), Some("Moscow"));
        assert_eq!(search("RU", "", "en", 5).len(), 5);
        assert_eq!(search("RU", "", "en", 1)[0].name, "Moscow");
        assert!(search("RU", "zzzz", "en", 5).is_empty());
        assert!(search("XX", "", "en", 5).is_empty());
    }

    #[test]
    fn countries_are_listed_once_in_order() {
        let all = countries();
        assert!(all.windows(2).all(|w| w[0] < w[1]));
        assert!(all.contains(&"DE") && all.contains(&"RU"));
        assert!((100..250).contains(&all.len()), "{}", all.len());
    }
}

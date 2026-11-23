//! Raw web-editor text is interpreted by the same core parsers as native input.
use dyapp_profile::{birth_input, editor_input};
use std::collections::HashMap;

#[derive(Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Input {
    birth_year: String,
    birth_month: String,
    birth_day: String,
    height: String,
    interests: String,
    seeking_genders: Vec<String>,
    seeking_age_from: String,
    seeking_age_to: String,
    not_looking: bool,
}

#[derive(Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Fields {
    birth_year: Option<u32>,
    birth_month: Option<u32>,
    birth_day: Option<u32>,
    height: Option<u32>,
    interests: Vec<String>,
    seeking_genders: Vec<u32>,
    seeking_age_from: Option<u32>,
    seeking_age_to: Option<u32>,
    not_looking: bool,
}

// Each map is already validated by its core parser, including unsigned range checks.
fn unsigned(values: &HashMap<String, i32>, key: &str) -> Option<u32> {
    values.get(key).and_then(|value| u32::try_from(*value).ok())
}

fn seeking(input: &Input) -> Option<editor_input::Seeking> {
    let normalized = editor_input::parse_seeking(
        &[],
        &input.seeking_age_from,
        &input.seeking_age_to,
        input.not_looking,
    )?;
    // Core decides whether hidden input matters before converting checkbox representation.
    if normalized.not_looking {
        return Some(normalized);
    }
    let genders = input
        .seeking_genders
        .iter()
        .map(|value| value.trim().parse().ok())
        .collect::<Option<Vec<u32>>>()?;
    editor_input::parse_seeking(
        &genders,
        &input.seeking_age_from,
        &input.seeking_age_to,
        false,
    )
}

pub(crate) fn parse(input: &Input) -> Result<Fields, &'static str> {
    let birth =
        birth_input::parse_birth_parts(&input.birth_year, &input.birth_month, &input.birth_day)
            .ok_or("invalid birth date")?;
    let height = editor_input::parse_height(&input.height).ok_or("invalidHeight")?;
    let interests = dyapp_client::profile_editor::parse_interest_lines(&input.interests)
        .map_err(|_| "invalidInterests")?;
    let seeking = seeking(input).ok_or("invalid profile seeking")?;
    Ok(Fields {
        birth_year: unsigned(&birth, "birth_year"),
        birth_month: unsigned(&birth, "birth_month"),
        birth_day: unsigned(&birth, "birth_day"),
        height: unsigned(&height, "height"),
        interests,
        seeking_genders: seeking.genders,
        seeking_age_from: seeking.age_from,
        seeking_age_to: seeking.age_to,
        not_looking: seeking.not_looking,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_raw_boundary_preserves_clear_partial_dates_and_canonical_tags() {
        assert_eq!(parse(&Input::default()).unwrap(), Fields::default());
        let input: Input = serde_json::from_value(serde_json::json!({
            "birthYear": "", "birthMonth": " 2 ", "birthDay": "29", "height": " 180 ",
            "interests": " Music, Art; Books\r\n\nE\u{0301}COLE\nmusic, art; books",
            "seekingGenders": [" 2 ", "1"], "seekingAgeFrom": "18", "seekingAgeTo": "120",
            "notLooking": false
        }))
        .unwrap();
        assert_eq!(
            parse(&input).unwrap(),
            Fields {
                birth_month: Some(2),
                birth_day: Some(29),
                height: Some(180),
                interests: vec![
                    "music, art; books".into(),
                    "école".into(),
                    "music, art; books".into()
                ],
                seeking_genders: vec![1, 2],
                seeking_age_from: Some(18),
                seeking_age_to: Some(120),
                ..Fields::default()
            }
        );
    }

    #[test]
    fn raw_checkbox_codes_preserve_closed_vocabulary_and_hidden_input_rules() {
        for genders in [
            vec!["1e0".into()],
            vec!["-1".into()],
            vec!["999".into()],
            vec!["1".into(), "1".into()],
        ] {
            assert_eq!(
                parse(&Input {
                    seeking_genders: genders.clone(),
                    ..Input::default()
                }),
                Err("invalid profile seeking")
            );
            assert_eq!(
                parse(&Input {
                    seeking_genders: genders,
                    seeking_age_from: "wrong".into(),
                    seeking_age_to: "-1".into(),
                    not_looking: true,
                    ..Input::default()
                })
                .unwrap(),
                Fields {
                    not_looking: true,
                    ..Fields::default()
                }
            );
        }
    }

    #[test]
    fn malformed_input_is_refused_instead_of_silently_becoming_an_empty_field() {
        for text in ["0", "-1", "1.5", "1e2", "4294967296", "wrong"] {
            assert_eq!(
                parse(&Input {
                    height: text.into(),
                    ..Input::default()
                }),
                Err("invalidHeight")
            );
        }
        assert_eq!(
            parse(&Input {
                birth_year: "2023".into(),
                birth_month: "2".into(),
                birth_day: "29".into(),
                ..Input::default()
            }),
            Err("invalid birth date")
        );
        assert_eq!(
            parse(&Input {
                interests: "tag\n".repeat(dyapp_profile::MAX_INTERESTS + 1),
                ..Input::default()
            }),
            Err("invalidInterests")
        );
        let invalid = || Input {
            seeking_genders: vec!["unknown".into()],
            seeking_age_from: "wrong".into(),
            seeking_age_to: "-1".into(),
            ..Input::default()
        };
        assert_eq!(parse(&invalid()), Err("invalid profile seeking"));
        assert_eq!(
            parse(&Input {
                not_looking: true,
                ..invalid()
            })
            .unwrap(),
            Fields {
                not_looking: true,
                ..Fields::default()
            }
        );
    }
}

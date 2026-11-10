use super::*;
use dyapp_profile::{Profile, VerifiedProfile};

#[test]
fn blank_text_and_any_mode_preserve_the_desktop_contract() {
    for mode in [EmployerMode::Any, EmployerMode::Same, EmployerMode::Exclude] {
        for text in ["", " \t\n", "\u{feff}", "\u{a0}"] {
            assert_eq!(employer_filter(mode, text), Employer::Any);
        }
    }
    assert_eq!(
        employer_filter(EmployerMode::Any, "Acme; Globex"),
        Employer::Any
    );
}

#[test]
fn same_keeps_one_literal_company_and_exclude_preserves_order_and_duplicates() {
    assert_eq!(
        employer_filter(EmployerMode::Same, " \tAcme ; ;Globex\u{feff} "),
        Employer::Same("Acme ; ;Globex".into())
    );
    assert_eq!(
        employer_filter(EmployerMode::Exclude, " ; Acme ; ; Globex ;Acme ;\u{feff}"),
        Employer::Exclude(vec!["Acme".into(), "Globex".into(), "Acme".into()])
    );
    assert_eq!(
        employer_filter(EmployerMode::Exclude, "; ;"),
        Employer::Exclude(Vec::new())
    );
}

#[test]
fn unicode_boundaries_match_linux_instead_of_platform_trim_variants() {
    assert_eq!(
        employer_filter(EmployerMode::Same, "\u{85}"),
        Employer::Same("\u{85}".into())
    );
    assert_eq!(
        employer_filter(EmployerMode::Same, "\u{1c}"),
        Employer::Same("\u{1c}".into())
    );
    assert_eq!(
        employer_filter(
            EmployerMode::Exclude,
            "\u{feff}Acme\u{a0}; \u{feff} ; Globex"
        ),
        Employer::Exclude(vec!["Acme".into(), "Globex".into()])
    );
}

fn profiles() -> Vec<VerifiedProfile> {
    [
        ("first", "acme corp"),
        ("second", "Globex"),
        ("unknown", ""),
    ]
    .into_iter()
    .map(|(peer, company)| VerifiedProfile {
        peer_id: peer.into(),
        profile: Profile {
            company: company.into(),
            ..Profile::default()
        },
    })
    .collect()
}

#[test]
fn parsed_filters_use_existing_local_matching_and_preserve_missing_company_rules() {
    for (mode, expected) in [
        (EmployerMode::Any, vec!["first", "second", "unknown"]),
        (EmployerMode::Same, vec!["first"]),
        (EmployerMode::Exclude, vec!["second", "unknown"]),
    ] {
        let mut values = profiles();
        employer_filter(mode, " ACME \t CORP ").retain(&mut values);
        let actual: Vec<_> = values.into_iter().map(|profile| profile.peer_id).collect();
        assert_eq!(actual, expected);
    }
}

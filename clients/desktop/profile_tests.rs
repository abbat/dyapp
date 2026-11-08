use super::*;

#[test]
fn a_link_code_is_told_apart_from_an_expired_one_and_from_other_text() {
    let code = Code {
        public: [1; 32],
        mailbox: [2; 32],
        expires_at: 100,
    };
    let text = format!(" {}\n", code.text());
    assert_eq!(link_code(&text, 99), Ok(code));
    assert_eq!(link_code(&text, 100), Err("expired".to_string()));
    assert_eq!(link_code("dyapp-contact:1:ab", 99), Err("bad".to_string()));
}

fn temp_dir() -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("dyapp-desktop-profile-{nanos}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn woman(year: Option<u32>) -> Draft {
    Draft {
        gender: dyapp_profile::Gender::Woman.code(),
        birth_year: year,
        birth_month: Some(2),
        ..Draft::default()
    }
}

#[test]
fn every_public_field_can_remain_unspecified() {
    let (dir, owner) = (temp_dir(), Identity::generate());
    let empty = save(&dir, &owner, Draft::default()).unwrap();
    assert_eq!(empty.draft, Draft::default());
    assert_eq!((empty.ages, empty.problem), (None, None));
    assert_eq!(load(&dir, &owner).unwrap(), empty);
    let cancel = AtomicBool::new(false);
    assert_eq!(
        solve(&cancel, &dir, &owner, false, 4),
        Err("blocked: notAttested".into())
    );
    assert_eq!(solve(&cancel, &dir, &owner, true, 4), Ok(()));
    assert_eq!(load(&dir, &owner).unwrap().draft, Draft::default());
}

#[test]
fn a_draft_survives_reopening_with_its_ages_and_problem() {
    let (dir, alice) = (temp_dir(), Identity::generate());
    let empty = load(&dir, &alice).unwrap();
    assert_eq!(empty.draft, Draft::default());
    assert_eq!(empty.problem, None);
    assert!(!dir.join("client.db").exists());
    let saved = save(&dir, &alice, woman(Some(1990))).unwrap();
    assert!(saved.ages.is_some_and(|(youngest, _)| youngest >= 30));
    assert_eq!((saved.published, saved.problem), (0, None));
    assert_eq!(load(&dir, &alice).unwrap(), saved);
    let yearless = save(&dir, &alice, woman(None)).unwrap();
    assert_eq!((yearless.ages, yearless.problem), (None, None));
    let child = save(&dir, &alice, woman(Some(2025))).unwrap();
    assert_eq!(child.problem, Some("underage"));
}

#[test]
fn a_public_name_is_trimmed_and_refused_with_hidden_symbols_or_too_long() {
    let (dir, alice) = (temp_dir(), Identity::generate());
    let named = |name: String| Draft {
        name,
        ..woman(None)
    };
    let saved = save(&dir, &alice, named("  Anna  ".into())).unwrap();
    assert_eq!(saved.draft.name, "Anna");
    assert_eq!(load(&dir, &alice).unwrap().draft.name, "Anna");
    for name in ["x".repeat(MAX_FIELD_CHARS + 1), "An\u{202E}na".into()] {
        let error = save(&dir, &alice, named(name));
        assert_eq!(error, Err("invalidName".into()));
    }
    assert_eq!(load(&dir, &alice).unwrap().draft.name, "Anna");
}

#[test]
fn interests_are_canonical_persisted_and_explicitly_cleared() {
    let (dir, owner) = (temp_dir(), Identity::generate());
    let draft = Draft {
        interests: vec!["  Музыка  ".into(), " E\u{301}COLE ".into()],
        ..Draft::default()
    };
    let saved = save(&dir, &owner, draft).unwrap();
    assert_eq!(saved.draft.interests, ["музыка", "école"]);
    assert_eq!(
        load(&dir, &owner).unwrap().draft.interests,
        saved.draft.interests
    );
    let mut published = stored(&open(&dir, &owner).unwrap()).unwrap();
    published.version = 1;
    assert!(dyapp_profile::verify(&published.sign(&owner)).is_ok());
    let refused = Draft {
        interests: vec!["  ".into()],
        ..saved.draft.clone()
    };
    assert_eq!(save(&dir, &owner, refused), Err("invalidInterests".into()));
    assert_eq!(
        load(&dir, &owner).unwrap().draft.interests,
        saved.draft.interests
    );
    save(&dir, &owner, Draft::default()).unwrap();
    assert_eq!(
        load(&dir, &owner).unwrap().draft.interests,
        Vec::<String>::new()
    );
}

#[test]
fn interest_limits_apply_after_normalization() {
    let longest = "x".repeat(MAX_FIELD_CHARS);
    assert!(canonical_interests(&vec![longest; dyapp_profile::MAX_INTERESTS]).is_ok());
    for tags in [
        vec!["x".into(); dyapp_profile::MAX_INTERESTS + 1],
        vec!["x".repeat(MAX_FIELD_CHARS + 1)],
        vec!["a\u{202e}b".into()],
        vec![String::new()],
    ] {
        assert_eq!(canonical_interests(&tags), Err("invalidInterests".into()));
    }
}

#[test]
fn height_is_saved_cleared_and_refused_out_of_range() {
    let (dir, alice) = (temp_dir(), Identity::generate());
    let tall = |height| Draft {
        height,
        ..woman(None)
    };
    save(&dir, &alice, tall(Some(180))).unwrap();
    assert_eq!(load(&dir, &alice).unwrap().draft.height, Some(180));
    for height in [49, 273] {
        let error = save(&dir, &alice, tall(Some(height)));
        assert_eq!(error, Err("invalidHeight".into()));
    }
    assert_eq!(load(&dir, &alice).unwrap().draft.height, Some(180));
    save(&dir, &alice, tall(None)).unwrap();
    assert_eq!(load(&dir, &alice).unwrap().draft.height, None);
}

#[test]
fn seeking_is_saved_cleared_refused_when_invalid_and_dropped_when_not_looking() {
    let (dir, alice) = (temp_dir(), Identity::generate());
    let seeks = Draft {
        seeking_genders: vec![dyapp_profile::Gender::Woman.code()],
        seeking_age_from: Some(30),
        seeking_age_to: Some(60),
        ..woman(None)
    };
    let saved = |draft| save(&dir, &alice, draft).err();
    assert_eq!(saved(seeks.clone()), None);
    assert_eq!(load(&dir, &alice).unwrap().draft, seeks);
    for bad in [
        Draft {
            seeking_genders: vec![999],
            ..seeks.clone()
        },
        Draft {
            seeking_genders: vec![0],
            ..seeks.clone()
        },
        Draft {
            seeking_age_from: Some(17),
            ..seeks.clone()
        },
        Draft {
            seeking_age_from: Some(61),
            ..seeks.clone()
        },
    ] {
        assert_eq!(saved(bad), Some("invalid profile seeking".into()));
    }
    assert_eq!(load(&dir, &alice).unwrap().draft, seeks);
    // The switch wins, even over bounds the form no longer checks while it is on.
    let paused = Draft {
        not_looking: true,
        seeking_age_from: Some(61),
        ..seeks.clone()
    };
    assert_eq!(saved(paused), None);
    assert_eq!(
        load(&dir, &alice).unwrap().draft,
        Draft {
            not_looking: true,
            ..woman(None)
        }
    );
    assert_eq!(saved(woman(None)), None);
    assert_eq!(load(&dir, &alice).unwrap().draft, woman(None));
}

#[test]
fn habits_and_about_are_saved_cleared_and_refused_when_invalid() {
    let (dir, alice) = (temp_dir(), Identity::generate());
    let filled = Draft {
        about: "  First line\nSecond line  ".into(),
        smoking: dyapp_profile::Smoking::Sometimes.code(),
        drinking: dyapp_profile::Drinking::No.code(),
        tattoos: dyapp_profile::Tattoos::Hidden.code(),
        marital_status: dyapp_profile::MaritalStatus::Divorced.code(),
        pets: dyapp_profile::Pets::Yes.code(),
        ..woman(None)
    };
    save(&dir, &alice, filled.clone()).unwrap();
    let saved = load(&dir, &alice).unwrap().draft;
    assert_eq!(saved.about, "First line\nSecond line");
    assert_eq!(
        saved,
        Draft {
            about: saved.about.clone(),
            ..filled.clone()
        }
    );
    for about in [
        "x".repeat(dyapp_profile::MAX_ABOUT_CHARS + 1),
        "An\u{202E}na".into(),
    ] {
        let refused = Draft {
            about,
            ..filled.clone()
        };
        assert_eq!(
            save(&dir, &alice, refused).err().as_deref(),
            Some("invalidAbout")
        );
    }
    let unknown = Draft {
        pets: dyapp_profile::Pets::MAX + 1,
        ..filled.clone()
    };
    assert!(save(&dir, &alice, unknown).is_err());
    save(&dir, &alice, woman(None)).unwrap();
    assert_eq!(load(&dir, &alice).unwrap().draft, woman(None));
}

#[test]
fn only_a_picked_place_is_saved_and_it_survives_reopening() {
    let (dir, alice) = (temp_dir(), Identity::generate());
    let at = |country: &str, place: &str| Draft {
        country: country.into(),
        place: place.into(),
        ..woman(None)
    };
    let berlin = save(&dir, &alice, at("DE", "2950159")).unwrap();
    assert_eq!(load(&dir, &alice).unwrap().draft, berlin.draft);
    assert_eq!(berlin.draft.place, "2950159");
    assert!(save(&dir, &alice, at("DE", "")).is_ok());
    for (country, place) in [
        ("XX", ""),
        ("de", ""),
        ("", "2950159"),
        ("FR", "2950159"),
        ("DE", "02950159"),
        ("DE", "Berlin"),
    ] {
        let error = save(&dir, &alice, at(country, place));
        assert_eq!(error, Err("invalid place".into()), "{country} {place}");
    }
    assert_eq!(load(&dir, &alice).unwrap().draft.country, "DE");
}

#[test]
fn the_puzzle_is_gated_resumes_after_cancel_and_edits_keep_it() {
    let (dir, alice) = (temp_dir(), Identity::generate());
    let cancel = AtomicBool::new(false);
    let run = |attested, bits| solve(&cancel, &dir, &alice, attested, bits);
    save(&dir, &alice, woman(None)).unwrap();
    assert_eq!(run(false, 4), Err("blocked: notAttested".into()));
    // 30 bits cannot be found in one step of eight nonces.
    cancel.store(true, Ordering::Relaxed);
    assert_eq!(run(true, 30), Err("cancelled".into()));
    let store = open(&dir, &alice).unwrap();
    let progress = stored(&store).unwrap();
    assert_eq!((progress.pow_nonce, progress.pow_bits), (STEP, 0));
    assert_eq!(run(true, 4), Ok(()));
    save(&dir, &alice, woman(Some(1990))).unwrap();
    let solved = stored(&store).unwrap();
    assert_eq!((solved.pow_bits, solved.birth_year), (4, Some(1990)));
}

#[tokio::test]
async fn the_view_says_when_a_held_profile_reaches_search() {
    let (dir, alice) = (temp_dir(), Identity::generate());
    assert_eq!(load(&dir, &alice).unwrap().search_minutes, None);
    save(&dir, &alice, woman(None)).unwrap();
    assert_eq!(load(&dir, &alice).unwrap().search_minutes, None);
    // Nothing is due, so nothing needs a network.
    assert!(send_search(&dir, &alice).await.is_ok());
    let store = open(&dir, &alice).unwrap();
    let signed = |version| {
        Profile {
            version,
            ..Profile::default()
        }
        .sign(&alice)
    };
    store.hold_search_profile(&signed(1)).unwrap();
    assert_eq!(load(&dir, &alice).unwrap().search_minutes, Some(0));
    assert_eq!(send_search(&dir, &alice).await, Err("notConnected".into()));
    store
        .search_profile_sent(&signed(1), unix_now().unwrap())
        .unwrap();
    assert_eq!(load(&dir, &alice).unwrap().search_minutes, None);
    store.hold_search_profile(&signed(2)).unwrap();
    assert_eq!(load(&dir, &alice).unwrap().search_minutes, Some(10));
}

#[tokio::test]
async fn publishing_needs_a_solved_puzzle_and_an_active_network() {
    let (dir, alice) = (temp_dir(), Identity::generate());
    let device = Identity::generate();
    save(&dir, &alice, woman(None)).unwrap();
    assert_eq!(
        publish(&dir, &dir, &alice, &device, true, 4).await,
        Err("unsolved".into())
    );
    solve(&AtomicBool::new(false), &dir, &alice, true, 4).unwrap();
    assert_eq!(
        publish(&dir, &dir, &alice, &device, false, 4).await,
        Err("blocked: notAttested".into())
    );
    assert_eq!(
        publish(&dir, &dir, &alice, &device, true, 4).await,
        Err("notConnected".into())
    );
}

#[test]
fn each_solve_has_a_distinct_cancellable_token() {
    let state = ProfileState::default();
    let first = state.begin().unwrap();
    let second = state.begin().unwrap();
    assert!(first.load(Ordering::Relaxed));
    assert!(!second.load(Ordering::Relaxed));
    state.cancel();
    assert!(second.load(Ordering::Relaxed));
    let third = state.begin().unwrap();
    assert!(!third.load(Ordering::Relaxed));
}

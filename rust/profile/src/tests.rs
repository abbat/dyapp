use super::*;

fn sample(version: u64) -> Profile {
    Profile {
        version,
        birth_year: Some(1992),
        country: "DE".into(),
        income_from: Some(50_000),
        income_to: Some(70_000),
        place: "Berlin".into(),
        interests: vec!["hiking".into()],
        ..Profile::default()
    }
}

#[test]
fn edited_profile_keeps_unknown_top_level_fields() {
    let mut original = sample(1).encode_to_vec();
    let unknown_varint = [0xa0, 0x06, 0x07]; // tag 100
    let unknown_bytes = [0xaa, 0x06, 0x02, 0xde, 0xad]; // tag 101
    original.extend_from_slice(&unknown_varint);
    original.extend_from_slice(&unknown_bytes);

    let mut edited = Profile::decode(original.as_slice()).unwrap();
    edited.birth_month = Some(5);
    let output = edited.encode_preserving(&original).unwrap();
    assert!(output
        .windows(unknown_varint.len())
        .any(|w| w == unknown_varint));
    assert!(output
        .windows(unknown_bytes.len())
        .any(|w| w == unknown_bytes));
    assert_eq!(
        Profile::decode(output.as_slice()).unwrap().birth_month,
        Some(5)
    );
    assert_eq!(
        sample(2)
            .encode_preserving(&sample(1).encode_to_vec())
            .unwrap(),
        sample(2).encode_to_vec()
    );
}

#[test]
fn edited_profile_keeps_untouched_photo_bytes() {
    let photo = Photo {
        full: vec![vec![1; 32]],
        thumbnail: vec![2; 32],
    };
    let mut nested = photo.encode_to_vec();
    nested.extend_from_slice(&[0xa0, 0x06, 0x07]); // unknown tag 100
    let mut original = sample(1).encode_to_vec();
    original.extend_from_slice(&[0xaa, 0x01, nested.len() as u8]); // tag 21
    original.extend_from_slice(&nested);

    let mut edited = Profile::decode(original.as_slice()).unwrap();
    edited.birth_month = Some(5);
    let output = edited.encode_preserving(&original).unwrap();
    assert!(output.windows(nested.len()).any(|w| w == nested));
    assert_eq!(
        Profile::decode(output.as_slice()).unwrap().birth_month,
        Some(5)
    );

    edited.photos = vec![Photo {
        thumbnail: vec![3; 32],
        ..photo
    }];
    let output = edited.encode_preserving(&original).unwrap();
    assert!(!output.windows(nested.len()).any(|w| w == nested));
    assert_eq!(
        Profile::decode(output.as_slice()).unwrap().photos,
        edited.photos
    );
}

#[test]
fn field_spans_cover_every_wire_type_and_reject_bad_keys() {
    let max = [0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01];
    let long_zero = [0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x00];
    let mut bytes = vec![0x08];
    bytes.extend_from_slice(&max);
    bytes.push(0x08);
    bytes.extend_from_slice(&long_zero);
    bytes.extend_from_slice(&[0x11, 1, 2, 3, 4, 5, 6, 7, 8]); // tag 2, fixed64
    bytes.extend_from_slice(&[0x1d, 1, 2, 3, 4]); // tag 3, fixed32
    assert_eq!(
        fields(&bytes),
        Ok(vec![(1, 0..11), (1, 11..22), (2, 22..31), (3, 31..36)])
    );

    let mut overflow = vec![0x08];
    overflow.extend_from_slice(&max[..9]);
    overflow.push(0x02);
    assert_eq!(fields(&overflow), Err(Error::Decode));
    assert_eq!(fields(&[0x00, 0x00]), Err(Error::Decode)); // tag 0
    let tag_too_big = [0x80, 0x80, 0x80, 0x80, 0x10, 0x00]; // tag 1 << 29
    assert_eq!(fields(&tag_too_big), Err(Error::Decode));
}

#[test]
fn malformed_original_profile_cannot_be_preserved() {
    assert_eq!(
        Profile::default().encode_preserving(&[0xff]),
        Err(Error::Decode)
    );
}

#[test]
fn device_directory_checks_owner_keys_order_and_signature() {
    let owner = Identity::generate();
    let first = Identity::generate();
    let second = Identity::generate();
    let mls_first = Identity::generate();
    let mls_second = Identity::generate();
    let certificate = |device: &Identity, mls: &Identity| {
        owner.sign(
            Domain::DeviceCertificate,
            DeviceCertificate {
                device_key: device.public_key().to_vec(),
                mls_signature_key: mls.public_key().to_vec(),
                generation: 1,
            }
            .encode_to_vec(),
        )
    };
    let mut devices = vec![
        certificate(&first, &mls_first),
        certificate(&second, &mls_second),
    ];
    devices.sort_by_key(|signed| {
        let cert = DeviceCertificate::decode(signed.payload.as_slice()).unwrap();
        key_hash(&cert.device_key.try_into().unwrap())
    });
    let mut profile = sample(1);
    profile.devices = devices;
    assert!(verify(&profile.sign(&owner)).is_ok());

    let binding = |device: &Identity, owner: &Identity| {
        device.sign(Domain::DeviceOwner, owner.public_key().to_vec())
    };
    profile.device_owners = vec![binding(&first, &owner), binding(&second, &owner)];
    assert!(verify(&profile.sign(&owner)).is_ok());
    for bad in [
        vec![binding(&Identity::generate(), &owner)],
        vec![binding(&first, &Identity::generate())],
        vec![first.sign(Domain::DeviceCertificate, owner.public_key().to_vec())],
        vec![binding(&first, &owner); 3],
    ] {
        profile.device_owners = bad;
        assert_eq!(
            verify(&profile.sign(&owner)),
            Err(Error::Invalid("invalid device owner binding"))
        );
    }
    profile.device_owners.clear();

    profile.devices.reverse();
    assert_eq!(
        verify(&profile.sign(&owner)),
        Err(Error::Invalid("duplicate or unsorted devices"))
    );
    profile.devices.reverse();
    profile.devices[1] = profile.devices[0].clone();
    assert_eq!(
        verify(&profile.sign(&owner)),
        Err(Error::Invalid("duplicate or unsorted devices"))
    );
    profile.devices[0].signature[0] ^= 1;
    assert_eq!(
        verify(&profile.sign(&owner)),
        Err(Error::Invalid("invalid device certificate signature"))
    );
    profile.devices[0] = Identity::generate().sign(
        Domain::DeviceCertificate,
        DeviceCertificate {
            device_key: first.public_key().to_vec(),
            mls_signature_key: mls_first.public_key().to_vec(),
            generation: 1,
        }
        .encode_to_vec(),
    );
    assert_eq!(
        verify(&profile.sign(&owner)),
        Err(Error::Invalid("invalid device certificate signature"))
    );
}

#[test]
fn content_rules_hold_at_their_boundaries() {
    let owner = Identity::generate();
    let invalid = |profile: &Profile, reason| {
        assert_eq!(
            verify(&profile.sign(&owner)),
            Err(Error::Invalid(reason)),
            "{reason}"
        );
    };
    let mut country = sample(1);
    for code in ["de", "DEU"] {
        country.country = code.into();
        invalid(&country, "country is not an ISO 3166-1 alpha-2 code");
    }
    let mut income = sample(1);
    income.income_to = income.income_from;
    assert!(verify(&income.sign(&owner)).is_ok());

    let certificate = |device: [u8; 32], mls: [u8; 32], generation| {
        owner.sign(
            Domain::DeviceCertificate,
            DeviceCertificate {
                device_key: device.to_vec(),
                mls_signature_key: mls.to_vec(),
                generation,
            }
            .encode_to_vec(),
        )
    };
    let mut keys: Vec<_> = (0..=MAX_DEVICES)
        .map(|_| {
            (
                Identity::generate().public_key(),
                Identity::generate().public_key(),
            )
        })
        .collect();
    keys.sort_by_key(|(device, _)| key_hash(device));
    let mut devices = sample(1);
    devices.devices = keys.iter().map(|&(d, m)| certificate(d, m, 1)).collect();
    invalid(&devices, "too many devices");
    devices.devices.pop();
    assert!(verify(&devices.sign(&owner)).is_ok());

    let (device, mls) = keys[0];
    let own = owner.public_key();
    for (device, mls, generation) in [
        (device, mls, 0),
        (device, device, 1),
        (own, mls, 1),
        (device, own, 1),
    ] {
        devices.devices = vec![certificate(device, mls, generation)];
        invalid(&devices, "invalid device key or generation");
    }

    let at_limit = owner.sign(Domain::Profile, vec![0xff; MAX_PAYLOAD_LEN]);
    assert_eq!(verify(&at_limit), Err(Error::Decode));
    let over = owner.sign(Domain::Profile, vec![0xff; MAX_PAYLOAD_LEN + 1]);
    assert_eq!(verify(&over), Err(Error::Invalid("profile too large")));
}

#[test]
fn highest_valid_version_wins() {
    let owner = Identity::generate();
    let v1 = sample(1).sign(&owner);
    let v2 = sample(2).sign(&owner);

    let accepted = accept(&v1, None).unwrap();
    assert_eq!(accepted.peer_id, owner.peer_id());
    assert_eq!(accepted.profile, sample(1));
    assert!(accept(&v2, Some(1)).is_ok());
    assert_eq!(accept(&v1, Some(2)), Err(Error::Stale));
    assert_eq!(accept(&v2, Some(2)), Err(Error::Stale));
    assert_eq!(stored_version(&v2), Ok(2));
}

#[test]
fn tombstone_supersedes_and_must_be_empty() {
    let owner = Identity::generate();
    let tombstone = Profile::tombstone(3, 0, 0).sign(&owner);
    assert!(accept(&tombstone, Some(2)).unwrap().profile.deleted);
    assert_eq!(accept(&sample(2).sign(&owner), Some(3)), Err(Error::Stale));

    let mut leaky = sample(4);
    leaky.deleted = true;
    assert_eq!(
        verify(&leaky.sign(&owner)),
        Err(Error::Invalid("tombstone carries data"))
    );
}

#[test]
fn required_pow_defaults_to_the_full_proof_until_a_test_lowers_it() {
    assert_eq!(required_pow_bits(), POW_BITS);
    set_required_pow_bits(4);
    assert_eq!(required_pow_bits(), 4);
    set_required_pow_bits(POW_BITS);
}

#[test]
fn heartbeat_pow_depends_on_the_day() {
    let key = [7; 32];
    let nonce = solve_heartbeat(&key, 20_000, 4, 0, 10_000).unwrap();
    assert!(heartbeat_pow_bits(&key, 20_000, nonce) >= 4);
    // Other days hash differently: at 4 bits one in 16 matches, so 4 days all matching is
    // a 1 in 65536 accident.
    assert!((1..=4).any(|d| heartbeat_pow_bits(&key, 20_000 + d, nonce) < 4));
}

#[test]
fn pow_is_pinned_and_solvable() {
    // Pinned format: change only with a protocol version bump. The first 6-bit nonce also
    // pins every hash before it.
    assert_eq!(pow_leading_bits(&[7; 32], 42), 1);
    assert_eq!(solve(&[7; 32], 6, 0, 10_000), Ok(154));
    let key = [9; 32];
    let nonce = solve(&key, 4, 0, 10_000).unwrap();
    assert!(pow_leading_bits(&key, nonce) >= 4);
    // `nonce` is the first match: a window ending before it fails and says where to resume.
    assert_eq!(solve(&key, 4, 0, nonce), Err(nonce));
    assert_eq!(solve(&key, 4, nonce, 1), Ok(nonce));
}

#[test]
fn tombstone_keeps_pow() {
    let owner = Identity::generate();
    let tombstone = Profile::tombstone(3, 77, 11).sign(&owner);
    let verified = verify(&tombstone).unwrap().profile;
    assert_eq!((verified.pow_nonce, verified.pow_bits), (77, 11));
}

#[test]
#[ignore = "benchmark: cargo test -p dyapp-profile --release -- --ignored --nocapture"]
fn pow_timing() {
    let start = std::time::Instant::now();
    for nonce in 0..20 {
        std::hint::black_box(pow_leading_bits(&[1; 32], nonce));
    }
    let ms = start.elapsed().as_secs_f64() * 1000.0 / 20.0;
    println!(
        "{ms:.1} ms per hash, ~{:.0} s for 11 bits",
        ms * 2048.0 / 1000.0
    );
}

#[test]
fn optional_career_fields_are_signed_and_validated() {
    let owner = Identity::generate();
    let mut profile = sample(1);
    assert!(profile.profession.is_empty());
    assert!(verify(&profile.sign(&owner)).is_ok());

    profile.profession = "Engineer".into();
    profile.company = "Example Co".into();
    profile.industry = "Technology".into();
    profile.career_stage = "Senior".into();
    profile.work_life_balance = "Flexible".into();
    assert_eq!(verify(&profile.sign(&owner)).unwrap().profile, profile);

    for invalid in ["x".repeat(MAX_FIELD_CHARS + 1), "bad\u{200B}".into()] {
        for field in 0..5 {
            let mut bad = profile.clone();
            *[
                &mut bad.profession,
                &mut bad.company,
                &mut bad.industry,
                &mut bad.career_stage,
                &mut bad.work_life_balance,
            ][field] = invalid.clone();
            assert!(verify(&bad.sign(&owner)).is_err());
        }
    }

    profile.deleted = true;
    assert_eq!(
        verify(&profile.sign(&owner)),
        Err(Error::Invalid("tombstone carries data"))
    );
}

#[test]
fn interests_require_canonical_tag() {
    let owner = Identity::generate();
    let mut profile = sample(1);
    assert_eq!(normalize_interest("  Музыка  "), "музыка");
    assert_eq!(normalize_interest("  E\u{301}COLE  "), "école");
    for input in ["Музыка", " музыка ", "e\u{301}cole"] {
        profile.interests = vec![input.into()];
        assert!(verify(&profile.sign(&owner)).is_err(), "{input:?}");
    }
    profile.interests = vec![normalize_interest("  Музыка  ")];
    assert!(verify(&profile.sign(&owner)).is_ok());
    profile.interests.clear();
    assert!(verify(&profile.sign(&owner)).is_ok());
}

#[test]
fn profile_strings_are_nfc_without_hidden_characters_and_bounded() {
    for text in [
        "Berlin",
        "Москва",
        "東京",
        "🙂",
        "👨\u{200D}👩\u{200D}👧",
        "می\u{200C}خواهم",
    ] {
        assert!(is_clean_text(text), "{text}");
    }
    for text in [
        "a\u{202E}b",
        "a\u{200B}b",
        "a\u{E000}b",
        "e\u{301}",
        "a\u{7}",
    ] {
        assert!(!is_clean_text(text), "{text:?}");
    }

    let owner = Identity::generate();
    let mut profile = sample(1);
    profile.place = "Mit\u{202E}te".into();
    assert!(verify(&profile.sign(&owner)).is_err());
    let mut profile = sample(1);
    profile.profession = "x".repeat(MAX_FIELD_CHARS);
    assert!(verify(&profile.sign(&owner)).is_ok());
    profile.profession.push('x');
    assert!(verify(&profile.sign(&owner)).is_err());
    profile.profession = "high\u{200B}".into();
    assert!(verify(&profile.sign(&owner)).is_err());
    let mut profile = sample(1);
    profile.interests = vec!["x".repeat(MAX_FIELD_CHARS); MAX_INTERESTS];
    assert!(verify(&profile.sign(&owner)).is_ok());
    profile.interests.push("chess".into());
    assert!(verify(&profile.sign(&owner)).is_err());
    profile.interests = vec!["x".repeat(MAX_FIELD_CHARS + 1)];
    assert!(verify(&profile.sign(&owner)).is_err());
    profile.interests = vec!["\u{E000}".into()];
    assert!(verify(&profile.sign(&owner)).is_err());

    let mut profile = sample(1);
    profile.name = "Анна 🙂".into();
    assert_eq!(verify(&profile.sign(&owner)).unwrap().profile, profile);
    profile.name = "x".repeat(MAX_FIELD_CHARS + 1);
    assert!(verify(&profile.sign(&owner)).is_err());
    profile.name = "Ann\u{202E}a".into();
    assert!(verify(&profile.sign(&owner)).is_err());
}

#[test]
fn verify_takes_height_only_inside_its_range() {
    let owner = Identity::generate();
    let with = |height| Profile {
        height,
        ..sample(1)
    };
    for height in [None, Some(50), Some(272)] {
        assert!(verify(&with(height).sign(&owner)).is_ok(), "{height:?}");
    }
    for height in [Some(0), Some(49), Some(273)] {
        assert!(verify(&with(height).sign(&owner)).is_err(), "{height:?}");
    }
}

#[test]
fn about_is_limited_to_500_code_points_and_may_hold_line_feeds() {
    let owner = Identity::generate();
    let mut profile = sample(1);
    for fits in ["я".repeat(MAX_ABOUT_CHARS), "Line\n\nline".into()] {
        profile.about = fits;
        assert!(verify(&profile.sign(&owner)).is_ok());
    }
    for refused in [
        "я".repeat(MAX_ABOUT_CHARS + 1),
        "Line\r\nline".into(),
        "a\u{202E}b".into(),
    ] {
        profile.about = refused;
        assert!(verify(&profile.sign(&owner)).is_err());
    }
}

#[test]
fn seeking_fields_are_optional_bounded_and_exclude_not_looking() {
    let owner = Identity::generate();
    let check = |change: &dyn Fn(&mut Profile)| {
        let mut profile = sample(1);
        change(&mut profile);
        verify(&profile.sign(&owner)).is_ok()
    };
    let codes = |n: u32| (1..=n).collect::<Vec<_>>();
    for fits in [
        (vec![], None, None),
        (vec![1, 3], Some(18), Some(120)),
        (vec![2], Some(30), Some(30)),
        (vec![2], Some(30), None),
        (vec![2], None, Some(30)),
        (vec![999], None, None),
        (codes(16), None, None),
    ] {
        assert!(check(&|p| {
            p.seeking_genders.clone_from(&fits.0);
            (p.seeking_age_from, p.seeking_age_to) = (fits.1, fits.2);
        }));
    }
    for refused in [
        (vec![0], None, None),
        (vec![2, 2], None, None),
        (codes(17), None, None),
        (vec![], Some(17), None),
        (vec![], None, Some(17)),
        (vec![], Some(121), None),
        (vec![], None, Some(121)),
        (vec![], Some(31), Some(30)),
    ] {
        assert!(!check(&|p| {
            p.seeking_genders.clone_from(&refused.0);
            (p.seeking_age_from, p.seeking_age_to) = (refused.1, refused.2);
        }));
    }
    assert!(check(&|p| p.not_looking = true));
    assert!(!check(&|p| {
        p.not_looking = true;
        p.seeking_genders = vec![1];
    }));
    assert!(!check(&|p| {
        p.not_looking = true;
        p.seeking_age_from = Some(18);
    }));
    assert!(!check(&|p| {
        p.not_looking = true;
        p.seeking_age_to = Some(18);
    }));
}

#[test]
fn verify_takes_scale_codes_from_a_newer_client() {
    fn field(profile: &mut Profile, tag: u32) -> &mut u32 {
        match tag {
            36 => &mut profile.gender,
            37 => &mut profile.orientation,
            38 => &mut profile.relationship_goal,
            39 => &mut profile.role_preference,
            40 => &mut profile.career_ambition,
            41 => &mut profile.financial_philosophy,
            42 => &mut profile.employment_status,
            43 => &mut profile.fitness_level,
            44 => &mut profile.zodiac,
            48 => &mut profile.smoking,
            49 => &mut profile.drinking,
            50 => &mut profile.tattoos,
            51 => &mut profile.marital_status,
            52 => &mut profile.pets,
            _ => unreachable!(),
        }
    }
    let owner = Identity::generate();
    let mut profile = sample(1);
    for scale in SCALES {
        *field(&mut profile, scale.field) = scale.max;
        assert!(verify(&profile.sign(&owner)).is_ok(), "{}", scale.key);
        *field(&mut profile, scale.field) = scale.max + 1;
        assert!(verify(&profile.sign(&owner)).is_ok(), "{}", scale.key);
        *field(&mut profile, scale.field) = 0;
        assert!(verify(&profile.sign(&owner)).is_ok(), "{}", scale.key);
    }
    profile.education_level = EducationLevel::MAX + 1;
    assert!(verify(&profile.sign(&owner)).is_ok());
    assert!(verify(&Profile::tombstone(2, 0, 0).sign(&owner)).is_ok());
}

#[test]
fn rejects_bad_signature_and_content() {
    let owner = Identity::generate();
    let mut forged = sample(1).sign(&owner);
    forged.public_key = Identity::generate().public_key().to_vec();
    assert!(matches!(verify(&forged), Err(Error::Signature(_))));

    let mut reversed = sample(1);
    reversed.income_from = Some(80_000);
    assert!(verify(&reversed.sign(&owner)).is_err());

    let mut country = sample(1);
    country.country = "Germany".into();
    assert!(verify(&country.sign(&owner)).is_err());

    let mut place = sample(1);
    place.place = "я".repeat(MAX_PLACE_CHARS);
    assert!(verify(&place.sign(&owner)).is_ok());
    place.place.push('я');
    assert!(verify(&place.sign(&owner)).is_err());
    place.place = "Mitte\n".into();
    assert!(verify(&place.sign(&owner)).is_err());

    assert!(verify(&sample(0).sign(&owner)).is_err());

    let mut gender = sample(1);
    gender.gender = Gender::Woman.code();
    let verified = verify(&gender.sign(&owner)).unwrap();
    assert_eq!(verified.profile.gender, Gender::Woman.code());

    let photo = Photo {
        full: vec![vec![1; 32], vec![2; 32]],
        thumbnail: vec![3; 32],
    };
    let mut photos = sample(1);
    photos.photos = vec![photo.clone(); MAX_PHOTOS];
    assert!(verify(&photos.sign(&owner)).is_ok());
    photos.photos.push(photo.clone());
    assert!(verify(&photos.sign(&owner)).is_err());
    for bad in [
        Photo {
            thumbnail: vec![],
            ..photo.clone()
        },
        Photo {
            full: vec![],
            ..photo.clone()
        },
        Photo {
            full: vec![vec![1; 31]],
            ..photo.clone()
        },
        Photo {
            full: vec![vec![1; 32]; MAX_PHOTO_BLOBS + 1],
            ..photo
        },
    ] {
        photos.photos = vec![bad];
        assert!(verify(&photos.sign(&owner)).is_err());
    }

    let garbage = owner.sign(Domain::Profile, vec![0xff; 4]);
    assert_eq!(verify(&garbage), Err(Error::Decode));
}

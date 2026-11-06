use super::*;

fn sample() -> Sticker {
    let (mut attachment, _) = crate::attachment::seal(b"not decoded by the payload codec").unwrap();
    attachment.release_secret = [0; 32];
    Sticker {
        id: [1; 16],
        hlc: 7,
        pack_id: [2; 32],
        sha256: attachment.plaintext_sha256,
        emoji: "😺".into(),
        pack_name: "Cats".into(),
        attachment: Some(attachment),
    }
}

fn wire() -> Wire {
    Wire::decode(sample().encode().unwrap().as_slice()).unwrap()
}

#[test]
fn descriptor_is_optional_and_hash_mismatch_keeps_fallback() {
    let mut sticker = sample();
    for attached in [true, false] {
        if !attached {
            sticker.attachment = None;
        }
        let decoded = Sticker::decode(&sticker.encode().unwrap()).unwrap();
        assert!(decoded == sticker);
        assert_eq!(decoded.download().is_some(), attached);
    }
    let mut mismatched = sample();
    mismatched.sha256 = [8; 32];
    let decoded = Sticker::decode(&mismatched.encode().unwrap()).unwrap();
    assert!(decoded.download().is_none());
    assert_eq!(
        (decoded.emoji.as_str(), decoded.pack_name.as_str()),
        ("😺", "Cats")
    );
    assert!(decoded.attachment.is_some());
}

#[test]
fn malformed_frames_and_conflicting_descriptor_context_are_refused() {
    for change in [
        |w: &mut Wire| {
            w.id.pop();
        },
        |w: &mut Wire| {
            w.pack_id.pop();
        },
        |w: &mut Wire| {
            w.sha256.push(0);
        },
        |w: &mut Wire| w.hlc = None,
        |w: &mut Wire| w.hlc = Some(u64::MAX),
        |w: &mut Wire| w.attachment.as_mut().unwrap().key.clear(),
        |w: &mut Wire| w.attachment.as_mut().unwrap().message_id = vec![9; 16],
        |w: &mut Wire| w.attachment.as_mut().unwrap().hlc = Some(8),
        |w: &mut Wire| w.attachment.as_mut().unwrap().size = 0,
        |w: &mut Wire| w.attachment.as_mut().unwrap().size = MAX_STICKER_BYTES as u64 + 1,
        |w: &mut Wire| w.attachment.as_mut().unwrap().release_secret = vec![1; 32],
        |w: &mut Wire| w.attachment.as_mut().unwrap().preview = vec![0],
        |w: &mut Wire| w.attachment.as_mut().unwrap().mime = "image/webp\n".into(),
    ] {
        let mut wire = wire();
        change(&mut wire);
        assert!(Sticker::decode(&wire.encode_to_vec()).is_err());
    }
    for bytes in [vec![], vec![0xff], vec![0; MAX_PAYLOAD_BYTES + 1]] {
        assert!(Sticker::decode(&bytes).is_err());
    }
    let original = sample();
    let mut unknown = original.encode().unwrap();
    unknown.extend_from_slice(&[0x80, 0x01, 0x01]); // Unknown field 16.
    assert!(Sticker::decode(&unknown).unwrap() == original);
}

#[test]
fn labels_follow_manifest_rules_at_encode_and_decode() {
    for (name, emoji, valid) in [
        ("Cats", "👨‍👩‍👧", true),
        ("Cats", "🇷🇺", true),
        ("Коты", "😺", true),
        ("", "😺", false),
        (" ", "😺", false),
        ("Cats\n", "😺", false),
        ("e\u{301}", "😺", false),
        ("Cats", "", false),
        ("Cats", "😺😺", false),
        ("Cats", "\u{202e}", false),
    ] {
        let mut sticker = sample();
        sticker.pack_name = name.into();
        sticker.emoji = emoji.into();
        assert_eq!(sticker.encode().is_ok(), valid, "{name:?} {emoji:?}");
        let mut wire = wire();
        wire.pack_name = name.into();
        wire.emoji = emoji.into();
        assert_eq!(Sticker::decode(&wire.encode_to_vec()).is_ok(), valid);
    }
    let mut sticker = sample();
    sticker.pack_name = "a".repeat(65);
    assert!(sticker.encode().is_err());
    let mut wire = wire();
    wire.pack_name = "a".repeat(65);
    assert!(Sticker::decode(&wire.encode_to_vec()).is_err());
}

#[test]
fn rendering_binds_validated_pixels_to_the_message_hash() {
    let lossy = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fuzz/corpus/sticker_webp/lossy.webp"
    ));
    let animated = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fuzz/corpus/sticker_webp/animated.webp"
    ));
    for bytes in [lossy.as_slice(), animated.as_slice()] {
        let mut sticker = sample();
        sticker.sha256 = dyapp_identity::sha256(bytes);
        assert!(sticker.download().is_none()); // Unrelated descriptor, valid saved-pack copy.
        let frame = sticker.first_frame(bytes).unwrap();
        assert_eq!(
            frame.pixels.len(),
            (frame.width * frame.height * 4) as usize
        );
        assert!(frame
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel[3] != 0));
        sticker.attachment = None;
        assert!(sticker.first_frame(bytes).is_ok());
        sticker.sha256[0] ^= 1;
        assert_eq!(
            sticker.first_frame(bytes).err().unwrap().to_string(),
            "attachment: sticker hash mismatch"
        );
    }
}

#[test]
fn matching_hash_does_not_authorize_invalid_images_or_metadata() {
    let mut transparent = Vec::new();
    image_webp::WebPEncoder::new(&mut transparent)
        .encode(&[0; 4], 1, 1, image_webp::ColorType::Rgba8)
        .unwrap();
    let mut sticker = sample();
    sticker.attachment = None;
    for bytes in [
        transparent,
        b"not WebP".to_vec(),
        vec![],
        vec![0; MAX_STICKER_BYTES + 1],
    ] {
        sticker.sha256 = dyapp_identity::sha256(&bytes);
        assert!(sticker.first_frame(&bytes).is_err());
    }
    let bytes = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fuzz/corpus/sticker_webp/lossy.webp"
    ));
    sticker.sha256 = dyapp_identity::sha256(bytes);
    sticker.emoji = "two labels".into();
    assert!(sticker.first_frame(bytes).is_err());
}

#[test]
fn normal_body_decoder_does_not_admit_payloads_before_projection() {
    let payload = sample().encode().unwrap();
    for tag in [14, 15, 16] {
        assert!(super::super::Body::decode(&[vec![tag], payload.clone()].concat()).is_err());
    }
}

use super::{
    apply_all, message, pad, unpad, AttachmentWire, Body, Delivery, Event, Receipt, TokenAnswer,
    TokenAsk, TokenRequest, MAX_RECEIPT_IDS, MAX_TEXT_BYTES, MAX_TOKENS, MIN_PADDED_BYTES,
};
use crate::attachment::{MAX_MIME_BYTES, MAX_PREVIEW_BYTES};
use crate::interactions::{Interaction, Kind};
use crate::Envelope;
use dyapp_identity::{Domain, Identity};
use dyapp_messaging::mls::Device;
use dyapp_p2p_net::proto;
use dyapp_profile::{Profile, VerifiedProfile};
use prost::Message as _;

fn person() -> (Identity, Device, VerifiedProfile) {
    let identity = Identity::generate();
    let device = Device::new(&identity, Identity::generate().public_key(), 1).unwrap();
    let profile = Profile {
        version: 1,
        devices: vec![device.certificate().clone()],
        ..Default::default()
    }
    .sign(&identity);
    (identity, device, dyapp_profile::verify(&profile).unwrap())
}

fn envelope(id: u8, ciphertext: Vec<u8>) -> Envelope {
    Envelope {
        id: vec![id],
        sender_key: Vec::new(),
        ciphertext,
    }
}

#[test]
fn applies_a_welcome_then_a_message_and_acks_both() {
    let (alice_identity, alice, alice_profile) = person();
    let (_, bob, bob_profile) = person();
    let profiles = [alice_profile, bob_profile];
    let package = bob.key_package(false).unwrap();
    let (mut group, welcome) = alice.create_group(&[package], &profiles).unwrap();
    let mut groups = Vec::new();
    let (events, acked) = apply_all(&bob, &mut groups, vec![envelope(1, welcome)], &profiles);
    assert!(events == [Event::Joined(group.id().to_vec())]);
    assert_eq!(acked, [vec![1]]);
    let sent = message(
        &mut group,
        &alice,
        &Body::text_with_id([1; 16], b"hi".to_vec()),
    )
    .unwrap();
    let (events, acked) = apply_all(
        &bob,
        &mut groups,
        vec![envelope(2, sent[0].mls.clone())],
        &profiles,
    );
    assert!(
        events
            == [Event::Message {
                group: group.id().to_vec(),
                sender: alice_identity.peer_id(),
                device: alice.device_key().unwrap(),
                body: Body::text_with_id([1; 16], b"hi".to_vec()),
            }]
    );
    assert_eq!(acked, [vec![2]]);

    // A forward arrives marked as forwarded, from the forwarder.
    let forward = Body::Forwarded(Box::new(Body::text_with_id(
        [1; 16],
        b"carol's words".to_vec(),
    )));
    let sent = message(&mut group, &alice, &forward).unwrap();
    let (events, _) = apply_all(
        &bob,
        &mut groups,
        vec![envelope(9, sent[0].mls.clone())],
        &profiles,
    );
    assert!(
        events
            == [Event::Message {
                group: group.id().to_vec(),
                sender: alice_identity.peer_id(),
                device: alice.device_key().unwrap(),
                body: forward,
            }]
    );

    let like = Interaction {
        kind: Kind::Like,
        event_id: [7; 16],
        hlc: 42,
    };
    let sent = message(&mut group, &alice, &Body::Interaction(like)).unwrap();
    // Padded from 256 bytes, not 1 KiB.
    assert!(
        sent[0].mls.len() < MIN_PADDED_BYTES,
        "{}",
        sent[0].mls.len()
    );
    let (events, acked) = apply_all(
        &bob,
        &mut groups,
        vec![envelope(3, sent[0].mls.clone())],
        &profiles,
    );
    let [Event::Interaction { device, event, .. }] = events.as_slice() else {
        panic!("expected decrypted interaction");
    };
    assert!(*event == like);
    assert_eq!(*device, alice.device_key().unwrap());
    assert_eq!(acked, [vec![3]]);

    // Presence and read receipts are signals too: short padding, delivered as message bodies.
    for (id, signal) in [
        (5, Body::Presence { hlc: 43 }),
        (6, Body::Read { up_to: [9; 16] }),
    ] {
        let sent = message(&mut group, &alice, &signal).unwrap();
        assert!(sent[0].mls.len() < MIN_PADDED_BYTES);
        let (events, _) = apply_all(
            &bob,
            &mut groups,
            vec![envelope(id, sent[0].mls.clone())],
            &profiles,
        );
        let [Event::Message { body, .. }] = events.as_slice() else {
            panic!("expected a decrypted signal");
        };
        assert!(*body == signal);
    }

    // An untagged plaintext, as before the frame, is refused and still acked.
    let raw = group.encrypt(&alice, &pad(b"hi")).unwrap();
    let (events, acked) = apply_all(&bob, &mut groups, vec![envelope(4, raw)], &profiles);
    assert!(events == [Event::Refused("bad reply: unknown message kind".into())]);
    assert_eq!(acked, [vec![4]]);
}

#[test]
fn body_frame_is_strict() {
    let like = Interaction {
        kind: Kind::Like,
        event_id: [7; 16],
        hlc: 42,
    };
    for body in [
        Body::text_with_id([1; 16], Vec::new()),
        Body::text_with_id([1; 16], b"hi".to_vec()),
        Body::Interaction(like),
        Body::Forwarded(Box::new(Body::text_with_id([1; 16], b"hi".to_vec()))),
    ] {
        assert!(Body::decode(&body.encode()).ok() == Some(body));
    }
    let tagged = Body::Interaction(like).encode();
    // Only a text or attachment is forwarded.
    let forwarded_like = [&[7], &tagged[..]].concat();
    for bad in [
        &b""[..],
        &[0],
        &[5],
        &[3],
        &[4],
        &[2, 1],
        &tagged[..tagged.len() - 1],
        &[7],
        &[7, 7, 1],
        &forwarded_like,
    ] {
        assert!(Body::decode(bad).is_err(), "{bad:?}");
    }
    // A text that starts like an interaction stays text.
    let text = [&[1u8][..], &[1; 16][..], &tagged[..]].concat();
    assert!(Body::decode(&text).ok() == Some(Body::text_with_id([1; 16], tagged)));
}

#[test]
fn message_operations_have_strict_frames() {
    let edit = Body::Edit {
        target: [1; 16],
        operation: [2; 16],
        hlc: 0x0102_0304_0506_0708,
        text: b"new text".to_vec(),
    };
    let delete = Body::Delete {
        target: [1; 16],
        operation: [3; 16],
        hlc: 42,
    };
    let presence = Body::Presence {
        hlc: 0x0102_0304_0506_0708,
    };
    let read = Body::Read { up_to: [6; 16] };
    for body in [
        Body::text_with_id([4; 16], vec![5; MAX_TEXT_BYTES]),
        edit.clone(),
        delete.clone(),
        presence.clone(),
        read.clone(),
    ] {
        assert!(Body::decode(&body.encode()).ok() == Some(body));
    }
    assert_eq!(presence.encode(), [10, 1, 2, 3, 4, 5, 6, 7, 8]);
    assert_eq!(read.encode(), [&[11], &[6; 16][..]].concat());
    assert_eq!(
        &edit.encode()[33..41],
        &0x0102_0304_0506_0708u64.to_be_bytes()
    );
    assert_eq!(delete.encode().len(), 41);
    for bad in [
        vec![1],
        vec![1; 16],
        Body::text_with_id([1; 16], vec![0; MAX_TEXT_BYTES + 1]).encode(),
        vec![8],
        vec![8; 40],
        Body::Edit {
            target: [1; 16],
            operation: [2; 16],
            hlc: 1,
            text: vec![0; MAX_TEXT_BYTES + 1],
        }
        .encode(),
        vec![9],
        vec![9; 40],
        [delete.encode(), vec![0]].concat(),
        [vec![7], delete.encode()].concat(),
        [
            vec![7],
            Body::Edit {
                target: [1; 16],
                operation: [2; 16],
                hlc: 1,
                text: vec![],
            }
            .encode(),
        ]
        .concat(),
        vec![10],
        vec![10; 8],
        vec![10; 10],
        vec![11],
        vec![11; 16],
        vec![11; 18],
        vec![12],
    ] {
        assert!(
            Body::decode(&bad).is_err(),
            "accepted bad body of {} bytes",
            bad.len()
        );
    }
}

#[test]
fn token_bodies_are_strict() {
    let ask = TokenAsk {
        mailbox: vec![1; 32],
        epoch: 20_000,
        token_key: vec![2; 32],
    };
    let request = |asks: Vec<TokenAsk>| Body::TokenRequest(TokenRequest { asks });
    let answer = |signed_tokens: Vec<Vec<u8>>| Body::TokenAnswer(TokenAnswer { signed_tokens });
    for body in [
        request(vec![ask.clone()]),
        request(vec![ask.clone(); MAX_TOKENS]),
        answer(vec![vec![9; 200]]),
        answer(vec![vec![9]; MAX_TOKENS]),
    ] {
        assert!(Body::decode(&body.encode()).ok() == Some(body));
    }
    let short = |mailbox: usize, token_key: usize| TokenAsk {
        mailbox: vec![1; mailbox],
        token_key: vec![2; token_key],
        ..ask.clone()
    };
    for bad in [
        request(Vec::new()),
        request(vec![ask.clone(); MAX_TOKENS + 1]),
        request(vec![ask.clone(), short(31, 32)]),
        request(vec![short(32, 33)]),
        answer(Vec::new()),
        answer(vec![vec![9]; MAX_TOKENS + 1]),
        answer(vec![vec![9], Vec::new()]),
    ] {
        assert!(Body::decode(&bad.encode()).is_err());
    }
    assert!(Body::decode(&[3, 0xff]).is_err());
}

#[test]
fn receipts_are_strict() {
    let receipt = |mailbox: usize, ids: Vec<Vec<u8>>| {
        Body::Receipt(Receipt {
            mailbox: vec![1; mailbox],
            ids,
        })
    };
    for body in [
        receipt(32, vec![vec![2; 16]]),
        receipt(32, vec![vec![2; 16]; MAX_RECEIPT_IDS]),
    ] {
        assert!(Body::decode(&body.encode()).ok() == Some(body));
    }
    for bad in [
        receipt(31, vec![vec![2; 16]]),
        receipt(32, Vec::new()),
        receipt(32, vec![vec![2; 16]; MAX_RECEIPT_IDS + 1]),
        receipt(32, vec![vec![2; 16], vec![2; 15]]),
    ] {
        assert!(Body::decode(&bad.encode()).is_err());
    }
}

#[test]
fn attachments_round_trip_and_refuse_wrong_lengths() {
    let (mut attachment, _) = crate::attachment::seal(b"photo").unwrap();
    attachment.preview = vec![0xff; MAX_PREVIEW_BYTES];
    attachment.mime = "image/jpeg".into();
    (attachment.width, attachment.height) = (4000, 3000);
    let body = Body::Attachment(attachment);
    assert!(Body::decode(&body.encode()).ok() == Some(body));
    let wire = |change: fn(&mut AttachmentWire)| {
        let mut wire = AttachmentWire {
            key: vec![1; 32],
            plaintext_sha256: vec![2; 32],
            blob_sha256: vec![3; 32],
            release_secret: vec![4; 32],
            size: 5,
            preview: Vec::new(),
            mime: String::new(),
            width: 0,
            height: 0,
        };
        change(&mut wire);
        Body::decode(&[&[6], wire.encode_to_vec().as_slice()].concat())
    };
    assert!(wire(|_| {}).is_ok());
    assert!(wire(|wire| wire.key.pop().map(drop).unwrap()).is_err());
    assert!(wire(|wire| wire.preview = vec![0; MAX_PREVIEW_BYTES + 1]).is_err());
    assert!(wire(|wire| wire.mime = "a".repeat(MAX_MIME_BYTES + 1)).is_err());
    assert!(wire(|wire| wire.mime = "image/jpeg\n".into()).is_err());
    assert!(Body::decode(&[6, 0xff]).is_err());
}

#[test]
fn debug_shows_no_text_sender_or_mailbox() {
    let event = Event::Message {
        group: vec![1],
        sender: "peer-of-bob".into(),
        device: vec![2; 32],
        body: Body::text_with_id([1; 16], b"secret text".to_vec()),
    };
    assert_eq!(format!("{event:?}"), "Message { group: [1], .. }");
    let delivery = Delivery {
        mailbox: vec![0xab; 32],
        mls: vec![0; 3],
    };
    assert_eq!(format!("{delivery:?}"), "Delivery { mls_len: 3, .. }");
}

#[test]
fn padding_hides_length_up_to_a_power_of_two() {
    for (length, size) in [
        (0, 1024),
        (200, 1024),
        (1020, 1024),
        (1021, 2048),
        (1500, 2048),
        (5000, 8192),
    ] {
        let plaintext = vec![7u8; length];
        let padded = pad(&plaintext);
        assert_eq!(padded.len(), size, "{length}");
        assert_eq!(unpad(&padded), Some(plaintext.as_slice()));
    }
    assert_eq!(unpad(&[0, 0, 4, 1, 0]), None);
    assert_eq!(unpad(&[0, 0]), None);
    assert!(pad(b"").len() == MIN_PADDED_BYTES);
}

#[test]
fn short_signal_padding_fits_the_signed_payload_limit() {
    let (alice_id, alice, alice_profile) = person();
    let (bob_id, bob, bob_profile) = person();
    let profiles = [alice_profile, bob_profile];
    let package = bob.key_package(false).unwrap();
    let (mut group, _) = alice.create_group(&[package], &profiles).unwrap();
    let token = bob_id.sign(
        Domain::WriteToken,
        proto::WriteToken {
            mailbox: vec![0; 32],
            token_key: alice_id.public_key().to_vec(),
            epoch: u32::MAX,
        }
        .encode_to_vec(),
    );
    let payload_size = |group: &mut dyapp_messaging::mls::Group, plaintext_len: usize, floor| {
        let size = (4 + plaintext_len).max(floor).next_power_of_two();
        let mut padded = vec![0; size];
        padded[..4].copy_from_slice(&(plaintext_len as u32).to_be_bytes());
        let ciphertext = group.encrypt(&alice, &padded).unwrap();
        proto::Envelope {
            id: vec![0; 16],
            mailbox: vec![0; 32],
            ciphertext,
            class: proto::EnvelopeClass::Signal.into(),
            write_token: Some(token.clone()),
        }
        .encoded_len()
    };
    let like = payload_size(&mut group, 16, 256);
    // 508 bytes plus the length prefix fills the 512-byte padding bucket.
    let introduction = payload_size(&mut group, 508, 256);
    let ordinary_padding = payload_size(&mut group, 16, MIN_PADDED_BYTES);
    assert!(like <= 1024, "like signed payload: {like}");
    assert!(
        introduction <= 1024,
        "introduction signed payload: {introduction}"
    );
    assert!(
        ordinary_padding > 1024,
        "ordinary padding: {ordinary_padding}"
    );
}

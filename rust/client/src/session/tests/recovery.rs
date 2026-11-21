use super::{bob_invited, key, temp_path, Invite, Session};
use crate::chat::{self, Body, Event};
use dyapp_identity::Identity;
use dyapp_messaging::mls::Device;

fn too_distant_text() -> (
    Session,
    Identity,
    crate::Envelope,
    [dyapp_profile::VerifiedProfile; 2],
    dyapp_messaging::mls::Group,
    Device,
) {
    let seal_key = [7; 32];
    let Invite {
        mut session,
        bob_id,
        alice,
        mut group,
        welcome,
        profiles,
        ..
    } = bob_invited(&temp_path(), &seal_key);
    session
        .stage_envelope(&key(1), &welcome, &profiles, &seal_key)
        .unwrap();
    let texts: Vec<_> = (0..25u8)
        .map(|i| {
            let body = Body::text_with_id([i + 1; 16], vec![b'a' + i]);
            chat::message(&mut group, &alice, &body).unwrap().remove(0)
        })
        .collect();
    assert!(matches!(
        session.stage_envelope(&key(2), &texts[0].mls, &[], &seal_key),
        Ok(Some(Event::UnknownOwner(_)))
    ));
    // Later texts of an unanswered request may go unshown; they still advance the ratchet.
    for (i, text) in (0u8..).zip(&texts).skip(20) {
        session
            .stage_envelope(&key(i + 2), &text.mls, &profiles, &seal_key)
            .unwrap();
    }
    let envelope = crate::Envelope {
        id: key(2).id.to_vec(),
        ciphertext: texts[0].mls.clone(),
        sender_key: Vec::new(),
    };
    (session, bob_id, envelope, profiles, group, alice)
}

#[test]
fn a_text_opened_after_many_later_ones_is_refused() {
    let (mut session, _, envelope, profiles, ..) = too_distant_text();
    assert!(matches!(
        session.stage_envelope(&key(2), &envelope.ciphertext, &profiles, &[7; 32]),
        Ok(Some(Event::ResendRequired))
    ));
}

#[tokio::test]
async fn a_too_distant_text_produces_a_negative_receipt() {
    let (mut session, bob_id, envelope, profiles, group, alice) = too_distant_text();
    assert!(matches!(
        session.stage_envelope(&key(2), &envelope.ciphertext, &profiles, &[7; 32]),
        Ok(Some(Event::ResendRequired))
    ));
    session
        .send_refusal(&mut [], &bob_id, &envelope, &[7; 32])
        .await
        .unwrap();
    let writes = session.due_outbound(0).unwrap();
    assert_eq!(writes.len(), 1);
    let negative = writes[0].envelope();
    let mut alice_groups = vec![group];
    let event = chat::apply(&alice, &mut alice_groups, &negative.ciphertext, &profiles)
        .unwrap()
        .unwrap();
    let Event::Message {
        body: Body::Receipt(receipt),
        ..
    } = event
    else {
        panic!("expected a receipt");
    };
    assert_eq!(receipt.refused, std::slice::from_ref(&envelope.id));
    assert_eq!(receipt.mailbox, bob_id.public_key().to_vec());
    let mut seen = Vec::new();
    assert!(session
        .duplicate_envelope(&mut [], &bob_id, &envelope, &[7; 32], &key(2), &mut seen,)
        .await
        .unwrap());
    assert_eq!(seen, [key(2)]);
    assert_eq!(session.due_outbound(0).unwrap().len(), 2);
}

async fn record(
    session: &mut Session,
    group: &[u8],
    sender: &str,
    device: &[u8],
    receipt: &chat::Receipt,
    seal_key: &[u8; 32],
) -> Result<(), crate::session::Error> {
    session
        .record_receipt(&mut [], group, sender, device, receipt, seal_key)
        .await
}

async fn recover(
    session: &mut Session,
    group: &[u8],
    sender: &str,
    device: &[u8],
    receipt: &chat::Receipt,
    seal_key: &[u8; 32],
    old: &crate::PreparedEnvelope,
) -> crate::PreparedEnvelope {
    record(session, group, sender, device, receipt, seal_key)
        .await
        .unwrap_err();
    let recovered = session.due_outbound(0).unwrap().remove(0);
    assert_ne!(old.id(), recovered.id());
    assert_ne!(old.as_bytes(), recovered.as_bytes());
    recovered
}

fn assert_recovered_text(
    alice: &Device,
    group: dyapp_messaging::mls::Group,
    profiles: &[dyapp_profile::VerifiedProfile],
    recovered: &crate::PreparedEnvelope,
) {
    let mut groups = vec![group];
    let event = chat::apply(
        alice,
        &mut groups,
        &recovered.envelope().ciphertext,
        profiles,
    )
    .unwrap()
    .unwrap();
    assert!(matches!(
        event,
        Event::Message {
            body: Body::Text { text, .. },
            ..
        } if text == b"recover me"
    ));
}

struct QueuedText {
    session: Session,
    alice_id: Identity,
    alice: Device,
    group: dyapp_messaging::mls::Group,
    group_id: Vec<u8>,
    profiles: [dyapp_profile::VerifiedProfile; 2],
    old: crate::PreparedEnvelope,
}

async fn queued_text() -> QueuedText {
    let seal_key = [7; 32];
    let Invite {
        mut session,
        alice_id,
        alice,
        group,
        welcome,
        profiles,
        ..
    } = bob_invited(&temp_path(), &seal_key);
    session
        .stage_envelope(&key(1), &welcome, &profiles, &seal_key)
        .unwrap();
    let group_id = group.id().to_vec();
    session
        .send_message(&mut [], &group_id, b"recover me", &seal_key)
        .await
        .unwrap_err();
    let old = session.due_outbound(0).unwrap().remove(0);
    QueuedText {
        session,
        alice_id,
        alice,
        group,
        group_id,
        profiles,
        old,
    }
}

#[tokio::test]
async fn a_negative_receipt_replaces_the_write_with_fresh_current_text() {
    let seal_key = [7; 32];
    let QueuedText {
        mut session,
        alice_id,
        alice,
        group,
        group_id,
        profiles,
        old,
    } = queued_text().await;
    let alice_device: [u8; 32] = alice.device_key().unwrap().try_into().unwrap();
    let receipt = chat::Receipt {
        mailbox: crate::mailbox(&alice_device).to_vec(),
        ids: Vec::new(),
        refused: vec![old.id().to_vec()],
    };
    let wrong_device = session.device.as_ref().unwrap().device_key().unwrap();
    record(
        &mut session,
        &group_id,
        &alice_id.peer_id(),
        &wrong_device,
        &receipt,
        &seal_key,
    )
    .await
    .unwrap();
    assert_eq!(session.due_outbound(0).unwrap()[0].id(), old.id());
    let recovered = recover(
        &mut session,
        &group_id,
        &alice_id.peer_id(),
        &alice_device,
        &receipt,
        &seal_key,
        &old,
    )
    .await;
    assert_recovered_text(&alice, group, &profiles, &recovered);
}

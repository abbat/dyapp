//! Sending with a held write token, and the fallbacks when the node refuses it.
use super::chat::{store_path, Person};
use super::*;
use dyapp_client::store::Store;
use dyapp_client::tokens;

const SEAL: [u8; 32] = [3; 32];

/// Makes `contact` hold a token `signer` signs for `mailbox` on `epoch`; returns its key.
fn hold(contact: &mut Store, signer: &Identity, mailbox: &[u8; 32], epoch: u32) -> [u8; 32] {
    let key = contact.token_key(mailbox, epoch, &SEAL).unwrap();
    let token = proto::WriteToken {
        mailbox: mailbox.to_vec(),
        token_key: key.to_vec(),
        epoch,
    };
    let signed = signer.sign(Domain::WriteToken, token.encode_to_vec());
    contact
        .set_write_token(mailbox, epoch, &signed.encode_to_vec())
        .unwrap();
    key
}

#[tokio::test]
async fn a_held_token_is_used_and_a_refused_one_falls_back() {
    let address = start_config(|config| {
        config.roles = vec![Role::Store];
        config.limits.requests_per_second = 1000;
        config.limits.min_free_mb = 0;
    })
    .await;
    let mut owner = Person::new(&address).await;
    let mut contact = Person::new(&address).await;
    let mut store = Store::open(&store_path("contact"), &contact.identity).unwrap();
    let mailbox = owner.mailbox();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let today = (now / 86_400) as u32;
    // A put's sender key: the token key, or a one-off key for a tokenless write.
    let mut sent = async |store: &Store, day: u32| {
        let epoch = tokens::send(&mut contact.node, store, &mailbox, &[1], day, &SEAL)
            .await
            .unwrap();
        let (envelopes, _) = owner.node.fetch(&owner.device_key).await.unwrap();
        let ids = envelopes
            .iter()
            .map(|envelope| envelope.id.clone())
            .collect();
        owner.node.ack(&owner.device_key, ids).await.unwrap();
        assert_eq!(envelopes.len(), 1);
        (epoch, envelopes[0].sender_key.clone())
    };

    // No token held: tokenless.
    assert_eq!(sent(&store, today).await.0, None);

    // Today's token from the owner device is taken.
    let key = hold(&mut store, &owner.device_key, &mailbox, today);
    assert_eq!(sent(&store, today).await, (Some(today), key.to_vec()));

    // A token signed by someone else is DENIED and the write goes tokenless.
    let forged = hold(&mut store, &Identity::generate(), &mailbox, today + 3);
    let (epoch, sender) = sent(&store, today + 3).await;
    assert_eq!(epoch, None);
    assert_ne!(sender, forged.to_vec());

    // Days ahead of the node are STALE, the next one too: tokenless.
    for day in [today + 5, today + 6] {
        hold(&mut store, &owner.device_key, &mailbox, day);
    }
    assert_eq!(sent(&store, today + 5).await.0, None);

    // A device a day behind is STALE once the node's day is an hour old, then takes the next day.
    hold(&mut store, &owner.device_key, &mailbox, today - 1);
    let expected = if now % 86_400 < 3600 {
        today - 1
    } else {
        today
    };
    assert_eq!(sent(&store, today - 1).await.0, Some(expected));
}

#[tokio::test]
async fn a_revoked_contact_token_is_denied_and_the_write_goes_tokenless() {
    let address = start_config(|config| {
        config.roles = vec![Role::Store];
        config.limits.requests_per_second = 1000;
        config.limits.min_free_mb = 0;
    })
    .await;
    let mut owner = Person::new(&address).await;
    let mut contact = Person::new(&address).await;
    let owner_store = Store::open(&store_path("revoking-owner"), &owner.identity).unwrap();
    let mut store = Store::open(&store_path("revoked-contact"), &contact.identity).unwrap();
    let mailbox = owner.mailbox();
    let today = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        / 86_400) as u32;
    let key = hold(&mut store, &owner.device_key, &mailbox, today);
    owner_store
        .record_issued("contact", &mailbox, &key, today)
        .unwrap();
    assert_eq!(
        put(&mut owner, &mut contact, &store, today).await,
        Some(today)
    );

    let revoked = tokens::revoke(
        &mut owner.node,
        &owner_store,
        &owner.device_key,
        "contact",
        today,
    )
    .await
    .unwrap();
    assert_eq!(revoked, 1);
    assert!(owner_store.issued_revoked(&key).unwrap());
    assert_eq!(put(&mut owner, &mut contact, &store, today).await, None);
}

/// Sends one byte from `contact` to `owner` with `tokens::send` and fetches it; returns the day
/// of the token the node took.
async fn put(owner: &mut Person, contact: &mut Person, store: &Store, today: u32) -> Option<u32> {
    let mailbox = owner.mailbox();
    let epoch = tokens::send(&mut contact.node, store, &mailbox, &[1], today, &SEAL)
        .await
        .unwrap();
    let (envelopes, _) = owner.node.fetch(&owner.device_key).await.unwrap();
    assert_eq!(envelopes.len(), 1);
    let ids = envelopes.iter().map(|e| e.id.clone()).collect();
    owner.node.ack(&owner.device_key, ids).await.unwrap();
    epoch
}

//! Mailbox write tokens carried in MLS chats. A contact device asks an owner device for a token
//! per day ahead with its own public token keys ([`TokenRequest`]); the owner device signs a
//! `WriteToken` for each with its mailbox key and answers ([`TokenAnswer`]).

use crate::chat::{Body, TokenAnswer, TokenAsk, TokenRequest, MAX_TOKENS};
use crate::store::{Error, Store};
use crate::{session, Node, PreparedEnvelope};
use dyapp_identity::{Domain, Identity, SignedRecord};
use dyapp_p2p_net::proto::{self, Status};
use prost::Message as _;

/// How many days after today a contact holds tokens for.
pub const AHEAD_DAYS: u32 = 30;
/// A contact asks again once it holds fewer days than this for a mailbox.
pub const REFILL_BELOW: usize = 15;
// With the tag and length prefix this pads to 64 KiB, so the envelope stays under the node's
// 100 KiB limit less the 256-byte token reserve; the next padding bucket, 128 KiB, would not.
const ANSWER_BYTES: usize = 64 * 1024 - 1 - 4;

/// The request a contact device sends to a group after joining it, and again later: for each
/// owner device mailbox holding fewer than [`REFILL_BELOW`] days from `today`, an ask with a fresh
/// or earlier token key for every missing day of the next [`AHEAD_DAYS`]. `None` when nothing is
/// missing. Asks past [`MAX_TOKENS`] wait for the next call.
pub fn request(
    store: &mut Store,
    mailboxes: &[[u8; 32]],
    today: u32,
    seal_key: &[u8; 32],
) -> Result<Option<Body>, Error> {
    let mut asks = Vec::new();
    for mailbox in mailboxes {
        let held = store.write_token_epochs(mailbox, today)?;
        if held.len() >= REFILL_BELOW {
            continue;
        }
        for epoch in today..today.saturating_add(AHEAD_DAYS) {
            if asks.len() < MAX_TOKENS && !held.contains(&epoch) {
                asks.push(TokenAsk {
                    mailbox: mailbox.to_vec(),
                    epoch,
                    token_key: store.token_key(mailbox, epoch, seal_key)?.to_vec(),
                });
            }
        }
    }
    Ok((!asks.is_empty()).then_some(Body::TokenRequest(TokenRequest { asks })))
}

/// Stores each token in `answer` signed by the owner device of its mailbox for a key this device
/// asked with; returns how many it stored. A token that fails a check is skipped.
pub fn accept(store: &Store, answer: &TokenAnswer, seal_key: &[u8; 32]) -> Result<usize, Error> {
    let mut stored = 0;
    for signed in &answer.signed_tokens {
        let Some((mailbox, epoch)) = check(store, signed, seal_key)? else {
            continue;
        };
        store.set_write_token(&mailbox, epoch, signed)?;
        stored += 1;
    }
    Ok(stored)
}

// The mailbox and day of a token this device can use: the signer's mailbox is the token's, and its
// key is the one held for that mailbox and day.
fn check(
    store: &Store,
    signed: &[u8],
    seal_key: &[u8; 32],
) -> Result<Option<([u8; 32], u32)>, Error> {
    let Ok(record) = SignedRecord::decode(signed) else {
        return Ok(None);
    };
    let Ok(token) = proto::WriteToken::decode(record.payload.as_slice()) else {
        return Ok(None);
    };
    let (Ok(signer), Ok(mailbox)) = (
        <[u8; 32]>::try_from(record.public_key.as_slice()),
        <[u8; 32]>::try_from(token.mailbox.as_slice()),
    ) else {
        return Ok(None);
    };
    if record.verify(Domain::WriteToken).is_err() || crate::mailbox(&signer) != mailbox {
        return Ok(None);
    }
    let held = store.held_key(&mailbox, token.epoch, seal_key)?;
    Ok(held
        .filter(|key| key.public_key().as_slice() == token.token_key)
        .map(|_| (mailbox, token.epoch)))
}

/// Puts `ciphertext` in `mailbox` with this device's token for `today` and returns the day of the
/// token the node took. On DENIED (a forged or revoked token) the write goes tokenless into the
/// mailbox pool and returns `None`. On STALE (the node is on another day) it retries with the
/// token for `today + 1`, then tokenless. Without a token it is tokenless from the start.
pub async fn send(
    node: &mut Node,
    store: &Store,
    mailbox: &[u8; 32],
    ciphertext: &[u8],
    today: u32,
    seal_key: &[u8; 32],
) -> Result<Option<u32>, session::Error> {
    let mut write = prepare(store, mailbox, ciphertext.to_vec(), today, seal_key)?;
    loop {
        match node.put_prepared(&write).await {
            Ok(()) => return Ok(epoch(&write)),
            Err(crate::Error::Status(status)) => {
                match fallback(store, &write, status, today, seal_key)? {
                    Some(next) => write = next,
                    None => return Err(crate::Error::Status(status).into()),
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
}

/// A put of `ciphertext` to `mailbox` with this device's token for `epoch`, or tokenless without
/// one.
pub(crate) fn prepare(
    store: &Store,
    mailbox: &[u8],
    ciphertext: Vec<u8>,
    epoch: u32,
    seal_key: &[u8; 32],
) -> Result<PreparedEnvelope, session::Error> {
    let held = match mailbox.try_into() {
        Ok(mailbox) => store.write_token(mailbox, epoch, seal_key)?,
        Err(_) => None,
    };
    Ok(match held {
        Some((key, token)) => crate::prepare_tokened_put(mailbox, ciphertext, &key, &token)?,
        None => crate::prepare_put(mailbox, ciphertext)?,
    })
}

/// The day of the token `write` carries.
pub(crate) fn epoch(write: &PreparedEnvelope) -> Option<u32> {
    let token = write.envelope().write_token?;
    proto::WriteToken::decode(token.payload.as_slice())
        .ok()
        .map(|token| token.epoch)
}

/// The same ciphertext to put after the node refused tokened `write` with `status`: on STALE with
/// the next day's token up to `today + 1`, otherwise tokenless. `None` when `write` was tokenless
/// or the refusal is not DENIED or STALE.
pub(crate) fn fallback(
    store: &Store,
    write: &PreparedEnvelope,
    status: Status,
    today: u32,
    seal_key: &[u8; 32],
) -> Result<Option<PreparedEnvelope>, session::Error> {
    let Some(epoch) = epoch(write) else {
        return Ok(None);
    };
    let ciphertext = write.envelope().ciphertext;
    let next = match status {
        Status::Stale if epoch < today.saturating_add(1) => {
            prepare(store, write.mailbox(), ciphertext, epoch + 1, seal_key)?
        }
        Status::Stale | Status::Denied => crate::prepare_put(write.mailbox(), ciphertext)?,
        _ => return Ok(None),
    };
    Ok(Some(next))
}

/// Revokes every token key this device issued to `contact` (hex Peer ID), for example when the
/// user blocks it: marks them revoked so they are never signed again, then tells the node to
/// refuse those still valid from `today`. Returns how many keys the node revoked; a failed call
/// can be repeated.
pub async fn revoke(
    node: &mut Node,
    store: &Store,
    device: &Identity,
    contact: &str,
    today: u32,
) -> Result<usize, session::Error> {
    let keys = store.revoke_issued(contact, today)?;
    node.revoke_tokens(device, &keys).await?;
    Ok(keys.len())
}

/// Signs the asks in `request` from `contact` (hex Peer ID of a verified group member) that name
/// this device's mailbox and a day from `today - 1` to `today + AHEAD_DAYS`, records each issued
/// key, and returns the answers to send, each fitting one envelope. Skips a key this profile
/// revoked or issued to another contact, and answers nothing to a blocked contact.
pub fn answer(
    store: &Store,
    device_key: &Identity,
    contact: &str,
    request: &TokenRequest,
    today: u32,
) -> Result<Vec<Body>, Error> {
    if store.blocked(contact)? {
        return Ok(Vec::new());
    }
    let mailbox = crate::mailbox(&device_key.public_key());
    let days = today.saturating_sub(1)..=today.saturating_add(AHEAD_DAYS);
    let mut answers = Vec::new();
    let mut batch = TokenAnswer::default();
    for ask in &request.asks {
        let Ok(token_key) = <[u8; 32]>::try_from(ask.token_key.as_slice()) else {
            continue;
        };
        if ask.mailbox != mailbox
            || !days.contains(&ask.epoch)
            || store.issued_revoked(&token_key)?
        {
            continue;
        }
        match store.record_issued(contact, &mailbox, &token_key, ask.epoch) {
            Err(Error::TokenConflict) => continue,
            issued => issued?,
        }
        let token = proto::WriteToken {
            mailbox: mailbox.to_vec(),
            token_key: token_key.to_vec(),
            epoch: ask.epoch,
        };
        let signed = device_key.sign(Domain::WriteToken, token.encode_to_vec());
        batch.signed_tokens.push(signed.encode_to_vec());
        if batch.encoded_len() > ANSWER_BYTES {
            let last = batch.signed_tokens.pop().into_iter().collect();
            answers.push(Body::TokenAnswer(std::mem::replace(
                &mut batch,
                TokenAnswer {
                    signed_tokens: last,
                },
            )));
        }
    }
    if !batch.signed_tokens.is_empty() {
        answers.push(Body::TokenAnswer(batch));
    }
    Ok(answers)
}

#[cfg(test)]
mod tests {
    use super::{accept, answer, request, ANSWER_BYTES};
    use crate::chat::{pad, Body, TokenAnswer, TokenAsk, TokenRequest, MAX_TOKENS};
    use crate::store::tests::temp_db_path;
    use crate::store::Store;
    use dyapp_identity::{Domain, Identity, SignedRecord};
    use dyapp_p2p_net::proto;
    use prost::Message as _;

    fn ask(mailbox: [u8; 32], epoch: u32, key: u8) -> TokenAsk {
        TokenAsk {
            mailbox: mailbox.to_vec(),
            epoch,
            token_key: vec![key; 32],
        }
    }

    fn tokens(bodies: &[Body]) -> Vec<proto::WriteToken> {
        let mut tokens = Vec::new();
        for body in bodies {
            let Body::TokenAnswer(answer) = body else {
                panic!("not an answer");
            };
            for signed in &answer.signed_tokens {
                let record = SignedRecord::decode(signed.as_slice()).unwrap();
                record.verify(Domain::WriteToken).unwrap();
                tokens.push(proto::WriteToken::decode(record.payload.as_slice()).unwrap());
            }
        }
        tokens
    }

    #[test]
    fn signs_own_mailbox_in_window_and_records_the_contact() {
        let device_key = Identity::generate();
        let own = crate::mailbox(&device_key.public_key());
        let store = Store::open(&temp_db_path(), &Identity::generate()).unwrap();
        let today = 20_000;
        let asks = vec![
            ask(own, today - 1, 1),
            ask(own, today + 30, 2),
            ask([9; 32], today, 3),
            ask(own, today - 2, 4),
            ask(own, today + 31, 5),
        ];
        let bodies = answer(&store, &device_key, "bob", &TokenRequest { asks }, today).unwrap();
        let signed = tokens(&bodies);
        assert_eq!(
            signed
                .iter()
                .map(|t| (t.token_key[0], t.epoch))
                .collect::<Vec<_>>(),
            [(1, today - 1), (2, today + 30)]
        );
        assert!(signed.iter().all(|t| t.mailbox == own));
        // Bob's key is now his: another contact's ask with it is skipped, a revoked one too.
        let again = TokenRequest {
            asks: vec![ask(own, today - 1, 1)],
        };
        assert!(answer(&store, &device_key, "carol", &again, today)
            .unwrap()
            .is_empty());
        store.revoke_issued("bob", today).unwrap();
        assert!(answer(&store, &device_key, "bob", &again, today)
            .unwrap()
            .is_empty());
        // A blocked contact gets no key signed, not even a fresh one.
        let fresh = TokenRequest {
            asks: vec![ask(own, today, 6)],
        };
        store.set_blocked("bob", true).unwrap();
        assert!(answer(&store, &device_key, "bob", &fresh, today)
            .unwrap()
            .is_empty());
        store.set_blocked("bob", false).unwrap();
        assert_eq!(
            tokens(&answer(&store, &device_key, "bob", &fresh, today).unwrap()).len(),
            1
        );
    }

    #[test]
    fn contact_asks_for_missing_days_and_refills_under_the_threshold() {
        let owner = Identity::generate();
        let own = crate::mailbox(&owner.public_key());
        let seal = [7; 32];
        let today = 20_000;
        let mut contact = Store::open(&temp_db_path(), &Identity::generate()).unwrap();
        let issuer = Store::open(&temp_db_path(), &Identity::generate()).unwrap();
        let Some(Body::TokenRequest(first)) = request(&mut contact, &[own], today, &seal).unwrap()
        else {
            panic!("no request");
        };
        assert_eq!(first.asks.len(), 30);
        // A retry before the answer asks with the same keys.
        let Some(Body::TokenRequest(retry)) = request(&mut contact, &[own], today, &seal).unwrap()
        else {
            panic!("no retry");
        };
        assert!(retry == first);
        let Body::TokenAnswer(answered) = answer(&issuer, &owner, "bob", &first, today)
            .unwrap()
            .remove(0)
        else {
            panic!("not an answer");
        };
        assert_eq!(accept(&contact, &answered, &seal).unwrap(), 30);
        assert!(request(&mut contact, &[own], today, &seal)
            .unwrap()
            .is_none());
        // 15 days left: still enough; 14 left: ask for the 16 missing days.
        assert!(request(&mut contact, &[own], today + 15, &seal)
            .unwrap()
            .is_none());
        let Some(Body::TokenRequest(refill)) =
            request(&mut contact, &[own], today + 16, &seal).unwrap()
        else {
            panic!("no refill");
        };
        assert_eq!(refill.asks.len(), 16);
        assert!(refill.asks.iter().all(|a| a.epoch >= today + 30));
    }

    #[test]
    fn contact_refuses_tokens_it_cannot_use() {
        let owner = Identity::generate();
        let own = crate::mailbox(&owner.public_key());
        let seal = [7; 32];
        let today = 20_000;
        let mut contact = Store::open(&temp_db_path(), &Identity::generate()).unwrap();
        let key = contact.token_key(&own, today, &seal).unwrap();
        let sign = |signer: &Identity, mailbox: [u8; 32], token_key: [u8; 32], epoch| {
            let token = proto::WriteToken {
                mailbox: mailbox.to_vec(),
                token_key: token_key.to_vec(),
                epoch,
            };
            signer
                .sign(Domain::WriteToken, token.encode_to_vec())
                .encode_to_vec()
        };
        let mut forged = SignedRecord::decode(sign(&owner, own, key, today).as_slice()).unwrap();
        forged.payload[0] ^= 1;
        let bad = [
            sign(&Identity::generate(), own, key, today),
            sign(&owner, own, [9; 32], today),
            sign(&owner, own, key, today + 1),
            owner
                .sign(Domain::Envelope, sign(&owner, own, key, today))
                .encode_to_vec(),
            forged.encode_to_vec(),
            vec![0xff],
        ];
        let forgeries = TokenAnswer {
            signed_tokens: bad.to_vec(),
        };
        assert_eq!(accept(&contact, &forgeries, &seal).unwrap(), 0);
        assert!(contact.write_token_epochs(&own, today).unwrap().is_empty());
        let good = TokenAnswer {
            signed_tokens: vec![sign(&owner, own, key, today)],
        };
        assert_eq!(accept(&contact, &good, &seal).unwrap(), 1);
        assert_eq!(contact.write_token_epochs(&own, today).unwrap(), [today]);
    }

    #[test]
    fn a_large_answer_splits_into_envelope_sized_bodies() {
        let device_key = Identity::generate();
        let own = crate::mailbox(&device_key.public_key());
        let store = Store::open(&temp_db_path(), &Identity::generate()).unwrap();
        let today = 20_000;
        let asks = (0..MAX_TOKENS)
            .map(|i| TokenAsk {
                token_key: (i as u64).to_be_bytes().repeat(4),
                ..ask(own, today + (i % 31) as u32, 0)
            })
            .collect();
        let bodies = answer(&store, &device_key, "bob", &TokenRequest { asks }, today).unwrap();
        assert!(bodies.len() > 1);
        assert_eq!(tokens(&bodies).len(), MAX_TOKENS);
        for body in &bodies {
            let encoded = body.encode();
            assert!(encoded.len() <= ANSWER_BYTES + 1);
            assert!(pad(&encoded).len() <= 64 * 1024);
            assert!(Body::decode(&encoded).ok().as_ref() == Some(body));
        }
    }
}

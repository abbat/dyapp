//! Mailbox write tokens carried in MLS chats. A contact device asks an owner device for a token
//! per day ahead with its own public token keys ([`TokenRequest`]); the owner device signs a
//! `WriteToken` for each with its mailbox key and answers ([`TokenAnswer`]).

use crate::chat::{Body, TokenAnswer, TokenRequest};
use crate::store::{Error, Store};
use dyapp_identity::{Domain, Identity};
use dyapp_p2p_net::proto;
use prost::Message as _;

/// How many days after today a contact holds tokens for.
pub const AHEAD_DAYS: u32 = 30;
// With the tag and length prefix this pads to 64 KiB, so the envelope stays under the node's
// 100 KiB limit less the 256-byte token reserve; the next padding bucket, 128 KiB, would not.
const ANSWER_BYTES: usize = 64 * 1024 - 1 - 4;

/// Signs the asks in `request` from `contact` (hex Peer ID of a verified group member) that name
/// this device's mailbox and a day from `today - 1` to `today + AHEAD_DAYS`, records each issued
/// key, and returns the answers to send, each fitting one envelope. Skips a key this profile
/// revoked or issued to another contact.
pub fn answer(
    store: &Store,
    device_key: &Identity,
    contact: &str,
    request: &TokenRequest,
    today: u32,
) -> Result<Vec<Body>, Error> {
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
    use super::{answer, ANSWER_BYTES};
    use crate::chat::{pad, Body, TokenAsk, TokenRequest, MAX_TOKENS};
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

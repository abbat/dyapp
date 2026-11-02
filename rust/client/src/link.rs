//! The `dyapp-link` code a new device shows and the sealed record it gets back
//! ([ADR 0040](../../../docs/decisions/0040-device-linking.md)). This is the only way the identity
//! key leaves a store.

use crate::{unhex, Error, Node};
use dyapp_identity::Identity;
use dyapp_messaging::link::{seal, LinkKey};
use std::collections::BTreeMap;
use zeroize::Zeroizing;

const PREFIX: &str = "dyapp-link:1:";
/// How long a code stays valid.
pub const TTL_SECONDS: u64 = 300;
const SECRET_LEN: usize = 32;
const ID_LEN: usize = 16;
const HEADER_LEN: usize = ID_LEN + 4;
/// Leaves room for the envelope around a part: a node takes 100 KiB.
const PART_DATA: usize = 90 * 1024;

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn key32(text: &str) -> Result<[u8; 32], Error> {
    unhex(text)
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or(Error::Input("bad link code"))
}

/// What a QR code carries from the new device to a device of the account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Code {
    /// One-use X25519 key the record is sealed to.
    pub public: [u8; 32],
    /// Mailbox of the new device.
    pub mailbox: [u8; 32],
    /// Unix seconds.
    pub expires_at: u64,
}

impl Code {
    /// The text form of the code. Contact QR codes never match it.
    pub fn text(&self) -> String {
        format!(
            "{PREFIX}{}:{}:{}",
            hex(&self.public),
            hex(&self.mailbox),
            self.expires_at
        )
    }

    /// Reads a scanned code, refusing a malformed or an expired one.
    pub fn parse(text: &str, now: u64) -> Result<Self, Error> {
        let rest = text
            .strip_prefix(PREFIX)
            .ok_or(Error::Input("bad link code"))?;
        let mut fields = rest.split(':');
        let (Some(public), Some(mailbox), Some(expires_at), None) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            return Err(Error::Input("bad link code"));
        };
        let code = Self {
            public: key32(public)?,
            mailbox: key32(mailbox)?,
            expires_at: expires_at
                .parse()
                .map_err(|_| Error::Input("bad link code"))?,
        };
        code.check(now)?;
        Ok(code)
    }

    fn check(&self, now: u64) -> Result<(), Error> {
        if now >= self.expires_at {
            return Err(Error::Input("the link code expired"));
        }
        Ok(())
    }

    /// Seals the identity key and the owner's local state for this code, as parts for its mailbox.
    pub fn seal_record(
        &self,
        identity: &Identity,
        state: &[u8],
        now: u64,
    ) -> Result<Vec<Vec<u8>>, Error> {
        self.check(now)?;
        let secret = identity.secret();
        let record = Zeroizing::new([secret.as_slice(), state].concat());
        let sealed = seal(&self.public, &record)?;
        let mut id = [0; ID_LEN];
        getrandom::fill(&mut id).map_err(|_| Error::Input("no system randomness"))?;
        let chunks = sealed.chunks(PART_DATA);
        let count = u16::try_from(chunks.len()).map_err(|_| Error::Input("link state too big"))?;
        Ok((0..count)
            .zip(chunks)
            .map(|(index, data)| {
                [&id[..], &index.to_be_bytes(), &count.to_be_bytes(), data].concat()
            })
            .collect())
    }

    /// Seals the record and puts its parts in the code's mailbox.
    pub async fn send(
        &self,
        node: &mut Node,
        identity: &Identity,
        state: &[u8],
        now: u64,
    ) -> Result<(), Error> {
        for part in self.seal_record(identity, state, now)? {
            node.put(&self.mailbox, part).await?;
        }
        Ok(())
    }
}

/// The new device's side of a link: the one-use key behind a code.
pub struct Invite {
    key: LinkKey,
    code: Code,
}

/// What a linked device receives from the account.
pub struct Linked {
    /// The account.
    pub identity: Identity,
    /// The owner's local state, opaque here.
    pub state: Vec<u8>,
}

impl Invite {
    /// Starts a link that delivers to `mailbox`.
    pub fn new(mailbox: [u8; 32], now: u64) -> Result<Self, Error> {
        let key = LinkKey::generate()?;
        let public = key
            .public()
            .try_into()
            .map_err(|_| Error::Input("bad link key"))?;
        Ok(Self {
            key,
            code: Code {
                public,
                mailbox,
                expires_at: now + TTL_SECONDS,
            },
        })
    }

    /// The code to show.
    pub fn code(&self) -> &Code {
        &self.code
    }

    /// The record waiting in the mailbox of `device_key`, if a complete one opens. Poll this until
    /// the code expires; the call after that erases the key.
    pub async fn receive(
        &mut self,
        node: &mut Node,
        device_key: &Identity,
        now: u64,
    ) -> Result<Option<Linked>, Error> {
        self.live(now)?;
        let (mut parts, mut ids, mut cursor) = (Vec::new(), Vec::new(), None);
        loop {
            let page = node.fetch_page(device_key, cursor.as_deref(), 100).await?;
            for envelope in page.envelopes {
                ids.push(envelope.id);
                parts.push(envelope.ciphertext);
            }
            if !page.more {
                break;
            }
            cursor = page.next_cursor;
        }
        let linked = self.open(&parts, now)?;
        if linked.is_some() {
            // The key is gone, so a lost ack must not lose the record; the parts expire unread.
            let _ = node.ack(device_key, ids).await;
        }
        Ok(linked)
    }

    // An expired code takes its key with it.
    fn live(&mut self, now: u64) -> Result<(), Error> {
        let state = self.code.check(now);
        if state.is_err() {
            self.key.erase();
        }
        state
    }

    /// Looks for a record in what the mailbox holds, in any order and among anything else. `None`
    /// means no complete record opened yet: someone who saw the code cannot spend the key with junk.
    pub fn open(&mut self, parts: &[Vec<u8>], now: u64) -> Result<Option<Linked>, Error> {
        self.live(now)?;
        let mut records: BTreeMap<(&[u8], u16), BTreeMap<u16, &[u8]>> = BTreeMap::new();
        for part in parts.iter().filter(|part| part.len() > HEADER_LEN) {
            let (id, rest) = part.split_at(ID_LEN);
            let index = u16::from_be_bytes([rest[0], rest[1]]);
            let count = u16::from_be_bytes([rest[2], rest[3]]);
            if index < count {
                records
                    .entry((id, count))
                    .or_default()
                    .entry(index)
                    .or_insert(&rest[4..]);
            }
        }
        for ((_, count), record) in records {
            if record.len() != usize::from(count) {
                continue;
            }
            let Ok(opened) = self.key.open(&record.into_values().collect::<Vec<_>>().concat())
            else {
                continue;
            };
            let opened = Zeroizing::new(opened);
            let (secret, state) = opened
                .split_first_chunk::<SECRET_LEN>()
                .ok_or(Error::Input("short link record"))?;
            return Ok(Some(Linked {
                identity: Identity::from_secret(secret),
                state: state.to_vec(),
            }));
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dyapp_identity::safety::{qr_matches, qr_payload};

    const NOW: u64 = 1_000;

    fn invite() -> Invite {
        Invite::new([7; 32], NOW).unwrap()
    }

    #[test]
    fn a_code_survives_its_text_form_and_refuses_anything_else() {
        let invite = invite();
        let code = invite.code();
        let text = code.text();
        assert_eq!(Code::parse(&text, NOW).unwrap(), *code);

        assert!(Code::parse(&text, NOW + TTL_SECONDS).is_err());
        assert!(Code::parse(&format!("{text}:1"), NOW).is_err());
        assert!(Code::parse(&text.replace("dyapp-link:1", "dyapp-link:2"), NOW).is_err());
        assert!(Code::parse(&text[..text.len() - 40], NOW).is_err());
        assert!(Code::parse(
            &String::from_utf8_lossy(&qr_payload(&[1; 32], &[2; 32])),
            NOW
        )
        .is_err());
        assert!(!qr_matches(text.as_bytes(), &code.public, &code.mailbox));
    }

    #[test]
    fn a_record_crosses_in_parts_among_junk_and_opens_once() {
        let identity = Identity::generate();
        let state = vec![9; 2 * PART_DATA + 1];
        let mut invite = invite();
        let code = Code::parse(&invite.code().text(), NOW).unwrap();
        let mut parts = code.seal_record(&identity, &state, NOW).unwrap();
        assert_eq!(parts.len(), 3);
        assert!(parts
            .iter()
            .all(|part| part.len() <= HEADER_LEN + PART_DATA));

        // Nothing opens until the last part arrives, and junk spends nothing.
        let last = parts.remove(1);
        let junk = vec![1; HEADER_LEN + 40];
        parts.reverse();
        parts.push(junk);
        parts.push(vec![2; 3]);
        assert!(invite.open(&parts, NOW).unwrap().is_none());

        parts.push(last);
        let linked = invite.open(&parts, NOW).unwrap().unwrap();
        assert_eq!(linked.identity.peer_id(), identity.peer_id());
        assert_eq!(linked.state, state);
        assert!(invite.open(&parts, NOW).unwrap().is_none());
    }

    #[test]
    fn only_the_key_of_the_code_opens_a_record() {
        let parts = invite()
            .code()
            .seal_record(&Identity::generate(), b"state", NOW)
            .unwrap();
        assert!(invite().open(&parts, NOW).unwrap().is_none());
    }

    #[test]
    fn an_expired_code_seals_and_opens_nothing() {
        let mut invite = invite();
        let code = *invite.code();
        let identity = Identity::generate();
        assert!(code.seal_record(&identity, b"", code.expires_at).is_err());
        let parts = code.seal_record(&identity, b"", NOW).unwrap();
        assert!(invite.open(&parts, code.expires_at).is_err());
        // The key went with the code: even a clock set back opens nothing.
        assert!(invite.open(&parts, NOW).unwrap().is_none());
    }
}

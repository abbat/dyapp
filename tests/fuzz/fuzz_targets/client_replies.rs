#![no_main]
//! Mailbox and search answers from an untrusted node.

use dyapp_fuzz::{split, IDENTITY};
use dyapp_identity::Domain;
use dyapp_p2p_net::{
    proto::{self, MailboxResponse, SearchResponse},
    search_results,
};
use prost::Message;

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let Some((selector, data)) = split(data) else {
        return;
    };
    if selector & 1 == 0 {
        let Ok(response) = MailboxResponse::decode(data) else {
            return;
        };
        let _envelopes: Vec<dyapp_client::Envelope> = response
            .envelopes
            .into_iter()
            .filter_map(|record| {
                let envelope = proto::Envelope::decode(record.payload.as_slice()).ok()?;
                Some(dyapp_client::Envelope {
                    id: envelope.id,
                    sender_key: record.public_key,
                    ciphertext: envelope.ciphertext,
                })
            })
            .collect();
        return;
    }
    let Ok(mut response) = SearchResponse::decode(data) else {
        return;
    };
    if selector & 0x80 != 0 {
        for record in &mut response.records {
            *record = IDENTITY.sign(
                Domain::Profile,
                std::mem::take(&mut record.payload),
            );
        }
    }
    let _ = search_results::check(&[], 0, &[response.records]);
});

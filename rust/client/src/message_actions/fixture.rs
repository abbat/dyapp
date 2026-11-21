use super::*;
use crate::store::messages::VerifiedChatSender;

pub(super) struct Fixture {
    pub(super) media: crate::sticker_media::tests::Fixture,
    pub(super) text: MessageKey,
    pub(super) sticker: MessageKey,
}

impl Fixture {
    pub(super) fn new() -> Self {
        let mut media = crate::sticker_media::tests::Fixture::new(true, false);
        let own = media.store.owner_peer_id().to_owned();
        let mut sticker = media
            .store
            .current_chat_stickers(&media.peer)
            .unwrap()
            .remove(0)
            .sticker;
        sticker.id = [2; 16];
        for author in [&own, &media.peer] {
            media
                .store
                .record_chat_sticker(
                    VerifiedChatSender {
                        group: &media.key.group,
                        author,
                        device: &[8; 32],
                    },
                    &sticker,
                    false,
                )
                .unwrap();
        }
        let sticker = MessageKey {
            group: media.key.group.clone(),
            author: own,
            id: sticker.id,
        };
        let text = MessageKey {
            id: [3; 16],
            ..sticker.clone()
        };
        media.store.record_chat_text(&text, b"own text").unwrap();
        Self {
            media,
            text,
            sticker,
        }
    }

    pub(super) fn select(&self, key: &MessageKey, kind: Kind) -> Result<[u8; 16], Error> {
        prepare(
            &self.media.store,
            Source {
                peer: &self.media.peer,
                group: &key.group,
                author: &key.author,
                id: &key.id,
            },
            kind,
        )
    }
}

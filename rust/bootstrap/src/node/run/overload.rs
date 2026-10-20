//! Which events are requests, dropped unanswered at `max_memory_mb`.

use super::*;

/// An inbound request of any served protocol.
pub(super) fn is_request(event: &BehaviourEvent) -> bool {
    matches!(
        event,
        BehaviourEvent::Node(Event::Message {
            message: Message::Request { .. },
            ..
        }) | BehaviourEvent::Profile(Event::Message {
            message: Message::Request { .. },
            ..
        }) | BehaviourEvent::KeyPackage(Event::Message {
            message: Message::Request { .. },
            ..
        }) | BehaviourEvent::Media(Event::Message {
            message: Message::Request { .. },
            ..
        }) | BehaviourEvent::Mailbox(Event::Message {
            message: Message::Request { .. },
            ..
        }) | BehaviourEvent::Search(Event::Message {
            message: Message::Request { .. },
            ..
        }) | BehaviourEvent::Inventory(Event::Message {
            message: Message::Request { .. },
            ..
        }) | BehaviourEvent::Digest(Event::Message {
            message: Message::Request { .. },
            ..
        })
    )
}

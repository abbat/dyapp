//! Keep only cursor windows per holder; construct proof pages at dispatch.
use super::{Job, Repair, Service, TIMEOUT};
use crate::media::MediaStore;
use crate::{BootstrapError, Result};
use dyapp_identity::{Domain, SignedRecord};
use dyapp_p2p_net::proto;
use libp2p::PeerId;
use prost::Message;
use std::collections::HashMap;
use std::time::Instant;

fn invalid() -> BootstrapError {
    BootstrapError::InvalidRequest("invalid media repair ancestry progress".into())
}
fn intent(record: &SignedRecord) -> Result<proto::MediaKeepIntent> {
    proto::MediaKeepIntent::decode(record.payload.as_slice()).map_err(|_| invalid())
}

#[derive(Clone, Copy)]
struct Window {
    ceiling: u64,
    floor: u64,
    probe: bool,
}
#[derive(Default)]
struct Cursor {
    next: Option<u64>,
    sent: Option<Window>,
    current: bool,
    done: bool,
}

impl Cursor {
    fn accept(&mut self, progress: &proto::MediaKeepProgress, window: Window) -> Result<bool> {
        if progress.next_version > window.floor
            || (!window.probe && progress.next_version >= window.ceiling)
        {
            return Err(invalid());
        }
        if progress.ready {
            if progress.target_current != Some(true) {
                return Err(invalid());
            }
            self.current = true;
        } else if progress.target_current.is_some() || progress.next_version == 0 || self.current {
            return Err(invalid());
        }
        self.next = Some(progress.next_version);
        self.done = progress.next_version == 0;
        Ok(true)
    }
}

pub(crate) struct History {
    owner: String,
    expected: u64,
    started: bool,
    peers: HashMap<PeerId, Cursor>,
}

fn progress(response: Option<proto::MediaResponse>) -> Result<proto::MediaKeepProgress> {
    let mut response = response.ok_or_else(invalid)?;
    let progress = response.keep_progress.take().ok_or_else(invalid)?;
    let expected = proto::MediaResponse {
        status: proto::Status::Ok.into(),
        ..Default::default()
    };
    if response != expected {
        return Err(invalid());
    }
    Ok(progress)
}

impl History {
    pub(crate) fn from_record(record: &SignedRecord) -> Result<Option<Self>> {
        let Ok(owner) = record.verify(Domain::MediaConditionalKeep) else {
            record.verify(Domain::MediaKeep).map_err(|_| invalid())?;
            return Ok(None);
        };
        let head = intent(record)?;
        let expected = head.expected.ok_or_else(invalid)?.version;
        if expected.checked_add(1) != Some(head.version) {
            return Err(invalid());
        }
        Ok(Some(Self {
            owner,
            expected,
            started: false,
            peers: HashMap::new(),
        }))
    }

    pub(crate) fn track(&mut self, peer: PeerId) {
        self.peers.entry(peer).or_default();
    }

    pub(crate) fn waiting(&self, peer: &PeerId) -> bool {
        self.peers
            .get(peer)
            .is_some_and(|cursor| cursor.sent.is_some())
    }

    pub(crate) fn finished(&self, peer: &PeerId) -> bool {
        self.peers.get(peer).is_some_and(|cursor| cursor.done)
    }

    fn pending(&mut self, peers: impl Iterator<Item = PeerId>) -> Vec<PeerId> {
        if !self.started {
            self.peers
                .extend(peers.map(|peer| (peer, Cursor::default())));
            self.started = true;
        }
        self.peers
            .iter()
            .filter(|(_, cursor)| !cursor.done)
            .map(|(peer, _)| *peer)
            .collect()
    }

    fn window(
        &self,
        media: &MediaStore,
        next: Option<u64>,
    ) -> Result<(Vec<SignedRecord>, Option<Window>)> {
        let Some(through) = next else {
            return Ok((
                Vec::new(),
                Some(Window {
                    ceiling: self.expected,
                    floor: self.expected,
                    probe: true,
                }),
            ));
        };
        let records = media.keep_history_reverse(
            &self.owner,
            through,
            MediaStore::keep_history_page_bytes(),
        )?;
        let Some(first) = records.first() else {
            return Ok((records, None));
        };
        if intent(first)?.version != through {
            return Err(invalid());
        }
        let last = records.last().ok_or_else(invalid)?;
        let floor = intent(last)?.expected.ok_or_else(invalid)?.version;
        if floor >= through {
            return Err(invalid());
        }
        Ok((
            records,
            Some(Window {
                ceiling: through,
                floor,
                probe: false,
            }),
        ))
    }

    pub(crate) fn request(
        &mut self,
        peer: PeerId,
        target: &SignedRecord,
        media: &MediaStore,
    ) -> Result<Option<proto::MediaRequest>> {
        let cursor = self.peers.get(&peer).ok_or_else(invalid)?;
        if cursor.done {
            return Ok(None);
        }
        if cursor.sent.is_some() {
            return Err(invalid());
        }
        let next = cursor.next;
        let current = cursor.current;
        let (ancestors, window) = self.window(media, next)?;
        let cursor = self.peers.get_mut(&peer).ok_or_else(invalid)?;
        let Some(window) = window else {
            if !current {
                return Err(invalid());
            }
            cursor.done = true;
            return Ok(None);
        };
        cursor.sent = Some(window);
        Ok(Some(proto::MediaRequest {
            request: Some(proto::media_request::Request::KeepCatchup(
                proto::MediaKeepCatchup {
                    target: Some(target.clone()),
                    ancestors,
                },
            )),
        }))
    }

    pub(crate) fn receive(
        &mut self,
        peer: PeerId,
        response: Option<proto::MediaResponse>,
    ) -> Result<bool> {
        let cursor = self.peers.get_mut(&peer).ok_or_else(invalid)?;
        // A dispatch with no older local proof finishes optional backfill after valid admission.
        if cursor.done && cursor.current && cursor.sent.is_none() && response.is_none() {
            return Ok(false);
        }
        let window = cursor.sent.take().ok_or_else(invalid)?;
        cursor.accept(&progress(response)?, window)
    }

    pub(crate) fn fail(&mut self, peer: PeerId) {
        if let Some(cursor) = self.peers.get_mut(&peer) {
            cursor.done = true;
            cursor.current = false;
            cursor.sent = None;
        }
    }

    pub(crate) fn admitted(&self, peer: &PeerId) -> bool {
        self.peers
            .get(peer)
            .is_some_and(|cursor| cursor.done && cursor.current)
    }
}

impl Repair {
    pub(super) fn history_pending(&mut self) -> bool {
        let Some(history) = &mut self.history else {
            return false;
        };
        let peers = history.pending(self.groups.keys().copied());
        for peer in peers {
            self.issue(Job::History(peer));
        }
        !self.jobs.is_empty()
    }

    pub(super) fn history_request(
        &mut self,
        peer: PeerId,
        service: &Service,
    ) -> Option<(proto::MediaRequest, u64)> {
        let result = self
            .history
            .as_mut()?
            .request(peer, &self.record, service.media.as_ref()?);
        match result {
            Ok(request) => request.map(|request| (request, 1024)),
            Err(error) => {
                tracing::warn!(%error, "media repair ancestry unavailable");
                self.history.as_mut()?.fail(peer);
                self.unconfirmed = true;
                None
            }
        }
    }

    pub(super) fn history_response(
        &mut self,
        peer: PeerId,
        response: Option<proto::MediaResponse>,
        now: Instant,
    ) {
        let Some(history) = &mut self.history else {
            self.unconfirmed = true;
            return;
        };
        if now >= self.deadline {
            history.fail(peer);
            self.unconfirmed = true;
            return;
        }
        match history.receive(peer, response) {
            Ok(true) => self.deadline = self.deadline.max(now + TIMEOUT),
            Ok(false) => {}
            Err(_) => {
                history.fail(peer);
                self.unconfirmed = true;
            }
        }
    }
}

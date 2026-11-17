use super::{
    drop_banned, judge, search_key, spread, Behaviour, Caught, Peer, PeerId, ResponseChannel,
    SearchResponse, Service, SignedRecord, Swarm, Verdict,
};
use dyapp_p2p_net::proto;
use libp2p::gossipsub;
use prost::Message;

pub(super) enum PowDone {
    Publish {
        peer: PeerId,
        from: Peer,
        channel: ResponseChannel<SearchResponse>,
        request: proto::SearchRequest,
        ok: bool,
    },
    Gossip {
        source: PeerId,
        id: gossipsub::MessageId,
        data: Vec<u8>,
        ok: bool,
    },
    CatchUp {
        records: Vec<SignedRecord>,
    },
}

pub(super) fn publish(
    swarm: &mut Swarm<Behaviour>,
    service: &Service,
    peer: PeerId,
    from: &Peer,
    channel: ResponseChannel<SearchResponse>,
    request: &proto::SearchRequest,
    ok: bool,
) -> bool {
    let succeeded = match service.search_pow_done(from, request, ok) {
        Ok(response) => {
            let gossip = search_key(request).map(|key| (key, request.encode_to_vec()));
            spread(swarm, gossip, &response, service);
            service.traffic.add(response.encoded_len() as u64);
            let _ = swarm
                .behaviour_mut()
                .search
                .send_response(channel, response);
            true
        }
        Err(error) => {
            tracing::error!(%error, "search request failed");
            false
        }
    };
    drop_banned(swarm, service, peer);
    succeeded
}

pub(super) fn gossip(
    swarm: &mut Swarm<Behaviour>,
    service: &Service,
    source: PeerId,
    id: &gossipsub::MessageId,
    data: &[u8],
    ok: bool,
) {
    let verdict = match ok.then(|| service.search_gossip(data)) {
        None => Verdict::Reject,
        // A cache cleared since the check asks again: drop without a strike.
        Some(Verdict::Pow(_)) => Verdict::Ignore,
        Some(verdict) => verdict,
    };
    judge(swarm, service, id, &source, &verdict);
}

pub(super) fn catch_up(service: &Service, records: &[SignedRecord]) {
    let stored = records
        .iter()
        .filter(|record| matches!(service.search_apply(record), Ok(Caught::Stored)))
        .count();
    tracing::debug!(
        stored,
        "search catch-up records applied after proof of work"
    );
}

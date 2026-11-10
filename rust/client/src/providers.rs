//! State of one provider lookup, retained when its deadline cancels collection.

use dyapp_p2p_net::{id_has_pow, id_pow_bits, Behaviour, BehaviourEvent};
use libp2p::core::ConnectedPoint;
use libp2p::futures::StreamExt;
use libp2p::kad;
use libp2p::swarm::dial_opts::{DialOpts, PeerCondition};
use libp2p::swarm::{ConnectionId, SwarmEvent};
use libp2p::{Multiaddr, PeerId, Swarm};
use std::collections::HashSet;

#[derive(Default)]
pub(super) struct Providers {
    pub(super) found: Vec<Multiaddr>,
    seen: HashSet<PeerId>,
    dials: HashSet<ConnectionId>,
    closing: HashSet<ConnectionId>,
    done: bool,
}

impl Providers {
    pub(super) async fn collect(
        &mut self,
        swarm: &mut Swarm<Behaviour>,
        query: kad::QueryId,
        max: usize,
    ) {
        // Drain closes: the next lookup must not use a connection that is still closing.
        while (self.found.len() < max && !(self.done && self.dials.is_empty()))
            || !self.closing.is_empty()
        {
            let event = swarm.select_next_some().await;
            self.event(swarm, query, event);
        }
    }

    fn event(
        &mut self,
        swarm: &mut Swarm<Behaviour>,
        query: kad::QueryId,
        event: SwarmEvent<BehaviourEvent>,
    ) {
        match event {
            SwarmEvent::Behaviour(BehaviourEvent::Kad(kad::Event::OutboundQueryProgressed {
                id,
                result: kad::QueryResult::GetProviders(result),
                step,
                ..
            })) if id == query => {
                self.dial(swarm, result);
                self.done |= step.last;
            }
            SwarmEvent::ConnectionEstablished {
                connection_id,
                endpoint: ConnectedPoint::Dialer { address, .. },
                ..
            } if self.dials.remove(&connection_id) => {
                swarm.close_connection(connection_id);
                self.closing.insert(connection_id);
                self.found.push(address);
            }
            SwarmEvent::ConnectionClosed { connection_id, .. } => {
                self.closing.remove(&connection_id);
            }
            SwarmEvent::OutgoingConnectionError { connection_id, .. } => {
                self.dials.remove(&connection_id);
            }
            _ => {}
        }
    }

    fn dial(
        &mut self,
        swarm: &mut Swarm<Behaviour>,
        result: Result<kad::GetProvidersOk, kad::GetProvidersError>,
    ) {
        let Ok(kad::GetProvidersOk::FoundProviders { providers, .. }) = result else {
            return;
        };
        for provider in providers {
            if !self.seen.insert(provider) || !id_has_pow(&provider, id_pow_bits()) {
                continue;
            }
            // Even the current node needs a second connection to learn its address.
            let dial = DialOpts::peer_id(provider)
                .condition(PeerCondition::Always)
                .build();
            let connection = dial.connection_id();
            if swarm.dial(dial).is_ok() {
                self.dials.insert(connection);
            }
        }
    }
}

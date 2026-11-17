use super::events;
use super::hangup::Hangup;
use super::pow::PowDone;
use super::search_jobs::{SearchDone, SearchJob};
use super::{
    cached_peers, dispatch, identified, jobs, refill, reload_deny, republish, state, Arc,
    Behaviour, ConnectionId, DenyStore, Duration, Finished, HashMap, Job, Multiaddr,
    OutboundRequestId, PeerId, Replicas, Role, Service, Swarm, VecDeque, Watchers, SEARCH_KEY,
    SEARCH_TOPICS,
};
use libp2p::kad;

pub(super) struct Channels {
    pub(super) search_done_tx: tokio::sync::mpsc::UnboundedSender<SearchDone>,
    pub(super) search_done: tokio::sync::mpsc::UnboundedReceiver<SearchDone>,
    pub(super) pow_tx: tokio::sync::mpsc::UnboundedSender<PowDone>,
    pub(super) pow_done: tokio::sync::mpsc::UnboundedReceiver<PowDone>,
    pub(super) store_tx: tokio::sync::mpsc::UnboundedSender<Finished>,
    pub(super) store_done: tokio::sync::mpsc::UnboundedReceiver<Finished>,
}

impl Channels {
    fn new() -> Self {
        let (search_done_tx, search_done) = tokio::sync::mpsc::unbounded_channel();
        let (pow_tx, pow_done) = tokio::sync::mpsc::unbounded_channel();
        let (store_tx, store_done) = tokio::sync::mpsc::unbounded_channel();
        Self {
            search_done_tx,
            search_done,
            pow_tx,
            pow_done,
            store_tx,
            store_done,
        }
    }
}

pub(super) struct Timers {
    pub(super) deny_check: tokio::time::Interval,
    pub(super) puzzle_tick: tokio::time::Interval,
    pub(super) put_check: tokio::time::Interval,
    pub(super) cleanup: tokio::time::Interval,
    pub(super) republish_scan: tokio::time::Interval,
    pub(super) maintain: tokio::time::Interval,
    pub(super) rejoin: tokio::time::Interval,
    pub(super) catch_up_at: tokio::time::Instant,
    pub(super) catch_up: tokio::time::Interval,
}

impl Timers {
    fn new(service: &Service, maintenance: &crate::config::Maintenance) -> Self {
        let now = super::clock_instant(service);
        let every = Duration::from_secs(300);
        Self {
            deny_check: tokio::time::interval(Duration::from_secs(10)),
            puzzle_tick: tokio::time::interval(crate::puzzle::TICK),
            put_check: tokio::time::interval(Duration::from_millis(100)),
            cleanup: tokio::time::interval(Duration::from_secs(3600)),
            republish_scan: tokio::time::interval(republish::REPUBLISH_INTERVAL),
            maintain: tokio::time::interval(Duration::from_secs(
                u64::from(maintenance.interval_minutes).saturating_mul(60),
            )),
            rejoin: tokio::time::interval_at(now.checked_add(every).unwrap_or(now), every),
            catch_up_at: now,
            catch_up: tokio::time::interval_at(
                now.checked_add(Duration::from_secs(60)).unwrap_or(now),
                Duration::from_secs(3600),
            ),
        }
    }
}

pub(super) struct State {
    pub(super) hangup: Hangup,
    pub(super) anchors: Vec<Multiaddr>,
    pub(super) deny: DenyStore,
    pub(super) deny_version: Option<i64>,
    pub(super) nonces: HashMap<ConnectionId, [u8; 32]>,
    pub(super) admission: identified::Admission,
    pub(super) watchers: Watchers,
    pub(super) replicas: Replicas,
    pub(super) refused: u64,
    pub(super) turn_unannounced: bool,
    pub(super) turn_query: Option<kad::QueryId>,
    pub(super) search_unannounced: bool,
    pub(super) search_query: Option<kad::QueryId>,
    pub(super) search_queue: jobs::GroupQueue<SearchJob>,
    pub(super) searching: usize,
    pub(super) pow_permits: Arc<tokio::sync::Semaphore>,
    pub(super) pow_gossip: usize,
    pub(super) asks: crate::catch_up::Asks,
    pub(super) catch_up_queue: HashMap<PeerId, VecDeque<[u8; 32]>>,
    pub(super) catch_up_asked: HashMap<OutboundRequestId, (PeerId, Vec<[u8; 32]>)>,
    pub(super) refill_round: refill::Round,
    pub(super) answered: u64,
    pub(super) failed: u64,
    pub(super) foreign_peers: u64,
    pub(super) maintenance: crate::config::Maintenance,
    pub(super) seeds: Vec<Multiaddr>,
    pub(super) store_queue: jobs::FairQueue<(ConnectionId, Job)>,
    pub(super) channels: Channels,
    pub(super) timers: Timers,
}

impl State {
    pub(super) fn new(swarm: &mut Swarm<Behaviour>, service: &Arc<Service>) -> Option<Self> {
        let (hangup, deny, deny_version) = open(service)?;
        let turn_unannounced = service.config.roles.contains(&Role::Turn);
        let search_unannounced = service.search.is_some();
        store_local_providers(swarm, turn_unannounced, search_unannounced);
        subscribe_search_topics(swarm, service);
        refill::spawn_rebuild(service);
        let maintenance = service.config.maintenance.clone();
        let timers = Timers::new(service, &maintenance);
        Some(Self::fields(
            hangup,
            deny,
            deny_version,
            service,
            (turn_unannounced, search_unannounced),
            maintenance,
            timers,
        ))
    }

    fn fields(
        hangup: Hangup,
        deny: DenyStore,
        deny_version: Option<i64>,
        service: &Arc<Service>,
        (turn_unannounced, search_unannounced): (bool, bool),
        maintenance: crate::config::Maintenance,
        timers: Timers,
    ) -> Self {
        let mut replicas = Replicas::default();
        let delay = u64::from(service.config.network.storage_trust_minutes) * 60;
        replicas.trust.delay = Duration::from_secs(delay);
        Self {
            hangup,
            anchors: cached_peers(&service.config.storage.anchors_path()),
            deny,
            deny_version,
            nonces: HashMap::new(),
            admission: identified::Admission::default(),
            watchers: Watchers::new(),
            replicas,
            refused: 0,
            turn_unannounced,
            turn_query: None,
            search_unannounced,
            search_query: None,
            search_queue: jobs::GroupQueue::default(),
            searching: 0,
            pow_permits: Arc::new(tokio::sync::Semaphore::new(
                std::thread::available_parallelism().map_or(2, |n| (n.get() / 2).max(2)),
            )),
            pow_gossip: 0,
            asks: crate::catch_up::Asks::default(),
            catch_up_queue: HashMap::new(),
            catch_up_asked: HashMap::new(),
            refill_round: refill::Round::default(),
            answered: 0,
            failed: 0,
            foreign_peers: 0,
            maintenance,
            seeds: service
                .config
                .seeds
                .iter()
                .filter_map(|seed| crate::config::seed(seed).ok())
                .flatten()
                .collect(),
            store_queue: jobs::FairQueue::default(),
            channels: Channels::new(),
            timers,
        }
    }

    pub(super) fn events<'a>(
        &'a mut self,
        swarm: &'a mut Swarm<Behaviour>,
        service: &'a Arc<Service>,
    ) -> events::Context<'a> {
        events::Context {
            swarm,
            service,
            anchors: &mut self.anchors,
            deny: &self.deny,
            nonces: &mut self.nonces,
            admission: &mut self.admission,
            watchers: &mut self.watchers,
            replicas: &mut self.replicas,
            refused: &mut self.refused,
            turn_unannounced: &mut self.turn_unannounced,
            turn_query: &mut self.turn_query,
            search_unannounced: &mut self.search_unannounced,
            search_query: &mut self.search_query,
            search_queue: &mut self.search_queue,
            searching: &mut self.searching,
            search_done: &self.channels.search_done_tx,
            pow_permits: &self.pow_permits,
            pow_done: &self.channels.pow_tx,
            pow_gossip: &mut self.pow_gossip,
            asks: &mut self.asks,
            catch_up_queue: &mut self.catch_up_queue,
            catch_up_asked: &mut self.catch_up_asked,
            catch_up_at: &mut self.timers.catch_up_at,
            refill_round: &mut self.refill_round,
            answered: &mut self.answered,
            failed: &mut self.failed,
            foreign_peers: &mut self.foreign_peers,
            store_queue: &mut self.store_queue,
        }
    }

    pub(super) fn dispatch(&mut self, service: &Arc<Service>) {
        dispatch(
            &mut self.store_queue,
            &mut self.nonces,
            service,
            &self.channels.store_tx,
        );
        if self.asks.idle()
            && self.catch_up_asked.is_empty()
            && self.catch_up_queue.values().all(VecDeque::is_empty)
        {
            self.refill_round.drained(service);
        }
    }

    pub(super) fn report_state(&self, swarm: &Swarm<Behaviour>) {
        state(
            &self.replicas,
            swarm,
            &[
                self.nonces.len(),
                swarm.behaviour().endpoints.len(),
                self.admission.len(),
            ],
            &self.watchers,
        );
    }
}

fn hangup() -> Option<Hangup> {
    match Hangup::new() {
        Ok(hangup) => Some(hangup),
        Err(error) => {
            tracing::error!(%error, "SIGHUP handler not installed");
            None
        }
    }
}

fn open(service: &Service) -> Option<(Hangup, DenyStore, Option<i64>)> {
    let hangup = hangup()?;
    let deny = match DenyStore::open(&service.config.storage.deny_path()) {
        Ok(deny) => deny,
        Err(error) => {
            tracing::error!(%error, "deny list not opened");
            return None;
        }
    };
    let mut deny_version = None;
    reload_deny(service, &deny, &mut deny_version);
    Some((hangup, deny, deny_version))
}

fn store_local_providers(swarm: &mut Swarm<Behaviour>, turn: bool, search: bool) {
    // Until announced, the node lists itself only in its own store, enough for a client that asks it.
    let local = *swarm.local_peer_id();
    let keys = [(turn, dyapp_p2p_net::TURN_KEY), (search, SEARCH_KEY)];
    for (_, key) in keys.into_iter().filter(|(provides, _)| *provides) {
        use libp2p::kad::store::RecordStore as _;
        let record = kad::ProviderRecord::new(kad::RecordKey::new(&key), local, Vec::new());
        if let Err(error) = swarm.behaviour_mut().kad.store_mut().add_provider(record) {
            tracing::error!(%error, "provider record not stored");
        }
    }
}

fn subscribe_search_topics(swarm: &mut Swarm<Behaviour>, service: &Service) {
    if let Some(gossip) = swarm.behaviour_mut().gossip.as_mut() {
        for topic in (0..SEARCH_TOPICS).filter(|topic| service.config.search.keeps(*topic)) {
            if let Err(error) = gossip.subscribe(&service.protocols.search_topic(topic)) {
                tracing::error!(%error, topic, "search topic not joined");
            }
        }
    }
}

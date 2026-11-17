//! The swarm event loop of a node.

use super::*;
mod catch_up;
mod events;
mod finished;
mod hangup;
mod maintenance;
mod overload;
mod pow;
mod requests;
mod search_jobs;
mod state;
mod stored;

fn clock_instant(service: &Service) -> tokio::time::Instant {
    tokio::time::Instant::from_std(service.clock.instant())
}

/// Serves requests on `swarm` until the task is dropped. The deny list is checked for changes
/// every 10 seconds and, on Unix, on SIGHUP.
pub async fn run(mut swarm: Swarm<Behaviour>, service: Arc<Service>) {
    let Some(state) = state::State::new(&mut swarm, &service) else {
        return;
    };
    Box::pin(event_loop(swarm, service, state)).await;
}

async fn event_loop(mut swarm: Swarm<Behaviour>, service: Arc<Service>, mut state: state::State) {
    loop {
        state.dispatch(&service);
        tokio::select! {
            event = swarm.select_next_some() => {
                events::handle(state.events(&mut swarm, &service), event);
            }
            event = maintenance::replication_event(&mut state.timers.put_check, &mut state.timers.republish_scan) => {
                maintenance::replication(&mut state, &mut swarm, &service, &event);
            }
            event = maintenance::deny_event(&mut state.timers.deny_check, &mut state.hangup) => {
                maintenance::deny(&mut state, &swarm, &service, &event);
            }
            _ = state.timers.rejoin.tick() => maintenance::rejoin(&state, &mut swarm),
            _ = state.timers.cleanup.tick() => {
                state.replicas.repaired.clear();
                stored::expire(&service);
            }
            Some(done) = state.channels.store_done.recv() => {
                finished::store(&mut state, &mut swarm, &service, done);
            }
            _ = state.timers.puzzle_tick.tick() => service.puzzle.tick(service.clock.instant()),
            Some(done) = state.channels.search_done.recv() => {
                finished::search(&mut state, &mut swarm, &service, done);
            }
            Some(done) = state.channels.pow_done.recv() => {
                finished::pow_done(&mut state, &mut swarm, &service, done);
            }
            () = tokio::time::sleep_until(state.timers.catch_up_at), if !state.catch_up_queue.is_empty() => {
                maintenance::send_queued(&mut state, &mut swarm, &service);
            }
            // Hourly, each kept topic's digest from two random search peers finds what gossip missed.
            _ = state.timers.catch_up.tick(), if service.search.is_some() => {
                maintenance::catch_up(&mut state, &mut swarm, &service);
            }
            _ = state.timers.maintain.tick() => {
                maintenance::tick(&mut state, &mut swarm, &service);
            }
        }
    }
}

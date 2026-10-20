//! Bounded background search work for the swarm loop.

use super::*;

pub(super) struct SearchJob {
    pub(super) from: Peer,
    pub(super) channel: ResponseChannel<SearchResponse>,
    pub(super) work: Search,
    pub(super) queued: std::time::Instant,
}

pub(super) struct SearchDone {
    pub(super) from: Peer,
    pub(super) channel: ResponseChannel<SearchResponse>,
    pub(super) get: bool,
    pub(super) reply: crate::Result<SearchResponse>,
}

const SEARCH_WORKERS: usize = 2;

pub(super) fn start_search(
    queue: &mut jobs::GroupQueue<SearchJob>,
    running: &mut usize,
    service: &Service,
    done: &tokio::sync::mpsc::UnboundedSender<SearchDone>,
) {
    let Some(index) = &service.search else { return };
    while *running < SEARCH_WORKERS {
        let Some(SearchJob {
            from,
            channel,
            work,
            queued,
        }) = queue.pop()
        else {
            return;
        };
        service.puzzle.record_wait(queued.elapsed());
        *running += 1;
        let (index, done) = (index.clone(), done.clone());
        let rows = service.config.search.max_scan_rows;
        tokio::task::spawn_blocking(move || {
            let (get, reply) = match work {
                Search::Get(keys) => (true, run_get(&index, &keys)),
                Search::Query(query) => (false, run_query(&index, &query, rows)),
                Search::Reply(_) | Search::Pow(..) => unreachable!("answered on the loop"),
            };
            let _ = done.send(SearchDone {
                from,
                channel,
                get,
                reply,
            });
        });
    }
}

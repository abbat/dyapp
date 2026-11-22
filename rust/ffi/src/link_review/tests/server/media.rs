use super::{callback, proto, Callback};
use std::collections::BTreeMap;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Mutex,
};

#[derive(Default)]
pub(crate) struct Media {
    pub(crate) blobs: Mutex<BTreeMap<Vec<u8>, Vec<u8>>>,
    pub(crate) on_get: Callback,
    pub(crate) reads: AtomicUsize,
}

impl Media {
    pub(super) fn reply(&self, request: proto::MediaRequest) -> proto::MediaResponse {
        let mut response = proto::MediaResponse {
            status: proto::Status::NotFound.into(),
            ..Default::default()
        };
        if let Some(proto::media_request::Request::Get(get)) = request.request {
            self.reads.fetch_add(1, Ordering::SeqCst);
            callback(&self.on_get);
            if let Some(bytes) = self.blobs.lock().unwrap().get(&get.hash) {
                response.status = proto::Status::Ok.into();
                response.data.clone_from(bytes);
            }
        }
        response
    }
}

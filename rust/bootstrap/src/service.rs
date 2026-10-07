//! Answers node protocol requests (`/dyapp/node`, `/dyapp/profile`) from the stores. The libp2p
//! loop in `dyapp-node` passes each request here and sends back the reply.

use crate::{
    config::Role, rate_limit::PeerRateLimiter, BootstrapError, BootstrapStore, NodeConfig,
};
use dyapp_identity::SignedRecord;
use dyapp_p2p_net::proto::{
    self, node_request, profile_request, NodeInfo, NodeRequest, NodeResponse, ProfileRequest,
    ProfileResponse, Status,
};

pub struct Service {
    pub store: BootstrapStore,
    pub rate_limiter: PeerRateLimiter,
    pub config: NodeConfig,
}

impl Service {
    pub fn node(&self, request: NodeRequest) -> NodeResponse {
        match request.request {
            Some(node_request::Request::Info(_)) => NodeResponse {
                status: Status::Ok.into(),
                info: Some(self.info()),
            },
            None => NodeResponse {
                status: Status::Unsupported.into(),
                info: None,
            },
        }
    }

    fn info(&self) -> NodeInfo {
        let store = self.config.roles.contains(&Role::Store);
        NodeInfo {
            roles: if store {
                vec![proto::Role::Store.into()]
            } else {
                vec![]
            },
            max_profile_bytes: if store {
                dyapp_profile::MAX_PAYLOAD_LEN as u64
            } else {
                0
            },
            ..NodeInfo::default()
        }
    }

    /// `peer` is the remote libp2p peer, the key for rate limiting. A storage failure is an
    /// error: the caller drops the request and the client tries another node.
    pub fn profile(&self, peer: &str, request: ProfileRequest) -> crate::Result<ProfileResponse> {
        if !self.config.roles.contains(&Role::Store) {
            return Ok(reply(Status::Unsupported, None));
        }
        if !self.rate_limiter.check_limit(peer) {
            return Ok(reply(Status::RateLimited, None));
        }
        match request.request {
            Some(profile_request::Request::Publish(record)) => self.publish(&record),
            Some(profile_request::Request::Get(get)) => {
                let Ok(peer_id) = <[u8; 32]>::try_from(get.peer_id.as_slice()) else {
                    return Ok(reply(Status::Invalid, None));
                };
                Ok(match self.store.get_profile(&hex(&peer_id))? {
                    Some(record) => reply(Status::Ok, Some(record)),
                    None => reply(Status::NotFound, None),
                })
            }
            None => Ok(reply(Status::Unsupported, None)),
        }
    }

    fn publish(&self, record: &SignedRecord) -> crate::Result<ProfileResponse> {
        if record.payload.len() > dyapp_profile::MAX_PAYLOAD_LEN {
            return Ok(reply(Status::TooLarge, None));
        }
        match self.store.put_profile(record) {
            Ok(_) => Ok(reply(Status::Ok, None)),
            Err(BootstrapError::Profile(dyapp_profile::Error::Stale)) => {
                // A stale record verified, so its key is a valid 32-byte key.
                let key: [u8; 32] = record.public_key.as_slice().try_into().unwrap_or_default();
                let stored = self.store.get_profile(&dyapp_identity::peer_id(&key))?;
                Ok(reply(Status::Stale, stored))
            }
            Err(BootstrapError::Profile(dyapp_profile::Error::Signature(_))) => {
                Ok(reply(Status::Denied, None))
            }
            Err(BootstrapError::Profile(_)) => Ok(reply(Status::Invalid, None)),
            Err(error) => Err(error),
        }
    }
}

fn reply(status: Status, record: Option<SignedRecord>) -> ProfileResponse {
    ProfileResponse {
        status: status.into(),
        record,
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use dyapp_identity::Identity;
    use dyapp_profile::Profile;

    fn service() -> Service {
        let dir = format!("/tmp/ai/test-service-{}", uuid::Uuid::new_v4());
        Service {
            store: BootstrapStore::new(&dir).unwrap(),
            rate_limiter: PeerRateLimiter::new(100),
            config: NodeConfig::default(),
        }
    }

    fn publish(record: SignedRecord) -> ProfileRequest {
        ProfileRequest {
            request: Some(profile_request::Request::Publish(record)),
        }
    }

    fn get(peer_id: Vec<u8>) -> ProfileRequest {
        ProfileRequest {
            request: Some(profile_request::Request::Get(proto::GetProfile { peer_id })),
        }
    }

    fn status(response: &ProfileResponse) -> Status {
        Status::try_from(response.status).unwrap()
    }

    #[test]
    fn publish_get_stale_and_errors() {
        let service = service();
        let identity = Identity::generate();
        let hex_id = identity.peer_id();
        let id: Vec<u8> = (0..hex_id.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex_id[i..i + 2], 16).unwrap())
            .collect();
        let v2 = Profile {
            version: 2,
            age: 30,
            ..Profile::default()
        }
        .sign(&identity);

        assert_eq!(
            status(&service.profile("p", get(id.clone())).unwrap()),
            Status::NotFound
        );
        assert_eq!(
            status(&service.profile("p", publish(v2.clone())).unwrap()),
            Status::Ok
        );
        let got = service.profile("p", get(id.clone())).unwrap();
        assert_eq!((status(&got), got.record), (Status::Ok, Some(v2.clone())));

        let v1 = Profile {
            version: 1,
            ..Profile::default()
        }
        .sign(&identity);
        let stale = service.profile("p", publish(v1)).unwrap();
        assert_eq!(
            (status(&stale), stale.record),
            (Status::Stale, Some(v2.clone()))
        );

        let mut forged = v2;
        forged.payload.push(0);
        assert_eq!(
            status(&service.profile("p", publish(forged)).unwrap()),
            Status::Denied
        );
        assert_eq!(
            status(&service.profile("p", get(vec![1; 5])).unwrap()),
            Status::Invalid
        );
        let empty = ProfileRequest { request: None };
        assert_eq!(
            status(&service.profile("p", empty).unwrap()),
            Status::Unsupported
        );
    }

    #[test]
    fn rate_limit_and_info() {
        let mut service = service();
        service.rate_limiter = PeerRateLimiter::new(1);
        let id = vec![0; 32];
        assert_eq!(
            status(&service.profile("p", get(id.clone())).unwrap()),
            Status::NotFound
        );
        assert_eq!(
            status(&service.profile("p", get(id.clone())).unwrap()),
            Status::RateLimited
        );
        assert_eq!(
            status(&service.profile("q", get(id)).unwrap()),
            Status::NotFound
        );

        let info = service.node(NodeRequest {
            request: Some(node_request::Request::Info(proto::InfoRequest {})),
        });
        assert_eq!(
            info.info.unwrap().roles,
            vec![i32::from(proto::Role::Store)]
        );
    }
}

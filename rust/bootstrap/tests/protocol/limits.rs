use super::*;

#[tokio::test]
async fn challenge_is_bound_to_its_connection() {
    let addr = start(100).await;
    let mut first = Client::connect(&addr).await;
    let mut second = Client::connect(&addr).await;
    let device = Identity::try_generate().unwrap();
    let nonce = first.challenge().await;
    second.challenge().await;
    let reply = second.mailbox(fetch(&device, nonce.clone())).await;
    assert_eq!(status(reply.status), Status::Denied);
    let reply = first.mailbox(fetch(&device, nonce)).await;
    assert_eq!(status(reply.status), Status::Ok);
}

#[tokio::test]
async fn bad_input_fails_only_its_request() {
    let addr = start(100).await;
    let profile = dyapp_p2p_net::PROFILE_PROTOCOL;
    // Not protobuf: field 1 with a length running past the end.
    assert_eq!(raw(&addr, profile.clone(), vec![0x0a, 0x7f, 1]).await, None);
    let oversized = vec![0; usize::try_from(MAX_CONTROL_MESSAGE_BYTES).unwrap() + 1];
    assert_eq!(
        raw(&addr, dyapp_p2p_net::MAILBOX_PROTOCOL, oversized).await,
        None
    );
    // Field 15 is a request variant from a newer client.
    let reply = raw(&addr, profile, vec![15 << 3 | 2, 0]).await.unwrap();
    let reply = proto::ProfileResponse::decode(reply.as_slice()).unwrap();
    assert_eq!(status(reply.status), Status::Unsupported);
    // An empty mailbox request decodes as no variant too.
    let reply = raw(&addr, dyapp_p2p_net::MAILBOX_PROTOCOL, vec![])
        .await
        .unwrap();
    let reply = proto::MailboxResponse::decode(reply.as_slice()).unwrap();
    assert_eq!(status(reply.status), Status::Unsupported);

    let mut client = Client::connect(&addr).await;
    assert_eq!(status(client.info().await.status), Status::Ok);
}

#[tokio::test]
async fn requests_over_the_limit_are_refused() {
    let addr = start(2).await;
    let mut client = Client::connect(&addr).await;
    let get = || {
        profile_request::Request::Get(proto::GetProfile {
            peer_id: vec![0; 32],
        })
    };
    for _ in 0..2 {
        assert_eq!(status(client.profile(get()).await.status), Status::NotFound);
    }
    assert_eq!(
        status(client.profile(get()).await.status),
        Status::RateLimited
    );
    // Node info is not limited.
    assert_eq!(status(client.info().await.status), Status::Ok);
}

#[tokio::test]
async fn connections_over_the_limit_are_refused() {
    let addr = start_with(|l| l.max_connections = 1).await;
    let mut first = Client::connect(&addr).await;
    let info = proto::NodeRequest {
        request: Some(node_request::Request::Info(proto::InfoRequest {})),
    };
    assert_eq!(
        raw(&addr, dyapp_p2p_net::NODE_PROTOCOL, info.encode_to_vec()).await,
        None
    );
    assert_eq!(status(first.info().await.status), Status::Ok);
}

#[tokio::test]
async fn connections_over_the_memory_limit_are_refused() {
    // The test process alone uses more than 1 MiB.
    let addr = start_with(|l| l.max_memory_mb = 1).await;
    let info = proto::NodeRequest {
        request: Some(node_request::Request::Info(proto::InfoRequest {})),
    };
    assert_eq!(
        raw(&addr, dyapp_p2p_net::NODE_PROTOCOL, info.encode_to_vec()).await,
        None
    );
}

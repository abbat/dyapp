use super::*;
use ureq::http::{HeaderValue, Request, Response};
use ureq::middleware::{Middleware, MiddlewareNext};

struct Reply {
    status: Option<u16>,
    etags: Vec<HeaderValue>,
    conditional: Option<&'static str>,
}

impl Middleware for Reply {
    fn handle(
        &self,
        request: Request<ureq::SendBody<'_>>,
        _next: MiddlewareNext<'_>,
    ) -> Result<Response<ureq::Body>, ureq::Error> {
        // Never continue the chain: no DNS, socket, TLS or external feed is involved.
        assert_eq!(request.uri().to_string(), FEED_URL);
        assert_eq!(request.method(), "GET");
        assert_eq!(
            request
                .headers()
                .get("if-none-match")
                .map(|value| value.to_str().unwrap()),
            self.conditional,
        );
        assert!(request.headers().get("authorization").is_none());
        assert!(request.headers().get("cookie").is_none());
        let status = self.status.ok_or(ureq::Error::ConnectionFailed)?;
        let mut response = Response::builder()
            .status(status)
            .body(ureq::Body::builder().data(BODY.to_vec()))
            .unwrap();
        for value in &self.etags {
            response.headers_mut().append("etag", value.clone());
        }
        Ok(response)
    }
}

fn client(
    status: Option<u16>,
    etags: Vec<HeaderValue>,
    conditional: Option<&'static str>,
) -> Client {
    Client {
        agent: ureq::Agent::config_builder()
            .middleware(Reply {
                status,
                etags,
                conditional,
            })
            .build()
            .new_agent(),
    }
}

#[test]
fn actual_client_get_and_conditional_reply_use_only_the_fixed_feed() {
    let first = client(Some(200), vec![HeaderValue::from_static("\"new\"")], None)
        .fetch(None)
        .unwrap();
    assert_eq!(first.bytes(), BODY);
    assert_eq!(first.etag(), Some("\"new\""));
    let unchanged = client(Some(304), Vec::new(), Some("\"new\""))
        .fetch(Some(&first))
        .unwrap();
    assert_eq!(unchanged.bytes(), BODY);
    assert_eq!(unchanged.etag(), first.etag());
    let replaced = client(
        Some(304),
        vec![HeaderValue::from_static("W/\"next\"")],
        Some("\"new\""),
    )
    .fetch(Some(&first))
    .unwrap();
    assert_eq!(replaced.etag(), Some("W/\"next\""));
    assert_eq!(first.etag(), Some("\"new\""));
}

#[test]
fn actual_response_headers_reject_duplicate_or_nontext_etags() {
    let saved = Cache::restore(BODY.to_vec(), None).unwrap();
    for etags in [
        vec![
            HeaderValue::from_static("\"a\""),
            HeaderValue::from_static("\"b\""),
        ],
        vec![HeaderValue::from_bytes(b"\"\xff\"").unwrap()],
    ] {
        assert!(matches!(
            client(Some(200), etags, None).fetch(Some(&saved)),
            Err(Error::Etag),
        ));
    }
    assert_eq!(saved.bytes(), BODY);
}

#[test]
fn actual_fetch_preserves_cache_on_status_network_or_unconfirmed_304() {
    let saved = Cache::restore(BODY.to_vec(), None).unwrap();
    for status in [302, 404, 500] {
        assert!(matches!(
            client(Some(status), Vec::new(), None).fetch(Some(&saved)),
            Err(Error::Status(actual)) if actual == status
        ));
    }
    assert!(matches!(
        client(None, Vec::new(), None).fetch(Some(&saved)),
        Err(Error::Network(_))
    ));
    assert!(matches!(
        client(Some(304), Vec::new(), None).fetch(None),
        Err(Error::NoCache)
    ));
    assert_eq!(saved.bytes(), BODY);
}

#[test]
fn production_configuration_disables_redirects_proxies_and_unbounded_io() {
    let client = Client::default();
    let config = client.agent.config();
    assert!(config.https_only());
    assert_eq!(config.max_redirects(), 0);
    assert!(!config.http_status_as_error());
    assert!(config.proxy().is_none());
    assert_eq!(config.timeouts().global, Some(Duration::from_secs(30)));
    assert!(
        matches!(config.user_agent(), ureq::config::AutoHeaderValue::Provided(value)
        if value.as_str() == "dyapp-client-updates")
    );
    assert!(
        matches!(config.accept(), ureq::config::AutoHeaderValue::Provided(value)
        if value.as_str() == "application/json")
    );
    assert!(matches!(
        config.accept_encoding(),
        ureq::config::AutoHeaderValue::None
    ));
}

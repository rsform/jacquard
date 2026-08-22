//! Full-stack replay against the containerized jetstream v2 server:
//! plan the sealed archive, download and decode a segment, and exchange
//! frames on the live `subscribeEvents` connection.
//!
//! Runs only under `scripts/e2e.sh jetstream`, which exports the
//! `JACQUARD_E2E_*` coordinates.

use jacquard::jetstream::archive::JetstreamClient;
use jacquard::jetstream::plan::{PlanCursor, ReplayFilters};
use jacquard_common::DefaultStr;
use jacquard_common::deps::fluent_uri::Uri;
use jacquard_common::jss::SegmentHeader;
use jacquard_e2e::provider::ProviderContext;
use jacquard_e2e::transport::{AllowedHost, FixtureTransport, TransportAllowlist};

fn transport_for(context: &ProviderContext) -> FixtureTransport {
    let base = context
        .coordinates
        .provider_url
        .parse::<http::Uri>()
        .expect("provider url");
    let host = base.host().expect("host").to_string();
    let port = base.port_u16().unwrap_or(80);
    let allowlist = TransportAllowlist::new([AllowedHost {
        host,
        port,
        scheme: "http",
    }]);
    FixtureTransport::new(reqwest::Client::new(), allowlist)
}

#[tokio::test]
async fn replay_against_real_server() {
    let context = ProviderContext::from_env().unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(context.provider.name(), "jetstream");

    let http = transport_for(&context);
    let base = Uri::parse(context.coordinates.provider_url.clone()).expect("base uri");
    let client = JetstreamClient::new(http, base, None);

    // Plan the sealed archive end to end.
    let filters = ReplayFilters::default();
    let mut cursor = PlanCursor::new(&filters, None, None);
    let mut segments = Vec::new();
    let mut sealed_tip = None;
    while let Some(page) = cursor
        .next_page(&client)
        .await
        .unwrap_or_else(|e| panic!("plan failed: {e}"))
    {
        let out = page
            .parse::<DefaultStr>()
            .unwrap_or_else(|e| panic!("plan page decode failed: {e}"));
        sealed_tip.get_or_insert(out.sealed_tip_seq);
        segments.extend(out.segments);
    }
    assert!(!segments.is_empty(), "sealed archive has segments");

    // Download and decode the first planned segment.
    let segment = &segments[0];
    let bytes = client
        .get_segment(segment.name.as_ref())
        .await
        .unwrap_or_else(|e| panic!("getSegment failed: {e}"));
    let header = SegmentHeader::decode(&bytes, bytes.len())
        .unwrap_or_else(|e| panic!("segment decode failed: {e}"));
    assert_eq!(header.version, 1);
    assert!(header.max_seq >= header.min_seq);

    // The live subscription exchanges frames: connect with a cursor at
    // the sealed tip and require at least one message.
    let ws = jacquard_common::websocket::tungstenite_client::TungsteniteClient::new();
    let mut stream = jacquard::jetstream::replay::ReplayStream::new(
        client,
        ws,
        ReplayFilters::default(),
        jacquard::jetstream::replay::ReplayMode::Replay { after_seq: None },
        jacquard::jetstream::replay::ReplayOptions::default(),
    );
    let sealed_tip = sealed_tip.expect("plan reported sealed tip");
    let live_seq = tokio::time::timeout(std::time::Duration::from_secs(120), async {
        loop {
            let item = stream
                .next()
                .await
                .unwrap_or_else(|e| panic!("replay failed: {e}"))
                .expect("stream remains live");
            if let Some(seq) = item.last_seq
                && seq > sealed_tip
            {
                return seq;
            }
        }
    })
    .await
    .expect("live event beyond sealed tip within 120s");
    assert!(live_seq > sealed_tip);
}

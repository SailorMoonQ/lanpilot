mod common;

use lanpilot_core::identity::Identity;
use lanpilot_core::proto::v1::{Hello, Os, PairRequest, StreamOpen, stream_open};
use lanpilot_core::session::{
    Opened, SessionError, accept_open, answer_hello, local_hello, open_session,
};
use lanpilot_core::version::{PROTO_MAX, VersionMismatch};

#[tokio::test]
async fn hello_exchange_negotiates_version() {
    let p = common::connected(&Identity::generate(), &Identity::generate()).await;
    let server_hello = local_hello("Desk", Os::Windows, "0.1.0", &[]);

    let server = tokio::spawn({
        let conn = p.server.clone();
        let server_hello = server_hello.clone();
        async move {
            let Opened::Session {
                mut send, hello, ..
            } = accept_open(&conn).await.unwrap()
            else {
                panic!("expected a session");
            };
            let version = answer_hello(&mut send, &server_hello, &hello)
                .await
                .unwrap();
            (hello, version)
        }
    });

    let client_hello = local_hello("iPhone", Os::Ios, "0.1.0", &["text"]);
    let session = open_session(&p.client, &client_hello).await.unwrap();
    let (seen, server_version) = server.await.unwrap();

    assert_eq!(session.version, PROTO_MAX);
    assert_eq!(server_version, PROTO_MAX);
    assert_eq!(session.server_hello.device_name, "Desk");
    assert_eq!(seen.device_name, "iPhone");
    assert_eq!(seen.capabilities, vec!["text".to_owned()]);
}

#[tokio::test]
async fn version_mismatch_is_reported_on_both_sides() {
    // The server handler drops its connection as soon as `answer_hello`
    // returns; the client must still read the server Hello and report
    // `PeerTooOld`, not a connection error.
    for _ in 0..10 {
        let common::Pair {
            client,
            server: server_conn,
            client_endpoint: _client_endpoint,
            server_endpoint: _server_endpoint,
        } = common::connected(&Identity::generate(), &Identity::generate()).await;

        let server = tokio::spawn(async move {
            let Opened::Session {
                mut send, hello, ..
            } = accept_open(&server_conn).await.unwrap()
            else {
                panic!("expected a session");
            };
            let local = local_hello("Desk", Os::Linux, "0.1.0", &[]);
            let result = answer_hello(&mut send, &local, &hello).await;
            drop(server_conn);
            result
        });

        let future_client = Hello {
            proto_min: 99,
            proto_max: 99,
            ..Default::default()
        };
        let client_result = open_session(&client, &future_client).await;
        assert!(
            matches!(
                client_result,
                Err(SessionError::Version(VersionMismatch::PeerTooOld))
            ),
            "{:?}",
            client_result.err()
        );
        assert!(matches!(
            server.await.unwrap(),
            Err(SessionError::Version(VersionMismatch::PeerTooNew))
        ));
    }
}

#[tokio::test]
async fn pair_request_is_recognized() {
    let p = common::connected(&Identity::generate(), &Identity::generate()).await;
    let (mut send, _recv) = p.client.open_bi().await.unwrap();
    let open = StreamOpen {
        kind: Some(stream_open::Kind::Pair(PairRequest {
            device_name: "iPhone".into(),
            ..Default::default()
        })),
    };
    lanpilot_core::framing::write_msg(&mut send, &open)
        .await
        .unwrap();

    match accept_open(&p.server).await.unwrap() {
        Opened::Pair { request, .. } => assert_eq!(request.device_name, "iPhone"),
        Opened::Session { .. } => panic!("expected a pair request"),
    }
}

#[tokio::test]
async fn empty_stream_open_is_rejected() {
    let p = common::connected(&Identity::generate(), &Identity::generate()).await;
    let (mut send, _recv) = p.client.open_bi().await.unwrap();
    lanpilot_core::framing::write_msg(&mut send, &StreamOpen { kind: None })
        .await
        .unwrap();
    assert!(matches!(
        accept_open(&p.server).await,
        Err(SessionError::Unexpected(_))
    ));
}

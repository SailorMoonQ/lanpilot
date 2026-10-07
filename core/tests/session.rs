mod common;

use lanpilot_core::identity::Identity;
use lanpilot_core::proto::v1::{Hello, Os, PairRequest, StreamOpen, stream_open};
use lanpilot_core::session::{
    Opened, SessionError, accept_authorized, accept_open, answer_hello, connect_paired,
    local_hello, open_session,
};
use lanpilot_core::transport::{client_endpoint, server_endpoint};
use lanpilot_core::version::{PROTO_MAX, VersionMismatch};
use std::time::Duration;

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
async fn received_hello_names_are_sanitized() {
    let p = common::connected(&Identity::generate(), &Identity::generate()).await;
    let dirty = |name: &str| Hello {
        proto_min: 1,
        proto_max: 1,
        device_name: name.into(),
        ..Default::default()
    };

    let server = tokio::spawn({
        let conn = p.server.clone();
        let server_hello = dirty("\u{202E}Desk\u{0}");
        async move {
            let Opened::Session {
                mut send, hello, ..
            } = accept_open(&conn).await.unwrap()
            else {
                panic!("expected a session");
            };
            answer_hello(&mut send, &server_hello, &hello)
                .await
                .unwrap();
            hello
        }
    });

    let session = open_session(&p.client, &dirty("\u{2066}iPhone\n\u{200B}"))
        .await
        .unwrap();
    let seen = server.await.unwrap();
    assert_eq!(session.server_hello.device_name, "Desk");
    assert_eq!(seen.device_name, "iPhone");
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

fn pair_open(name: &str) -> StreamOpen {
    StreamOpen {
        kind: Some(stream_open::Kind::Pair(PairRequest {
            device_name: name.into(),
            ..Default::default()
        })),
    }
}

#[tokio::test]
async fn unpaired_hello_is_rejected() {
    let p = common::connected(&Identity::generate(), &Identity::generate()).await;
    let server = tokio::spawn({
        let conn = p.server.clone();
        async move { accept_authorized(&conn, |_| false).await }
    });

    let hello = local_hello("iPhone", Os::Ios, "0.1.0", &[]);
    let opened = tokio::time::timeout(Duration::from_secs(5), open_session(&p.client, &hello))
        .await
        .expect("client gave up within 5 s");
    assert!(opened.is_err());
    assert!(matches!(
        server.await.unwrap(),
        Err(SessionError::NotPaired)
    ));
    let reason = tokio::time::timeout(Duration::from_secs(5), p.client.closed())
        .await
        .expect("server closed the connection");
    assert!(
        matches!(&reason, lanpilot_core::quinn::ConnectionError::ApplicationClosed(c)
            if c.error_code == lanpilot_core::session::CLOSE_NOT_PAIRED.into()),
        "{reason:?}"
    );
}

#[tokio::test]
async fn paired_hello_is_accepted() {
    let client_id = Identity::generate();
    let client_key = client_id.public_key();
    let p = common::connected(&Identity::generate(), &client_id).await;
    let server = tokio::spawn({
        let conn = p.server.clone();
        async move {
            let Opened::Session {
                mut send, hello, ..
            } = accept_authorized(&conn, |k| *k == client_key)
                .await
                .unwrap()
            else {
                panic!("expected a session");
            };
            let local = local_hello("Desk", Os::Linux, "0.1.0", &[]);
            answer_hello(&mut send, &local, &hello).await.unwrap()
        }
    });

    let hello = local_hello("iPhone", Os::Ios, "0.1.0", &[]);
    let session = tokio::time::timeout(Duration::from_secs(5), open_session(&p.client, &hello))
        .await
        .expect("session opened within 5 s")
        .unwrap();
    assert_eq!(server.await.unwrap(), session.version);
    assert_eq!(session.server_hello.device_name, "Desk");
}

#[tokio::test]
async fn unpaired_pair_request_is_allowed() {
    let p = common::connected(&Identity::generate(), &Identity::generate()).await;
    let (mut send, _recv) = p.client.open_bi().await.unwrap();
    lanpilot_core::framing::write_msg(&mut send, &pair_open("iPhone"))
        .await
        .unwrap();
    match accept_authorized(&p.server, |_| false).await.unwrap() {
        Opened::Pair { request, .. } => assert_eq!(request.device_name, "iPhone"),
        Opened::Session { .. } => panic!("expected a pair request"),
    }
}

#[tokio::test]
async fn connect_paired_rejects_wrong_server_before_sending() {
    let server_id = Identity::generate();
    let server = server_endpoint(&server_id, "127.0.0.1:0".parse().unwrap()).unwrap();
    let addr = server.local_addr().unwrap();
    let accept = tokio::spawn({
        let server = server.clone();
        async move {
            let conn = server.accept().await.unwrap().await.unwrap();
            // The client must close before opening any stream.
            conn.accept_bi().await.is_err()
        }
    });

    let client = client_endpoint(&Identity::generate()).unwrap();
    let wrong = Identity::generate().public_key();
    let err = connect_paired(&client, addr, &wrong).await.unwrap_err();
    assert!(matches!(err, SessionError::UnexpectedPeer), "{err:?}");
    assert!(accept.await.unwrap(), "server must not receive a stream");

    // The right key connects.
    let accept = tokio::spawn(async move { server.accept().await.unwrap().await.unwrap() });
    let conn = connect_paired(&client, addr, &server_id.public_key())
        .await
        .unwrap();
    accept.await.unwrap();
    conn.close(0u32.into(), b"done");
}

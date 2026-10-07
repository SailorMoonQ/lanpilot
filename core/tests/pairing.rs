use lanpilot_core::framing::{read_msg, write_msg};
use lanpilot_core::identity::{Identity, PublicKey};
use lanpilot_core::pairing::flow::{
    NewDevice, PairingAuthority, PairingFlowError, pair_with_invite, pair_with_password,
    serve_pairing,
};
use lanpilot_core::pairing::invite::Invite;
use lanpilot_core::pairing::password::{ClientHandshake, MAX_FAILURES, PasswordAttempts};
use lanpilot_core::pairing::tokens::TokenStore;
use lanpilot_core::proto::v1::{
    Os, PairConfirm, PairRejectReason, PairRequest, PairResult, PairServerMessage, PasswordPairing,
    StreamOpen, pair_request, pair_server_message, stream_open,
};
use lanpilot_core::quinn;
use lanpilot_core::session::{Opened, accept_open};
use lanpilot_core::transport::{client_endpoint, connect, server_endpoint};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct TestAuthority {
    tokens: Mutex<TokenStore>,
    attempts: Mutex<PasswordAttempts>,
    password: Option<String>,
    approve: bool,
    approved: Mutex<Vec<String>>,
}

impl TestAuthority {
    fn new(password: Option<&str>, approve: bool) -> Arc<Self> {
        Arc::new(Self {
            tokens: Mutex::new(TokenStore::new()),
            attempts: Mutex::new(PasswordAttempts::new()),
            password: password.map(str::to_owned),
            approve,
            approved: Mutex::new(Vec::new()),
        })
    }

    fn issue_token(&self) -> [u8; 16] {
        self.tokens.lock().unwrap().issue(Instant::now())
    }
}

impl PairingAuthority for TestAuthority {
    fn server_name(&self) -> String {
        "Desk".into()
    }
    fn server_os(&self) -> Os {
        Os::Linux
    }
    fn consume_token(&self, token: &[u8]) -> bool {
        self.tokens.lock().unwrap().consume(token, Instant::now())
    }
    fn password(&self) -> Option<String> {
        self.password.clone()
    }
    fn password_begin(&self) -> Result<(), Duration> {
        self.attempts.lock().unwrap().begin(Instant::now())
    }
    fn password_succeeded(&self) {
        self.attempts.lock().unwrap().record_success();
    }
    fn approve(&self, device: &NewDevice) -> impl Future<Output = bool> + Send {
        if self.approve {
            self.approved.lock().unwrap().push(device.name.clone());
        }
        let ok = self.approve;
        async move { ok }
    }
}

struct Server {
    id: Identity,
    endpoint: quinn::Endpoint,
    addr: SocketAddr,
}

fn server() -> Server {
    let id = Identity::generate();
    let endpoint = server_endpoint(&id, "127.0.0.1:0".parse().unwrap()).unwrap();
    let addr = endpoint.local_addr().unwrap();
    Server { id, endpoint, addr }
}

/// Accepts one pairing connection and serves it, then drops the connection at
/// once (like a real server handler would), with no grace period for the client.
fn serve_one(
    s: &Server,
    auth: Arc<TestAuthority>,
) -> tokio::task::JoinHandle<Result<Option<NewDevice>, PairingFlowError>> {
    let endpoint = s.endpoint.clone();
    let key = s.id.public_key();
    tokio::spawn(async move {
        let conn = endpoint.accept().await.unwrap().await.unwrap();
        let Opened::Pair {
            send,
            recv,
            request,
        } = accept_open(&conn).await.unwrap()
        else {
            panic!("expected pairing");
        };
        let result = serve_pairing(&conn, &key, send, recv, request, auth.as_ref()).await;
        drop(conn);
        result
    })
}

fn invite_for(s: &Server, token: [u8; 16], key: PublicKey) -> Invite {
    Invite {
        addrs: vec![s.addr.ip()],
        port: s.addr.port(),
        server_public_key: key,
        token,
        server_name: "Desk".into(),
    }
}

#[tokio::test]
async fn qr_pairing_succeeds() {
    let s = server();
    let auth = TestAuthority::new(None, true);
    let invite = invite_for(&s, auth.issue_token(), s.id.public_key());
    let task = serve_one(&s, auth.clone());

    let client_id = Identity::generate();
    let client = client_endpoint(&client_id).unwrap();
    let paired = pair_with_invite(&client, &invite, "iPhone", Os::Ios)
        .await
        .unwrap();

    assert_eq!(paired.public_key, s.id.public_key());
    assert_eq!(paired.name, "Desk");
    assert_eq!(paired.os, Os::Linux);
    let device = task.await.unwrap().unwrap().unwrap();
    assert_eq!(device.public_key, client_id.public_key());
    assert_eq!(device.os, Os::Ios);
    assert_eq!(*auth.approved.lock().unwrap(), vec!["iPhone".to_owned()]);
}

#[tokio::test]
async fn accepted_result_survives_immediate_connection_drop() {
    // Regression: the server handler drops the connection right after
    // `serve_pairing` returns. The client must still receive the PairResult.
    for _ in 0..10 {
        let s = server();
        let auth = TestAuthority::new(None, true);
        let invite = invite_for(&s, auth.issue_token(), s.id.public_key());
        let task = serve_one(&s, auth.clone());
        let client = client_endpoint(&Identity::generate()).unwrap();
        let paired = pair_with_invite(&client, &invite, "iPhone", Os::Ios).await;
        assert_eq!(paired.unwrap().name, "Desk");
        assert!(task.await.unwrap().unwrap().is_some());
    }
}

#[tokio::test]
async fn device_name_is_sanitized_before_approval() {
    let s = server();
    let auth = TestAuthority::new(None, true);
    let invite = invite_for(&s, auth.issue_token(), s.id.public_key());
    let task = serve_one(&s, auth.clone());
    let client = client_endpoint(&Identity::generate()).unwrap();
    pair_with_invite(&client, &invite, " \u{202E}iPhone\u{0}\n", Os::Ios)
        .await
        .unwrap();
    let device = task.await.unwrap().unwrap().unwrap();
    assert_eq!(device.name, "iPhone");
    assert_eq!(*auth.approved.lock().unwrap(), vec!["iPhone".to_owned()]);
}

#[tokio::test]
async fn qr_token_cannot_be_reused() {
    let s = server();
    let auth = TestAuthority::new(None, true);
    let invite = invite_for(&s, auth.issue_token(), s.id.public_key());
    let client = client_endpoint(&Identity::generate()).unwrap();

    let first = serve_one(&s, auth.clone());
    pair_with_invite(&client, &invite, "iPhone", Os::Ios)
        .await
        .unwrap();
    first.await.unwrap().unwrap();

    let second = serve_one(&s, auth.clone());
    let err = pair_with_invite(&client, &invite, "iPhone", Os::Ios)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        PairingFlowError::Rejected {
            reason: PairRejectReason::BadToken,
            ..
        }
    ));
    assert!(second.await.unwrap().unwrap().is_none());
}

#[tokio::test]
async fn qr_with_wrong_server_key_never_reveals_token() {
    let s = server();
    let auth = TestAuthority::new(None, true);
    let token = auth.issue_token();
    let invite = invite_for(&s, token, PublicKey([9u8; 32]));
    let accept = {
        let endpoint = s.endpoint.clone();
        tokio::spawn(async move {
            let conn = endpoint.accept().await.unwrap().await.unwrap();
            accept_open(&conn).await.is_err()
        })
    };

    let client = client_endpoint(&Identity::generate()).unwrap();
    let err = pair_with_invite(&client, &invite, "iPhone", Os::Ios)
        .await
        .unwrap_err();
    assert!(matches!(err, PairingFlowError::ServerKeyMismatch));
    assert!(
        accept.await.unwrap(),
        "server must not receive a pairing request"
    );
    assert!(auth.consume_token(&token), "token must still be unused");
}

#[tokio::test]
async fn password_pairing_succeeds() {
    let s = server();
    let auth = TestAuthority::new(Some("hunter22"), true);
    let task = serve_one(&s, auth.clone());

    let client_id = Identity::generate();
    let client = client_endpoint(&client_id).unwrap();
    let paired = pair_with_password(
        &client,
        s.addr,
        &client_id.public_key(),
        "hunter22",
        "iPhone",
        Os::Ios,
    )
    .await
    .unwrap();

    assert_eq!(paired.public_key, s.id.public_key());
    let device = task.await.unwrap().unwrap().unwrap();
    assert_eq!(device.public_key, client_id.public_key());
}

#[tokio::test]
async fn wrong_password_fails_and_eventually_locks() {
    let s = server();
    let auth = TestAuthority::new(Some("hunter22"), true);
    let client_id = Identity::generate();
    let client = client_endpoint(&client_id).unwrap();

    for _ in 0..MAX_FAILURES {
        let task = serve_one(&s, auth.clone());
        let err = pair_with_password(
            &client,
            s.addr,
            &client_id.public_key(),
            "wrong-pw",
            "iPhone",
            Os::Ios,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, PairingFlowError::WrongPassword), "{err:?}");
        // The client hangs up instead of confirming; the server sees an error,
        // and the attempt was counted when it began.
        assert!(task.await.unwrap().is_err());
    }

    // Locked now, even with the right password.
    let task = serve_one(&s, auth.clone());
    let err = pair_with_password(
        &client,
        s.addr,
        &client_id.public_key(),
        "hunter22",
        "iPhone",
        Os::Ios,
    )
    .await
    .unwrap_err();
    match err {
        PairingFlowError::Rejected {
            reason,
            retry_after,
        } => {
            assert_eq!(reason, PairRejectReason::Locked);
            assert!(retry_after > Duration::from_secs(290));
        }
        other => panic!("expected locked, got {other:?}"),
    }
    assert!(task.await.unwrap().unwrap().is_none());
    assert!(auth.approved.lock().unwrap().is_empty());
}

#[tokio::test]
async fn password_pairing_disabled() {
    let s = server();
    let auth = TestAuthority::new(None, true);
    let task = serve_one(&s, auth.clone());
    let client_id = Identity::generate();
    let client = client_endpoint(&client_id).unwrap();
    let err = pair_with_password(
        &client,
        s.addr,
        &client_id.public_key(),
        "hunter22",
        "iPhone",
        Os::Ios,
    )
    .await
    .unwrap_err();
    assert!(matches!(
        err,
        PairingFlowError::Rejected {
            reason: PairRejectReason::Disabled,
            ..
        }
    ));
    assert!(task.await.unwrap().unwrap().is_none());
}

#[tokio::test]
async fn denied_by_authority() {
    let s = server();
    let auth = TestAuthority::new(None, false);
    let invite = invite_for(&s, auth.issue_token(), s.id.public_key());
    let task = serve_one(&s, auth.clone());
    let client = client_endpoint(&Identity::generate()).unwrap();
    let err = pair_with_invite(&client, &invite, "iPhone", Os::Ios)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        PairingFlowError::Rejected {
            reason: PairRejectReason::Denied,
            ..
        }
    ));
    assert!(task.await.unwrap().unwrap().is_none());
}

#[tokio::test]
async fn parallel_password_attempts_cannot_exceed_limit() {
    let s = server();
    let auth = TestAuthority::new(Some("hunter22"), true);
    let total = MAX_FAILURES as usize + 3;

    let tasks: Vec<_> = (0..total).map(|_| serve_one(&s, auth.clone())).collect();
    let mut clients = Vec::new();
    for _ in 0..total {
        let addr = s.addr;
        clients.push(tokio::spawn(async move {
            let id = Identity::generate();
            let client = client_endpoint(&id).unwrap();
            pair_with_password(
                &client,
                addr,
                &id.public_key(),
                "wrong-pw",
                "iPhone",
                Os::Ios,
            )
            .await
        }));
    }

    let mut wrong = 0;
    let mut locked = 0;
    for c in clients {
        match c.await.unwrap().unwrap_err() {
            PairingFlowError::WrongPassword => wrong += 1,
            PairingFlowError::Rejected {
                reason: PairRejectReason::Locked,
                ..
            } => locked += 1,
            other => panic!("unexpected result: {other:?}"),
        }
    }
    for t in tasks {
        let _ = t.await.unwrap();
    }
    // Reservations are atomic, so exactly MAX_FAILURES attempts ran SPAKE2.
    assert_eq!(
        wrong, MAX_FAILURES as usize,
        "{wrong} wrong-password results"
    );
    assert_eq!(locked, 3);
    assert!(auth.approved.lock().unwrap().is_empty());
}

/// A client that runs SPAKE2 with `password`, ignores the server's
/// confirmation and sends a garbage `PairConfirm` (or none, if the server
/// answers the request with a result right away). Returns the final result.
async fn malicious_password_client(addr: SocketAddr, password: &str) -> PairResult {
    let client = client_endpoint(&Identity::generate()).unwrap();
    let conn = connect(&client, addr).await.unwrap();
    let (mut send, mut recv) = conn.open_bi().await.unwrap();
    let (_handshake, spake_msg) = ClientHandshake::start(password);
    let open = StreamOpen {
        kind: Some(stream_open::Kind::Pair(PairRequest {
            device_name: "Mallory".into(),
            os: Os::Android as i32,
            method: Some(pair_request::Method::Password(PasswordPairing {
                spake_msg,
            })),
        })),
    };
    write_msg(&mut send, &open).await.unwrap();

    let first: PairServerMessage = read_msg(&mut recv).await.unwrap().unwrap();
    let result = match first.body {
        Some(pair_server_message::Body::Result(r)) => r,
        Some(pair_server_message::Body::Challenge(_)) => {
            let garbage = PairConfirm {
                client_confirm: vec![0xAA; 32],
            };
            write_msg(&mut send, &garbage).await.unwrap();
            send.finish().unwrap();
            let msg: PairServerMessage = read_msg(&mut recv).await.unwrap().unwrap();
            match msg.body {
                Some(pair_server_message::Body::Result(r)) => r,
                other => panic!("expected a result, got {other:?}"),
            }
        }
        None => panic!("empty PairServerMessage"),
    };
    conn.close(0u32.into(), b"done");
    result
}

#[tokio::test]
async fn garbage_confirmation_is_rejected_and_counted() {
    let s = server();
    let auth = TestAuthority::new(Some("hunter22"), true);

    for _ in 0..MAX_FAILURES {
        let task = serve_one(&s, auth.clone());
        let result = malicious_password_client(s.addr, "wrong-pw").await;
        assert!(!result.accepted);
        assert_eq!(result.reason, PairRejectReason::BadPassword as i32);
        assert!(task.await.unwrap().unwrap().is_none());
    }

    // Every garbage confirmation counted: now locked, even with the right password.
    let task = serve_one(&s, auth.clone());
    let result = malicious_password_client(s.addr, "hunter22").await;
    assert!(!result.accepted);
    assert_eq!(result.reason, PairRejectReason::Locked as i32);
    assert!(result.retry_after_secs > 0);
    assert!(task.await.unwrap().unwrap().is_none());
    assert!(auth.approved.lock().unwrap().is_empty());
}

use lanpilot_core::framing::{read_msg, write_msg};
use lanpilot_core::identity::Identity;
use lanpilot_core::proto::v1::Hello;
use lanpilot_core::transport::{client_endpoint, connect, peer_public_key, server_endpoint};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::UdpSocket;

fn localhost() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}

#[tokio::test]
async fn both_sides_see_each_others_public_key() {
    let server_id = Identity::generate();
    let client_id = Identity::generate();
    let server = server_endpoint(&server_id, localhost()).unwrap();
    let addr = server.local_addr().unwrap();
    let client = client_endpoint(&client_id).unwrap();

    let accept = tokio::spawn(async move {
        let conn = server.accept().await.unwrap().await.unwrap();
        (peer_public_key(&conn).unwrap(), conn, server)
    });
    let conn = connect(&client, addr).await.unwrap();
    let (seen_by_server, _server_conn, _server) = accept.await.unwrap();

    assert_eq!(peer_public_key(&conn).unwrap(), server_id.public_key());
    assert_eq!(seen_by_server, client_id.public_key());
}

#[tokio::test]
async fn streams_and_datagrams_work() {
    let server = server_endpoint(&Identity::generate(), localhost()).unwrap();
    let addr = server.local_addr().unwrap();
    let client = client_endpoint(&Identity::generate()).unwrap();

    let server_task = tokio::spawn(async move {
        let conn = server.accept().await.unwrap().await.unwrap();
        let (mut send, mut recv) = conn.accept_bi().await.unwrap();
        let hello: Hello = read_msg(&mut recv).await.unwrap().unwrap();
        write_msg(&mut send, &hello).await.unwrap();
        send.finish().unwrap();
        let datagram = conn.read_datagram().await.unwrap();
        conn.send_datagram(datagram).unwrap();
        // Keep the connection alive until the client is done.
        conn.closed().await;
    });

    let conn = connect(&client, addr).await.unwrap();
    let (mut send, mut recv) = conn.open_bi().await.unwrap();
    let hello = Hello {
        proto_min: 1,
        proto_max: 1,
        device_name: "test".into(),
        ..Default::default()
    };
    write_msg(&mut send, &hello).await.unwrap();
    let echoed: Hello = read_msg(&mut recv).await.unwrap().unwrap();
    assert_eq!(echoed, hello);

    conn.send_datagram(vec![1u8, 2, 3].into()).unwrap();
    let back = tokio::time::timeout(Duration::from_secs(2), conn.read_datagram())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&back[..], &[1, 2, 3]);

    conn.close(0u32.into(), b"done");
    server_task.await.unwrap();
}

/// UDP relay between client and server that can be cut, simulating the phone
/// dropping off Wi-Fi without closing the connection.
/// Returns the relay address, the cut switch and the relay tasks (abort them when done).
async fn cuttable_relay(
    server: SocketAddr,
) -> (
    SocketAddr,
    Arc<AtomicBool>,
    [tokio::task::JoinHandle<()>; 2],
) {
    let front = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let back = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    back.connect(server).await.unwrap();
    let front_addr = front.local_addr().unwrap();
    let cut = Arc::new(AtomicBool::new(false));
    let client: Arc<Mutex<Option<SocketAddr>>> = Arc::new(Mutex::new(None));

    let forward = {
        let (front, back, cut, client) = (front.clone(), back.clone(), cut.clone(), client.clone());
        tokio::spawn(async move {
            let mut buf = vec![0u8; 65536];
            loop {
                // Windows reports ICMP port-unreachable as a recv error; keep relaying.
                let Ok((n, from)) = front.recv_from(&mut buf).await else {
                    continue;
                };
                *client.lock().unwrap() = Some(from);
                if !cut.load(Ordering::SeqCst) {
                    let _ = back.send(&buf[..n]).await;
                }
            }
        })
    };
    let backward = {
        let cut = cut.clone();
        tokio::spawn(async move {
            let mut buf = vec![0u8; 65536];
            loop {
                let Ok(n) = back.recv(&mut buf).await else {
                    continue;
                };
                let to = *client.lock().unwrap();
                if let (false, Some(to)) = (cut.load(Ordering::SeqCst), to) {
                    let _ = front.send_to(&buf[..n], to).await;
                }
            }
        })
    };
    (front_addr, cut, [forward, backward])
}

#[tokio::test]
async fn vanished_peer_is_detected_within_idle_timeout() {
    let server = server_endpoint(&Identity::generate(), localhost()).unwrap();
    let (relay_addr, cut, relay_tasks) = cuttable_relay(server.local_addr().unwrap()).await;
    let client = client_endpoint(&Identity::generate()).unwrap();

    let server_task = tokio::spawn(async move {
        let conn = server.accept().await.unwrap().await.unwrap();
        conn.closed().await;
        tokio::time::Instant::now()
    });

    let conn = connect(&client, relay_addr).await.unwrap();
    // Let a couple of keep-alives pass to prove they keep the connection open.
    tokio::time::sleep(Duration::from_millis(3500)).await;
    assert!(
        conn.close_reason().is_none(),
        "keep-alive should hold the connection"
    );

    let cut_at = tokio::time::Instant::now();
    cut.store(true, Ordering::SeqCst);
    let closed_at = tokio::time::timeout(Duration::from_secs(10), server_task)
        .await
        .expect("server noticed within 10 s")
        .unwrap();
    let elapsed = closed_at - cut_at;
    assert!(elapsed < Duration::from_secs(5), "took {elapsed:?}");
    for task in relay_tasks {
        task.abort();
    }
}

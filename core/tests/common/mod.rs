#![allow(dead_code)]

use lanpilot_core::identity::Identity;
use lanpilot_core::quinn;
use lanpilot_core::transport::{client_endpoint, connect, server_endpoint};

pub struct Pair {
    pub client: quinn::Connection,
    pub server: quinn::Connection,
    pub client_endpoint: quinn::Endpoint,
    pub server_endpoint: quinn::Endpoint,
}

pub async fn connected(server_id: &Identity, client_id: &Identity) -> Pair {
    let server_endpoint = server_endpoint(server_id, "127.0.0.1:0".parse().unwrap()).unwrap();
    let addr = server_endpoint.local_addr().unwrap();
    let client_endpoint = client_endpoint(client_id).unwrap();
    let accepting = {
        let ep = server_endpoint.clone();
        tokio::spawn(async move { ep.accept().await.unwrap().await.unwrap() })
    };
    let client = connect(&client_endpoint, addr).await.unwrap();
    let server = accepting.await.unwrap();
    Pair {
        client,
        server,
        client_endpoint,
        server_endpoint,
    }
}

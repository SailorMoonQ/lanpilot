//! Requests on the control stream. One reader task owns the receive side
//! (`read_msg` is not cancel-safe) and routes each reply to its waiter by
//! request id; writers take turns through an async lock.

use crate::api::types::{BridgeError, ErrorKind};
use lanpilot_core::framing::{read_msg, write_msg};
use lanpilot_core::proto::v1::{ClientMessage, ServerMessage, client_message, server_message};
use lanpilot_core::quinn;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use tokio::sync::oneshot;

pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

type Waiters = Arc<Mutex<HashMap<u64, oneshot::Sender<ServerMessage>>>>;

pub struct Control {
    send: tokio::sync::Mutex<quinn::SendStream>,
    waiters: Waiters,
    next_id: AtomicU64,
    reader: tokio::task::JoinHandle<()>,
}

impl Control {
    /// Must be called inside a tokio runtime (spawns the reader task).
    pub fn start(send: quinn::SendStream, mut recv: quinn::RecvStream) -> Self {
        let waiters: Waiters = Arc::default();
        let routes = waiters.clone();
        let reader = tokio::spawn(async move {
            while let Ok(Some(reply)) = read_msg::<ServerMessage, _>(&mut recv).await {
                let waiter = routes
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .remove(&reply.request_id);
                if let Some(waiter) = waiter {
                    let _ = waiter.send(reply);
                }
            }
            // Dropping the senders fails every request still waiting.
            routes
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clear();
        });
        Self {
            send: tokio::sync::Mutex::new(send),
            waiters,
            next_id: AtomicU64::new(0),
            reader,
        }
    }

    pub async fn request(&self, body: client_message::Body) -> Result<(), BridgeError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let (tx, rx) = oneshot::channel();
        self.waiters
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id, tx);
        let message = ClientMessage {
            request_id: id,
            body: Some(body),
        };
        let written = {
            let mut send = self.send.lock().await;
            write_msg(&mut *send, &message).await
        };
        if let Err(e) = written {
            self.forget(id);
            return Err(BridgeError::new(ErrorKind::Closed, e.to_string()));
        }
        match tokio::time::timeout(REQUEST_TIMEOUT, rx).await {
            Ok(Ok(reply)) => check_reply(reply),
            Ok(Err(_)) => Err(BridgeError::new(
                ErrorKind::Closed,
                "the control stream ended",
            )),
            Err(_) => {
                self.forget(id);
                Err(BridgeError::new(ErrorKind::Timeout, "no reply within 5 s"))
            }
        }
    }

    /// Finishes our side of the stream (best effort).
    pub async fn finish(&self) {
        let _ = self.send.lock().await.finish();
    }

    fn forget(&self, id: u64) {
        self.waiters
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&id);
    }
}

impl Drop for Control {
    fn drop(&mut self) {
        self.reader.abort();
    }
}

fn check_reply(reply: ServerMessage) -> Result<(), BridgeError> {
    match reply.body {
        Some(server_message::Body::Ack(_)) => Ok(()),
        Some(server_message::Body::Error(e)) => {
            Err(BridgeError::new(ErrorKind::RequestFailed, e.message))
        }
        _ => Err(BridgeError::new(ErrorKind::Internal, "unexpected reply")),
    }
}

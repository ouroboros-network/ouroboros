// src/network/bft_msg.rs
use crate::bft::consensus::{HotStuff, Proposal, QuorumCertificate, Vote};
use log::warn;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

pub async fn start_bft_server(addr: SocketAddr, hotstuff: Arc<HotStuff>) -> anyhow::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    log::info!("BFT server listening on {}", addr);

    loop {
        let (stream, _) = listener.accept().await?;
        let hs = hotstuff.clone();
        tokio::spawn(async move {
            let handler = move |msg: BftMessage| -> anyhow::Result<()> {
                let hs_clone = hs.clone();
                tokio::spawn(async move {
                    let res = match msg {
                        BftMessage::Proposal(p) => hs_clone.handle_proposal(p).await,
                        BftMessage::Vote(v) => hs_clone
                            .handle_vote(v)
                            .await
                            .map_err(|e| anyhow::anyhow!(e)),
                        BftMessage::QC(qc) => {
                            hs_clone.handle_qc(qc).await.map_err(|e| anyhow::anyhow!(e))
                        }
                        _ => {
                            log::warn!("unhandled message type");
                            Ok(())
                        }
                    };
                    if let Err(e) = res {
                        log::error!("error handling bft message: {}", e);
                    }
                });
                Ok(())
            };
            handle_stream(stream, handler).await;
        });
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum BftMessage {
    Proposal(Proposal),
    Vote(Vote),
    QC(QuorumCertificate),
    Ping,
    Pong,
}

/// Simplified broadcaster handle. Writes a single JSON message to peer address list.
/// This is intentionally minimal — integrate into your BroadcastHandle if you already have one.
#[derive(Clone)]
pub struct BroadcastHandle {
    pub addrs: Arc<Vec<SocketAddr>>,
}

impl BroadcastHandle {
    pub fn new(addrs: Vec<SocketAddr>) -> Self {
        Self {
            addrs: Arc::new(addrs),
        }
    }

    /// Broadcast serializes the message to JSON and writes to peers (fire-and-forget style).
    pub async fn broadcast(&self, msg: &BftMessage) -> anyhow::Result<()> {
        let bytes = serde_json::to_vec(msg).map_err(|e| anyhow::anyhow!(e))?;

        for a in self.addrs.iter() {
            let addr = *a;
            let payload = bytes.clone();
            // spawn a fire-and-forget task for each peer to avoid blocking loop
            let _ = tokio::spawn(async move {
                match TcpStream::connect(addr).await {
                    Ok(mut s) => {
                        // length-prefix to help receiver
                        let len = (payload.len() as u32).to_be_bytes();
                        if let Err(e) = s.write_all(&len).await {
                            warn!("broadcast write len failed to {}: {}", addr, e);
                            return;
                        }
                        if let Err(e) = s.write_all(&payload).await {
                            warn!("broadcast write failed to {}: {}", addr, e);
                        }
                    }
                    Err(e) => {
                        warn!("broadcast connect failed to {}: {}", addr, e);
                    }
                }
            });
        }

        Ok(())
    }
}

/// Minimal receiver helper. Call from your existing network loop or server bootstrap.
pub async fn handle_stream<RxFn>(mut stream: TcpStream, mut handler: RxFn)
where
    RxFn: FnMut(BftMessage) -> anyhow::Result<()> + Send + 'static,
{
    // read length-prefixed framing
    let mut len_buf = [0u8; 4];
    loop {
        if let Err(e) = stream.read_exact(&mut len_buf).await {
            warn!("bft read len failed: {}", e);
            return;
        }
        let len = u32::from_be_bytes(len_buf) as usize;
        let mut buf = vec![0u8; len];
        if let Err(e) = stream.read_exact(&mut buf).await {
            warn!("bft read payload failed: {}", e);
            return;
        }
        match serde_json::from_slice::<BftMessage>(&buf) {
            Ok(msg) => {
                if let Err(e) = handler(msg) {
                    warn!("bft handler error: {}", e);
                }
            }
            Err(e) => {
                warn!("bft msg deserialize failed: {}", e);
                continue;
            }
        }
    }
}

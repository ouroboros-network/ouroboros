// tests/hotstuff_test.rs
// DISABLED: Needs API update to match current HotStuff implementation
#![cfg(disabled)]
use ed25519_dalek::SigningKey;
use ouro_dag::bft::consensus::{BFTNode, HotStuff, Proposal, Vote};
use ouro_dag::bft::crypto_bridge;
use ouro_dag::bft::messages::QuorumCertificate;
use ouro_dag::bft::state::BFTState;
use ouro_dag::network::bft_msg::{BftMessage, BroadcastHandle};
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

struct TestNode {
    hs: HotStuff,
    kp: SigningKey,
}

async fn setup_test_nodes(pool: PgPool, count: usize) -> (Vec<TestNode>, BroadcastHandle) {
    let bft_state = Arc::new(BFTState::new(pool));
    let broadcaster = BroadcastHandle::new(vec![], "127.0.0.1:9001".parse().unwrap());

    let mut nodes = Vec::new();
    let mut validators = HashMap::new();
    let mut node_ids = Vec::new();

    for i in 0..count {
        let node_id = format!("node{}", i);
        node_ids.push(node_id.clone());
        let kp = crypto_bridge::generate_keypair_write(std::path::Path::new(&format!(
            "{}.key",
            node_id
        )))
        .unwrap();
        let bft_node = BFTNode {
            name: node_id.clone(),
            pubkey: kp.verifying_key().as_bytes().to_vec(),
            keypath: Some(format!("{}.key", node_id)),
        };
        validators.insert(node_id.clone(), bft_node);
    }

    for i in 0..count {
        let node_id = format!("node{}", i);
        let peers: Vec<String> = node_ids
            .iter()
            .filter(|&id| *id != node_id)
            .cloned()
            .collect();
        let kp =
            crypto_bridge::load_keypair(std::path::Path::new(&format!("{}.key", node_id))).unwrap();
        let hs = HotStuff::new(
            node_id,
            peers,
            broadcaster.clone(),
            bft_state.clone(),
            validators.clone(),
            2000,
        );
        nodes.push(TestNode { hs, kp });
    }

    (nodes, broadcaster)
}

#[tokio::test]
async fn test_propose_vote_qc_flow() {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://ouro:ouro_pass@127.0.0.1:15432/ouro_db".into());
    let pool = PgPool::connect(&database_url).await.expect("pg connect");

    let (mut nodes, broadcaster) = setup_test_nodes(pool, 3).await;

    let (tx, mut rx) = mpsc::channel(100);
    for node in &mut nodes {
        broadcaster
            .add_peer_tx(node.hs.id.clone(), tx.clone())
            .await;
    }

    // Node 0 is proposer for view 1
    let proposer_node = &nodes[0];
    let view = proposer_node.hs.start_view().await.unwrap();
    assert_eq!(view, 1);

    // Capture the proposal
    let proposal_msg = rx.recv().await.unwrap();
    let proposal = match proposal_msg {
        BftMessage::Proposal(p) => p,
        _ => panic!("Expected proposal"),
    };

    // Replicas handle the proposal and vote
    for (i, node) in nodes.iter().enumerate() {
        if i == 0 {
            continue;
        } // Skip proposer
        node.hs.handle_proposal(proposal.clone()).await.unwrap();
    }

    // Capture votes
    let mut votes = Vec::new();
    for _ in 0..2 {
        // 2 replicas
        let vote_msg = rx.recv().await.unwrap();
        let vote = match vote_msg {
            BftMessage::Vote(v) => v,
            _ => panic!("Expected vote"),
        };
        votes.push(vote);
    }

    // Proposer handles votes
    for vote in votes {
        nodes[0].hs.handle_vote(vote).await.unwrap();
    }

    // Capture QC
    let qc_msg = rx.recv().await.unwrap();
    let qc = match qc_msg {
        BftMessage::QC(qc) => qc,
        _ => panic!("Expected QC"),
    };

    assert_eq!(qc.block_id, proposal.block_id);
    assert_eq!(qc.view, proposal.view);
    assert_eq!(qc.signers.len(), 2); // n1 and n2 votes + proposer self-vote
}

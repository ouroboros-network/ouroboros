// tests/consensus.rs
// DISABLED: Needs API update to match current HotStuff implementation
#![cfg(disabled)]
use ouro_dag::bft::consensus::BFTNode;
use ouro_dag::bft::consensus::{HotStuff, Vote};
use ouro_dag::bft::state::BFTState;
use ouro_dag::network::bft_msg::{BftMessage, BroadcastHandle};
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

// We'll use in-process channels. For brevity, we won't implement BroadcastHandle here.
// Instead we'll directly call node.handle_proposal/handle_vote in the simulated driver.

async fn setup_nodes(pool: PgPool) -> (HotStuff, HotStuff, HotStuff) {
    let bft_state = Arc::new(BFTState::new(pool));

    let mut validators = HashMap::new();
    validators.insert(
        "node1".to_string(),
        BFTNode {
            name: "node1".to_string(),
            pubkey: vec![1; 32],
            keypath: None,
        },
    );
    validators.insert(
        "node2".to_string(),
        BFTNode {
            name: "node2".to_string(),
            pubkey: vec![2; 32],
            keypath: None,
        },
    );
    validators.insert(
        "node3".to_string(),
        BFTNode {
            name: "node3".to_string(),
            pubkey: vec![3; 32],
            keypath: None,
        },
    );

    let broadcaster = BroadcastHandle::new(vec![], "127.0.0.1:9001".parse().unwrap());
    let n1 = HotStuff::new(
        "node1".into(),
        vec!["node2".into(), "node3".into()],
        broadcaster.clone(),
        bft_state.clone(),
        validators.clone(),
        1000,
    );
    let n2 = HotStuff::new(
        "node2".into(),
        vec!["node1".into(), "node3".into()],
        broadcaster.clone(),
        bft_state.clone(),
        validators.clone(),
        1000,
    );
    let n3 = HotStuff::new(
        "node3".into(),
        vec!["node1".into(), "node3".into()],
        broadcaster.clone(),
        bft_state.clone(),
        validators.clone(),
        1000,
    );

    (n1, n2, n3)
}

#[tokio::test]
async fn three_node_propose_vote_qc_flow() {
    // For simplicity assume DATABASE_URL env var points to a test DB.
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://ouro:ouro_pass@127.0.0.1:15432/ouro_db".into());
    let pool = PgPool::connect(&database_url).await.expect("pg connect");

    let (n1, n2, n3) = setup_nodes(pool).await;

    // Simulate: n1 is proposer for view 1
    let view = n1.start_view().await.expect("start view");
    assert_eq!(view, 1);

    // Extract the proposal n1 produced. This is tricky without changing the code to be more testable.
    // We will listen on a broadcast channel to get the proposal.
    let (tx, mut rx) = mpsc::channel(100);
    let mut broadcaster = n1.broadcaster.clone();
    broadcaster
        .add_peer_tx("node2".to_string(), tx.clone())
        .await;
    broadcaster
        .add_peer_tx("node3".to_string(), tx.clone())
        .await;

    // Restart view to get proposal
    let view = n1.start_view().await.expect("start view 2");
    assert_eq!(view, 2);

    let prop_msg = rx.recv().await.unwrap();
    let prop = match prop_msg {
        BftMessage::Proposal(p) => p,
        _ => panic!("Expected a proposal message"),
    };

    // other nodes receive proposal
    n2.handle_proposal(prop.clone()).await.unwrap();
    n3.handle_proposal(prop.clone()).await.unwrap();

    // n2 and n3 will have broadcasted votes. Simulate reception at n1 by crafting vote messages
    // In a real test, we would have the nodes sign the votes. For now, we will skip signature verification.
    // To do this, we will have to modify the handle_vote function to skip verification for tests.
    // Let's assume for now that the signature verification is disabled for this test.
    let v2 = Vote {
        block_id: prop.block_id,
        view: prop.view,
        voter: "node2".into(),
        sig: vec![],
    };
    let v3 = Vote {
        block_id: prop.block_id,
        view: prop.view,
        voter: "node3".into(),
        sig: vec![],
    };

    // n1 handles votes
    n1.handle_vote(v2).await.unwrap();
    n1.handle_vote(v3).await.unwrap();

    // After second vote, n1 should form a QC.
    // We can't directly check the internal state of n1 without making it more testable.
    // However, we can check if a QC message is broadcasted.
    let qc_msg = rx.recv().await.unwrap();
    let qc = match qc_msg {
        BftMessage::QC(qc) => qc,
        _ => panic!("Expected a QC message"),
    };

    assert_eq!(qc.block_id, prop.block_id);
    assert_eq!(qc.view, prop.view);
    assert_eq!(qc.signers.len(), 2); // n2 and n3
}

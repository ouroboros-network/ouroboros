// src/main_consensus_boot.rs
use ouro_dag::bft::consensus::HotStuff;
use ouro_dag::network::bft_msg::BroadcastHandle;
use ouro_dag::bft::state::BFTState;
use std::net::SocketAddr;
use std::collections::HashMap;
use ouro_dag::bft::consensus::BFTNode;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
 env_logger::init();

 // DB pool
 let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://ouro:ouro_pass@127.0.0.1:15432/ouro_db".to_string());
 let pool = PgPool::connect(&db_url).await?;

 // BFT state manager
 let bft_state = Arc::new(BFTState::new(pool));

 // peers and addresses - set these in your config
 let peers_addrs: Vec<SocketAddr> = vec![
 "127.0.0.1:9001".parse().unwrap(),
 "127.0.0.1:9002".parse().unwrap(),
 ];
 let my_addr: SocketAddr = "127.0.0.1:9000".parse().unwrap();
 let broadcaster = BroadcastHandle::new(peers_addrs.clone(), my_addr);

 // validators map - in production load pubkeys and optional key paths
 let mut validators = HashMap::new();
 validators.insert("node1".into(), BFTNode {
 name: "node1".into(),
 private_key_seed: vec![],
 dilithium_keypair: None,
 pq_migration_phase: ouro_dag::crypto::hybrid::MigrationPhase::Phase1EdOrHybrid,
 });
 validators.insert("node2".into(), BFTNode {
 name: "node2".into(),
 private_key_seed: vec![],
 dilithium_keypair: None,
 pq_migration_phase: ouro_dag::crypto::hybrid::MigrationPhase::Phase1EdOrHybrid,
 });
 validators.insert("node3".into(), BFTNode {
 name: "node3".into(),
 private_key_seed: vec![],
 dilithium_keypair: None,
 pq_migration_phase: ouro_dag::crypto::hybrid::MigrationPhase::Phase1EdOrHybrid,
 });

 let hotstuff = HotStuff::new("node1".into(), vec!["node2".into(), "node3".into()], broadcaster, bft_state, validators, 2000);

 // spawn network accept loop that passes incoming messages to hotstuff handlers
 // TODO: use your existing network acceptor and call hotstuff.handle_proposal/handle_vote/receive_qc appropriately.

 Ok(())
}

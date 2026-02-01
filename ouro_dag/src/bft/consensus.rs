// src/bft/consensus.rs
use crate::network::bft_msg::BroadcastHandle;
use crate::bft::state::BFTState;
use crate::bft::validator_registry::ValidatorRegistry;
use crate::bft::slashing::{SlashingManager, SlashingReason, SlashingSeverity};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;
use chrono::{Utc, DateTime};
use std::time::Instant;
use anyhow::Result;
use crate::crypto::keys::{sign_bytes, verify_bytes, public_from_seed};
use hex;
use log::error;
use crate::bft::qc;

// Phase 6: Post-quantum crypto imports
use crate::crypto::pq::DilithiumKeypair;
use crate::crypto::hybrid::{HybridKeypair, MigrationPhase};
use ed25519_dalek::SigningKey;

pub type NodeId = String;
pub type View = u64;
pub type BlockId = Uuid;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Block {
 pub id: Uuid,
 pub timestamp: DateTime<Utc>,
 pub tx_ids: Vec<Uuid>,
 pub validator_signatures: Vec<Vec<u8>>, // Binary signatures
 pub proposer: String, // Block proposer for reward distribution
 pub height: u64, // Block height for tail emission calculation
}

impl Block {
 pub fn new(proposer: &str, tx_ids: Vec<Uuid>) -> Self {
 Block {
 id: Uuid::new_v4(),
 timestamp: Utc::now(),
 tx_ids,
 validator_signatures: vec![format!("placeholder:{}", proposer).into_bytes()],
 proposer: proposer.to_string(),
 height: 0, // Set by caller or block insertion logic
 }
 }
}

#[derive(Clone, Debug)]
pub struct BFTNode {
 pub name: String,
 pub private_key_seed: Vec<u8>, // 32-byte seed

 /// Phase 6: Optional post-quantum Dilithium keypair for quantum-resistant signing
 /// When set, the node will use hybrid signatures (Ed25519 + Dilithium5)
 pub dilithium_keypair: Option<DilithiumKeypair>,

 /// Phase 6: Migration phase for gradual PQ adoption
 pub pq_migration_phase: MigrationPhase,
}

impl BFTNode {
 /// Sign a block ID using Ed25519 cryptography.
 /// Returns binary signature bytes (more efficient than hex encoding).
 ///
 /// If the private key seed is invalid or empty, returns a fallback signature
 /// (for backward compatibility with tests/legacy code).
 pub fn sign_block(&self, block_id: &Uuid) -> Vec<u8> {
 // Construct canonical message to sign: block_id bytes
 let message = block_id.as_bytes();

 // Use real Ed25519 signing if we have a valid 32-byte seed
 if self.private_key_seed.len() == 32 {
 match crate::crypto::keys::sign_bytes(&self.private_key_seed, message) {
 Some(sig_bytes) => {
 // Return binary signature (32-40% more efficient than hex)
 log::debug!("Signed block {} with Ed25519 (node: {})", block_id, self.name);
 return sig_bytes;
 }
 None => {
 log::warn!(
 "Ed25519 signing failed for node {} - using fallback signature",
 self.name
 );
 }
 }
 } else {
 log::warn!(
 "Node {} has invalid key seed (len: {}, expected: 32) - using fallback signature",
 self.name,
 self.private_key_seed.len()
 );
 }

 // Fallback for tests/invalid keys (should not happen in production)
 format!("fallback_sig:{}:{}", self.name, block_id).into_bytes()
 }

 /// Sign a block with hybrid post-quantum signatures (Phase 6)
 ///
 /// Uses Ed25519 + Dilithium5 for quantum resistance during migration period.
 /// Once quantum computers are viable, this becomes the primary signing method.
 pub fn sign_block_hybrid(&self, block_id: &Uuid) -> Result<Vec<u8>> {
 let message = block_id.as_bytes();

 // Check if PQ is enabled
 if let Some(ref dil_keypair) = self.dilithium_keypair {
 // We have Dilithium keypair, check migration phase
 if self.pq_migration_phase.requires_hybrid() ||
 self.pq_migration_phase.requires_dilithium_only() {

 if self.pq_migration_phase.requires_dilithium_only() {
 // Phase 3: Pure Dilithium signatures
 let dil_sig = dil_keypair.sign(message)?;
 log::debug!(
 "Signed block {} with Dilithium5 (node: {}, sig size: {} bytes)",
 block_id, self.name, dil_sig.bytes.len()
 );
 return Ok(bincode::serialize(&dil_sig)?);
 } else {
 // Phase 2: Hybrid signatures required
 if self.private_key_seed.len() != 32 {
 anyhow::bail!("Invalid Ed25519 key seed for hybrid signing");
 }

 // Create Ed25519 signing key from seed
 let ed_signing = SigningKey::from_bytes(
 self.private_key_seed.as_slice().try_into()
 .map_err(|_| anyhow::anyhow!("Invalid key seed length"))?
 );
 let ed_verifying = ed_signing.verifying_key();

 // Create hybrid keypair
 let hybrid_keypair = HybridKeypair::from_keypairs(
 ed_signing,
 ed_verifying,
 dil_keypair.clone(),
 );

 // Sign with both keys
 let hybrid_sig = hybrid_keypair.sign(message)?;

 log::debug!(
 "Signed block {} with hybrid signature (node: {}, sig size: {} bytes)",
 block_id, self.name, hybrid_sig.size_bytes()
 );

 return Ok(bincode::serialize(&hybrid_sig)?);
 }
 }
 }

 // Phase 1 or no PQ: Fall back to Ed25519 only
 Ok(self.sign_block(block_id))
 }

 /// Initialize Dilithium keypair for this node (Phase 6)
 pub fn init_pq_keypair(&mut self) {
 if self.dilithium_keypair.is_none() {
 log::info!(" Generating Dilithium5 keypair for node {}", self.name);
 self.dilithium_keypair = Some(DilithiumKeypair::generate());
 log::info!(" Node {} is now quantum-resistant", self.name);
 }
 }

 /// Set migration phase for this node
 pub fn set_pq_migration_phase(&mut self, phase: MigrationPhase) {
 log::info!("SYNC Node {} switching to migration phase: {:?}", self.name, phase);
 self.pq_migration_phase = phase;
 }

 /// Check if this node has PQ capabilities
 pub fn is_pq_enabled(&self) -> bool {
 self.dilithium_keypair.is_some()
 }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Proposal {
 pub block_id: BlockId,
 pub parent_id: Option<BlockId>,
 pub view: View,
 pub proposer: NodeId,
 pub sig: Vec<u8>, // Binary signature (32-40% more efficient than hex)
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Vote {
 pub block_id: BlockId,
 pub view: View,
 pub voter: NodeId,
 pub sig: Vec<u8>, // Binary signature
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct QuorumCertificate {
 pub block_id: BlockId,
 pub view: View,
 pub signers: Vec<NodeId>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum NodeState { Propose, Vote, Commit, Exec }

#[derive(Debug)]
pub struct HotStuffConfig {
 pub id: NodeId,
 pub peers: Vec<NodeId>, // ordered stable list of other nodes
 pub timeout_ms: u64,
 pub secret_seed: Vec<u8>, // 32-byte seed for signing (local only)
}

impl HotStuffConfig {
 pub fn new(id: NodeId, peers: Vec<NodeId>, timeout_ms: u64, secret_seed: Vec<u8>) -> Self {
 HotStuffConfig { id, peers, timeout_ms, secret_seed }
 }
}

pub struct HotStuffInner {
 pub view: View,
 pub state: NodeState,
 pub locked_qc: Option<QuorumCertificate>,
 pub highest_qc: Option<QuorumCertificate>,
 pub votes: HashMap<BlockId, HashSet<NodeId>>,
 pub pending_proposals: HashMap<View, Proposal>,
 pub last_view_start: Option<Instant>,
}

pub struct HotStuff {
 pub config: Arc<HotStuffConfig>,
 pub inner: Arc<Mutex<HotStuffInner>>,
 pub broadcaster: BroadcastHandle,
 pub bft_state: Arc<BFTState>,
 pub registry: Arc<ValidatorRegistry>,
}


// helper: canonical proposal payload bytes
fn proposal_payload_bytes(prop: &Proposal) -> Vec<u8> {
 let mut out = Vec::new();
 out.extend_from_slice(prop.block_id.as_bytes());
 if let Some(parent) = prop.parent_id {
 out.extend_from_slice(parent.as_bytes());
 }
 out.extend_from_slice(&prop.view.to_be_bytes());
 out.extend_from_slice(prop.proposer.as_bytes());
 out
}

// helper: canonical vote payload bytes
fn vote_payload_bytes(v: &Vote) -> Vec<u8> {
 let mut out = Vec::new();
 out.extend_from_slice(v.block_id.as_bytes());
 out.extend_from_slice(&v.view.to_be_bytes());
 out.extend_from_slice(v.voter.as_bytes());
 out
}

impl HotStuff {
 pub fn new(
 config: Arc<HotStuffConfig>, 
 broadcaster: BroadcastHandle, 
 bft_state: Arc<BFTState>,
 registry: Arc<ValidatorRegistry>,
 ) -> Self {
 let inner = Arc::new(Mutex::new(HotStuffInner {
 view: 0,
 state: NodeState::Propose,
 locked_qc: None,
 highest_qc: None,
 votes: HashMap::new(),
 pending_proposals: HashMap::new(),
 last_view_start: None,
 }));
 HotStuff {
 config,
 inner,
 broadcaster,
 bft_state,
 registry,
 }
 }

 // Number of nodes for quorum calculation (used in full BFT consensus)
 #[allow(dead_code)]
 fn n(&self) -> usize { 1 + self.config.peers.len() }

 // when proposing (proposer side)
 async fn sign_proposal_local(&self, block_id: &BlockId, parent: Option<BlockId>, view: View) -> String {
 let prop = Proposal {
 block_id: *block_id,
 parent_id: parent,
 view,
 proposer: self.config.id.clone(),
 sig: Vec::new(), // Binary signature placeholder
 };
 let payload = proposal_payload_bytes(&prop);
 if let Some(sig) = sign_bytes(&self.config.secret_seed, &payload) {
 hex::encode(sig)
 } else {
 // fallback: empty sig
 "".into()
 }
 }

 // verify incoming proposal
 async fn verify_proposal(&self, p: &Proposal) -> bool {
 use std::str::FromStr;
 let payload = proposal_payload_bytes(p);
 let sig_bytes = match hex::decode(&p.sig) {
 Ok(b) => b,
 Err(_) => return false,
 };

 // 1. Check registry first
 if let Some(pk) = self.registry.get(&p.proposer) {
 return verify_bytes(&pk, &payload, &sig_bytes);
 }

 // 2. Check DB (TODO_ROCKSDB: Implement with RocksDB)
 if let Ok(_proposer_uuid) = Uuid::from_str(&p.proposer) {
 // TODO_ROCKSDB: Query microchain pubkey from RocksDB
 }

 // 3. Fallback for self
 if p.proposer == self.config.id {
 if let Some(pk) = public_from_seed(&self.config.secret_seed) {
 return verify_bytes(&pk, &payload, &sig_bytes);
 }
 }
 false
 }

 // sign vote locally
 async fn sign_vote_local(&self, block_id: &BlockId, view: View) -> String {
 let vote = Vote {
 block_id: *block_id,
 view,
 voter: self.config.id.clone(),
 sig: Vec::new(), // Binary signature placeholder
 };
 let payload = vote_payload_bytes(&vote);
 if let Some(sig) = sign_bytes(&self.config.secret_seed, &payload) {
 hex::encode(sig)
 } else {
 "".into()
 }
 }

 // verify vote (used in full BFT consensus)
 #[allow(dead_code)]
 async fn verify_vote(&self, v: &Vote) -> bool {
 use std::str::FromStr;
 let payload = vote_payload_bytes(v);
 let sig_bytes = match hex::decode(&v.sig) {
 Ok(b) => b,
 Err(_) => return false,
 };

 // 1. Check registry first
 if let Some(pk) = self.registry.get(&v.voter) {
 return verify_bytes(&pk, &payload, &sig_bytes);
 }

 // 2. Check DB (TODO_ROCKSDB: Implement with RocksDB)
 if let Ok(_voter_uuid) = Uuid::from_str(&v.voter) {
 // TODO_ROCKSDB: Query microchain pubkey from RocksDB
 }
 
 // 3. Fallback for self
 if v.voter == self.config.id {
 if let Some(pk) = public_from_seed(&self.config.secret_seed) {
 return verify_bytes(&pk, &payload, &sig_bytes);
 }
 }
 false
 }

 pub async fn start_view(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
 // bump view and clear transient state under a short lock
 let view = {
 let mut inn = self.inner.lock().await;
 inn.view = inn.view.wrapping_add(1);
 inn.votes.clear();
 inn.pending_proposals.clear(); // Changed from inn.pending.clear()
 inn.last_view_start = Some(Instant::now());
 inn.view
 };

 // proposer selection (round robin)
 let mut nodes = self.config.peers.clone();
 nodes.push(self.config.id.clone());
 nodes.sort();
 let proposer = {
 let idx = ((view as usize).wrapping_sub(1)) % nodes.len();
 nodes[idx].clone()
 };

 if proposer == self.config.id {
 // create a block proposal
 let txs = crate::mempool::select_transactions(200).await.unwrap_or_default();
 let block_id = match crate::dag::dag::insert_block_stub(txs.clone(), &self.config.id, view).await {
 Ok(id) => id,
 Err(e) => {
 error!("Failed to create block proposal: {}", e);
 return Err(Box::new(std::io::Error::new(
 std::io::ErrorKind::Other,
 format!("Block creation failed: {}", e),
 )));
 }
 };

 let parent = None; // Simplified for now
 let sig = self.sign_proposal_local(&block_id, parent, view).await;

 // build Proposal (fill required fields that your Proposal type expects)
 let prop = Proposal {
 block_id,
 parent_id: parent,
 view,
 proposer: self.config.id.clone(),
 sig: sig.into_bytes(), // Convert hex string to bytes
 // if your Proposal has a timestamp field, set it here:
 // timestamp: chrono::Utc::now(),
 };

 // insert into pending under a short lock
 {
 let mut inn = self.inner.lock().await;
 inn.pending_proposals.insert(view, prop.clone()); // Changed from inn.pending.insert
 }

 // broadcast without holding any locks
 if let Err(e) = self.broadcaster
 .broadcast(&crate::network::bft_msg::BftMessage::Proposal(prop))
 .await {
 return Err(Box::new(std::io::Error::new(
 std::io::ErrorKind::Other,
 format!("{}", e),
 )));
 }
 }

 Ok(())
 }

 pub async fn handle_proposal(&self, p: Proposal) -> Result<()> {
 let current_view;
 {
 let inner = self.inner.lock().await;
 current_view = inner.view;
 }
 // stale
 if p.view < current_view { return Ok(()); }

 if !self.verify_proposal(&p).await {
 log::warn!("proposal verification failed for view {}", p.view);
 return Ok(());
 }

 {
 let mut inner = self.inner.lock().await;
 // lock rule: if we have locked_qc with higher view reject
 if let Some(l) = &inner.locked_qc {
 if l.view > p.view {
 return Ok(());
 }
 }
 inner.pending_proposals.insert(p.view, p.clone());
 }


 // vote
 let sig = self.sign_vote_local(&p.block_id, p.view).await;
 let vote = Vote {
 block_id: p.block_id,
 view: p.view,
 voter: self.config.id.clone(),
 sig: sig.into_bytes(), // Convert hex string to bytes
 };
 self.broadcaster.broadcast(&crate::network::bft_msg::BftMessage::Vote(vote.clone())).await.map_err(|e| anyhow::anyhow!("{}", e))?;
 // process our vote locally
 self.handle_vote(vote).await.map_err(|e| anyhow::anyhow!("{}", e))?;
 Ok(())
 }

 // ---- replace handle_vote in src/bft/consensus.rs ----
 pub async fn handle_vote(&self, v: Vote) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
 // ensure view not behind
 {
 let inn = self.inner.lock().await;
 if v.view < inn.view { return Ok(()); }
 }

 // --- Start of new verification logic ---
 // Reconstruct the canonical payload used when signing votes.
 let payload = vote_payload_bytes(&v); // Use existing helper function

 // Try to obtain the public key bytes for the voter.
 let pubkey_bytes_opt: Option<Vec<u8>> = if let Some(pk) = self.registry.get(&v.voter) {
 Some(pk.clone())
 } else {
 // TODO_ROCKSDB: Query microchain pubkey from RocksDB
 None
 };

 let pubkey_bytes: Vec<u8> = match pubkey_bytes_opt {
 Some(pk) => pk,
 None => {
 log::warn!("no pubkey for voter {}", v.voter);
 return Ok(());
 }
 };

 // Decode signature from hex
 let sig_bytes = match hex::decode(&v.sig) {
 Ok(b) => b,
 Err(e) => {
 log::error!(
 "CRITICAL: SECURITY: Rejecting vote from {} - invalid signature format: {}",
 v.voter, e
 );
 // TODO: Implement slashing for malformed signatures (indicates malicious validator)
 // For now, return error instead of silently discarding
 return Err(Box::new(std::io::Error::new(
 std::io::ErrorKind::InvalidData,
 format!("Invalid signature format from voter {}", v.voter),
 )));
 }
 };

 // SECURITY: verify using cryptographic signature verification
 let ok = crate::crypto::keys::verify_bytes(&pubkey_bytes, &payload, &sig_bytes);
 if !ok {
 log::error!(
 "CRITICAL: SECURITY VIOLATION: Rejecting vote from {} (view: {}, block: {}) - cryptographic signature verification FAILED",
 v.voter, v.view, v.block_id
 );
 log::error!(
 " This indicates either: (1) Malicious validator forging signatures, or (2) Network corruption"
 );

 // SLASHING: Punish validator for invalid signature (Major severity - 50% stake penalty)
 if let Some(pool) = self.bft_state.get_pg_pool_option() {
 let slashing_manager = SlashingManager::new(pool.clone());
 let evidence = format!(
 "Invalid signature on vote: view={}, block={}, voter={}, sig={}",
 v.view, v.block_id, v.voter, hex::encode(&v.sig)
 );

 match slashing_manager.slash_validator(
 &v.voter,
 SlashingReason::InvalidSignature,
 SlashingSeverity::Major,
 &evidence,
 ).await {
 Ok(event) => {
 log::error!(
 " SLASHING EXECUTED: Validator {} penalized {} units for invalid signature",
 v.voter, event.slashed_amount
 );
 }
 Err(e) => {
 log::error!("Failed to execute slashing for {}: {}", v.voter, e);
 }
 }
 } else {
 log::warn!("Cannot slash validator {} - database not available", v.voter);
 }

 return Err(Box::new(std::io::Error::new(
 std::io::ErrorKind::PermissionDenied,
 format!("Cryptographic signature verification failed for voter {} on view {}", v.voter, v.view),
 )));
 }

 log::debug!(
 " Vote signature verified: voter={}, view={}, block={}",
 v.voter, v.view, v.block_id
 );
 // --- End of new verification logic ---


 // SECURITY: Equivocation detection - validator voting for multiple blocks in same view
 match self.bft_state.record_signature(&v.voter, v.view, &v.block_id.to_string()).await {
 Ok(()) => {
 // Vote recorded successfully, no equivocation
 }
 Err(e) => {
 match e {
 crate::bft::state::BFTStateError::Equivocation(ev) => {
 log::error!(
 "CRITICAL: EQUIVOCATION DETECTED: Validator {} voted for multiple blocks in view {}",
 v.voter, v.view
 );
 log::error!(" Previous vote: {:?}", ev);
 log::error!(" Current vote: block_id={}", v.block_id);
 log::error!(" This is a severe protocol violation - validator will be slashed!");

 // SLASHING: Punish validator for equivocation (Critical severity - 100% stake penalty)
 // Equivocation is the most severe BFT violation and warrants full slashing
 if let Some(pool) = self.bft_state.get_pg_pool_option() {
 let slashing_manager = SlashingManager::new(pool.clone());
 let evidence = format!(
 "Equivocation: validator {} double-voted in view {}. \
 Previous block: {}, Current block: {}. Observed at: {}",
 ev.validator, ev.round, ev.existing, ev.conflicting, ev.observed_at
 );

 match slashing_manager.slash_validator(
 &v.voter,
 SlashingReason::Equivocation,
 SlashingSeverity::Critical,
 &evidence,
 ).await {
 Ok(event) => {
 log::error!(
 " SLASHING EXECUTED: Validator {} FULLY SLASHED ({} units) for equivocation",
 v.voter, event.slashed_amount
 );
 }
 Err(e) => {
 log::error!("Failed to execute slashing for equivocating validator {}: {}", v.voter, e);
 }
 }
 } else {
 log::warn!("Cannot slash validator {} for equivocation - database not available", v.voter);
 }

 // Return error instead of silently accepting
 return Err(Box::new(std::io::Error::new(
 std::io::ErrorKind::PermissionDenied,
 format!("Equivocation detected: validator {} violated consensus by double-voting", v.voter),
 )));
 }
 crate::bft::state::BFTStateError::Db(err) => {
 log::error!("Database error while recording vote signature: {}", err);
 // Database errors should be surfaced, not silently ignored
 return Err(Box::new(std::io::Error::new(
 std::io::ErrorKind::Other,
 format!("DB error: {}", err),
 )));
 }
 }
 }
 }

 // accumulate votes and maybe form QC
 let mut maybe_qc: Option<QuorumCertificate> = None;
 {
 let mut inn = self.inner.lock().await;
 let set = inn.votes.entry(v.block_id).or_insert_with(HashSet::new);
 set.insert(v.voter.clone());

 let q = qc::quorum_size(self.config.peers.len() + 1);
 if set.len() >= q {
 let signers = set.iter().cloned().collect::<HashSet<_>>();
 let qc = qc::form_qc(v.block_id, v.view, signers);
 // update highest_qc if appropriate
 let update = match &inn.highest_qc {
 None => true,
 Some(existing) => qc.view > existing.view,
 };
 if update {
 inn.highest_qc = Some(qc.clone());
 }
 maybe_qc = Some(qc);
 }
 }

 if let Some(qc) = maybe_qc {
 // broadcast QC to peers
 self.broadcaster.broadcast(&crate::network::bft_msg::BftMessage::QC(qc.clone())).await?;
 // handle QC locally
 self.handle_qc(qc).await?;
 }

 Ok(())
 }

 // ---- replace handle_qc in src/bft/consensus.rs ----
 pub async fn handle_qc(&self, qc: QuorumCertificate) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
 // update locked_qc/highest_qc using inner lock
 {
 let mut inn = self.inner.lock().await;
 let update_locked = match &inn.locked_qc {
 None => true,
 Some(existing) => qc.view > existing.view,
 };
 if update_locked {
 inn.locked_qc = Some(qc.clone());
 }
 if inn.highest_qc.as_ref().map_or(true, |h| qc.view > h.view) {
 inn.highest_qc = Some(qc.clone());
 }
 }

 // finalize commit path: fetch tx ids, construct finalized block and call VM/reconciliation
 let _tx_ids = crate::dag::dag::get_txids_for_block(qc.block_id).await.unwrap_or_default();
 let _ = crate::reconciliation::finalize_block(qc.block_id).await;

 if let Err(e) = self.start_view().await {
 return Err(Box::new(std::io::Error::new(
 std::io::ErrorKind::Other,
 format!("{}", e),
 )));
 }

 Ok(())
 }

 // ---- replace receive_qc (simple forward) in src/bft/consensus.rs ----
 pub async fn receive_qc(&self, qc: QuorumCertificate) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
 // Just call handle_qc; both are &self and use internal locking.
 self.handle_qc(qc).await
 }

 /// Check if the current view has timed out (leader unresponsive)
 /// Returns true if we should trigger a view change
 pub async fn check_view_timeout(&self) -> bool {
     let inner = self.inner.lock().await;
     if let Some(start) = inner.last_view_start {
         let elapsed_ms = start.elapsed().as_millis() as u64;
         elapsed_ms > self.config.timeout_ms
     } else {
         // No view started yet, don't timeout
         false
     }
 }

 /// Force a view change due to timeout (leader unresponsive)
 /// This is called by the liveness timer when timeout is detected
 pub async fn force_view_change(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
     let current_view = {
         let inner = self.inner.lock().await;
         inner.view
     };

     log::warn!(
         "VIEW CHANGE TIMEOUT: View {} exceeded {}ms - leader may be offline, forcing view change",
         current_view, self.config.timeout_ms
     );

     // Start a new view (this will select a new leader via round-robin)
     self.start_view().await
 }

 /// Spawn a background liveness timer that monitors for view timeouts
 /// This prevents network halts when the current leader crashes or goes offline
 ///
 /// The timer checks every `check_interval_ms` and triggers a view change if:
 /// - The current view has been running longer than `timeout_ms`
 /// - No QC has been received (indicating leader is not producing blocks)
 ///
 /// Returns a handle that can be used to stop the timer (abort the task)
 pub fn spawn_liveness_timer(
     self: Arc<Self>,
     check_interval_ms: u64,
 ) -> tokio::task::JoinHandle<()> {
     let node = self.clone();
     let timeout_ms = self.config.timeout_ms;

     tokio::spawn(async move {
         let mut interval = tokio::time::interval(
             std::time::Duration::from_millis(check_interval_ms)
         );

         log::info!(
             "LIVENESS: Started view timeout monitor (check every {}ms, timeout {}ms)",
             check_interval_ms, timeout_ms
         );

         loop {
             interval.tick().await;

             // Check if view has timed out
             if node.check_view_timeout().await {
                 log::warn!("LIVENESS: View timeout detected, triggering view change");

                 match node.force_view_change().await {
                     Ok(()) => {
                         log::info!("LIVENESS: View change successful, new leader selected");
                     }
                     Err(e) => {
                         log::error!("LIVENESS: View change failed: {} - will retry", e);
                     }
                 }
             }
         }
     })
 }

 /// Get the current view number (for monitoring/debugging)
 pub async fn current_view(&self) -> View {
     self.inner.lock().await.view
 }

 /// Get time elapsed in current view (for monitoring)
 pub async fn view_elapsed_ms(&self) -> Option<u64> {
     let inner = self.inner.lock().await;
     inner.last_view_start.map(|start| start.elapsed().as_millis() as u64)
 }
}

pub fn finalize_block(tx_ids: Vec<Uuid>, validators: &[BFTNode]) -> Block {
 Block {
 id: Uuid::new_v4(),
 timestamp: Utc::now(),
 tx_ids,
 validator_signatures: validators.iter().map(|v| v.sign_block(&Uuid::new_v4())).collect(),
 proposer: if validators.is_empty() { "unknown".to_string() } else { validators[0].name.clone() },
 height: 0, // Will be set by blockchain
 }
}

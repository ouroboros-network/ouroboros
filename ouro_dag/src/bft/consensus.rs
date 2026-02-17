// src/bft/consensus.rs
use crate::bft::qc;
use crate::bft::slashing::{SlashingManager, SlashingReason, SlashingSeverity};
use crate::bft::state::BFTState;
use crate::bft::validator_registry::ValidatorRegistry;
use crate::crypto::keys::{public_from_seed, sign_bytes, verify_bytes};
use crate::network::bft_msg::BroadcastHandle;
use anyhow::Result;
use chrono::{DateTime, Utc};
use hex;
use log::error;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use uuid::Uuid;

// Phase 6: Post-quantum crypto imports
use crate::crypto::hybrid::{HybridKeypair, MigrationPhase};
use crate::crypto::pq::DilithiumKeypair;
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
    pub proposer: String,                   // Block proposer for reward distribution
    pub height: u64,                        // Block height for tail emission calculation
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
    /// Returns error if the key is invalid - NEVER uses fallback signatures
    /// as that would compromise BFT security.
    pub fn sign_block(&self, block_id: &Uuid) -> Vec<u8> {
        self.try_sign_block(block_id).unwrap_or_else(|e| {
            // In production, invalid keys should cause the node to stop participating
            // rather than producing invalid signatures that break consensus
            log::error!(
                "CRITICAL: Node {} cannot sign block {}: {}",
                self.name, block_id, e
            );
            log::error!("Node will not participate in consensus until key is fixed");
            // Return empty signature - validators will reject this block
            Vec::new()
        })
    }

    /// Try to sign a block, returning Result for proper error handling
    pub fn try_sign_block(&self, block_id: &Uuid) -> Result<Vec<u8>> {
        // Construct canonical message to sign: block_id bytes
        let message = block_id.as_bytes();

        // Require valid 32-byte seed
        if self.private_key_seed.len() != 32 {
            return Err(anyhow::anyhow!(
                "Invalid key seed length: {} (expected 32 bytes)",
                self.private_key_seed.len()
            ));
        }

        // Use real Ed25519 signing
        match crate::crypto::keys::sign_bytes(&self.private_key_seed, message) {
            Some(sig_bytes) => {
                log::debug!(
                    "Signed block {} with Ed25519 (node: {})",
                    block_id,
                    self.name
                );
                Ok(sig_bytes)
            }
            None => Err(anyhow::anyhow!(
                "Ed25519 signing failed for node {}",
                self.name
            )),
        }
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
            if self.pq_migration_phase.requires_hybrid()
                || self.pq_migration_phase.requires_dilithium_only()
            {
                if self.pq_migration_phase.requires_dilithium_only() {
                    // Phase 3: Pure Dilithium signatures
                    let dil_sig = dil_keypair.sign(message)?;
                    log::debug!(
                        "Signed block {} with Dilithium5 (node: {}, sig size: {} bytes)",
                        block_id,
                        self.name,
                        dil_sig.bytes.len()
                    );
                    return Ok(bincode::serialize(&dil_sig)?);
                } else {
                    // Phase 2: Hybrid signatures required
                    if self.private_key_seed.len() != 32 {
                        anyhow::bail!("Invalid Ed25519 key seed for hybrid signing");
                    }

                    // Create Ed25519 signing key from seed
                    let ed_signing = SigningKey::from_bytes(
                        self.private_key_seed
                            .as_slice()
                            .try_into()
                            .map_err(|_| anyhow::anyhow!("Invalid key seed length"))?,
                    );
                    let ed_verifying = ed_signing.verifying_key();

                    // Create hybrid keypair
                    let hybrid_keypair =
                        HybridKeypair::from_keypairs(ed_signing, ed_verifying, dil_keypair.clone());

                    // Sign with both keys
                    let hybrid_sig = hybrid_keypair.sign(message)?;

                    log::debug!(
                        "Signed block {} with hybrid signature (node: {}, sig size: {} bytes)",
                        block_id,
                        self.name,
                        hybrid_sig.size_bytes()
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
        log::info!(
            "SYNC Node {} switching to migration phase: {:?}",
            self.name,
            phase
        );
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
pub enum NodeState {
    Propose,
    Vote,
    Commit,
    Exec,
}

#[derive(Debug)]
pub struct HotStuffConfig {
    pub id: NodeId,
    pub peers: Vec<NodeId>, // ordered stable list of other nodes
    pub timeout_ms: u64,
    pub secret_seed: Vec<u8>, // 32-byte seed for signing (local only)
    pub dilithium_keypair: Option<DilithiumKeypair>,
}

impl HotStuffConfig {
    pub fn new(
        id: NodeId,
        peers: Vec<NodeId>,
        timeout_ms: u64,
        secret_seed: Vec<u8>,
        dilithium_keypair: Option<DilithiumKeypair>,
    ) -> Self {
        HotStuffConfig {
            id,
            peers,
            timeout_ms,
            secret_seed,
            dilithium_keypair,
        }
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
    fn n(&self) -> usize {
        1 + self.config.peers.len()
    }

    // when proposing (proposer side)
    async fn sign_proposal_local(
        &self,
        block_id: &BlockId,
        parent: Option<BlockId>,
        view: View,
    ) -> String {
        let prop = Proposal {
            block_id: *block_id,
            parent_id: parent,
            view,
            proposer: self.config.id.clone(),
            sig: Vec::new(), // Binary signature placeholder
        };
        let payload = proposal_payload_bytes(&prop);

        // Phase 6: Hybrid Signing
        if let Some(ref dil_key) = self.config.dilithium_keypair {
            if let Ok(seed_array) = <&[u8; 32]>::try_from(self.config.secret_seed.as_slice()) {
                let ed_signing = SigningKey::from_bytes(seed_array);
                let ed_verifying = ed_signing.verifying_key();
                let hybrid_pair = HybridKeypair::from_keypairs(ed_signing, ed_verifying, dil_key.clone());

                if let Ok(sig) = hybrid_pair.sign(&payload) {
                    if let Ok(bytes) = bincode::serialize(&sig) {
                        return hex::encode(bytes);
                    }
                }
            }
        }

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
        // sig field now stores actual binary bytes (H3/H4 fix)
        let sig_bytes = &p.sig;

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

        // Phase 6: Hybrid Signing
        if let Some(ref dil_key) = self.config.dilithium_keypair {
            if let Ok(seed_array) = <&[u8; 32]>::try_from(self.config.secret_seed.as_slice()) {
                let ed_signing = SigningKey::from_bytes(seed_array);
                let ed_verifying = ed_signing.verifying_key();
                let hybrid_pair = HybridKeypair::from_keypairs(ed_signing, ed_verifying, dil_key.clone());

                if let Ok(sig) = hybrid_pair.sign(&payload) {
                    if let Ok(bytes) = bincode::serialize(&sig) {
                        return hex::encode(bytes);
                    }
                }
            }
        }

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
            let txs = crate::mempool::select_transactions(200)
                .await
                .unwrap_or_default();
            let block_id = match crate::dag::dag::insert_block_stub(
                txs.clone(),
                &self.config.id,
                view,
            )
            .await
            {
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
                sig: hex::decode(&sig).unwrap_or_else(|_| sig.into_bytes()),
            };

            // insert into pending under a short lock
            {
                let mut inn = self.inner.lock().await;
                inn.pending_proposals.insert(view, prop.clone()); // Changed from inn.pending.insert
            }

            // broadcast without holding any locks
            if let Err(e) = self
                .broadcaster
                .broadcast(&crate::network::bft_msg::BftMessage::Proposal(prop))
                .await
            {
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
        if p.view < current_view {
            return Ok(());
        }

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
            sig: hex::decode(&sig).unwrap_or_else(|_| sig.into_bytes()),
        };
        self.broadcaster
            .broadcast(&crate::network::bft_msg::BftMessage::Vote(vote.clone()))
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        // process our vote locally
        self.handle_vote(vote)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        Ok(())
    }

    // ---- replace handle_vote in src/bft/consensus.rs ----
    pub async fn handle_vote(
        &self,
        v: Vote,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // ensure view not behind
        {
            let inn = self.inner.lock().await;
            if v.view < inn.view {
                return Ok(());
            }
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

        // Signature is already stored as binary bytes (Vec<u8>)
        let sig_bytes = &v.sig;

        // Determine migration phase (Phase 1 for now)
        // In a real implementation, query blockchain height
        let phase = MigrationPhase::Phase1EdOrHybrid;

        // Try to parse pubkey as HybridPublicKey first, then fall back to raw Ed25519
        let (ed_pk, dil_pk) = if let Ok(hybrid_pk) = bincode::deserialize::<crate::crypto::hybrid::HybridPublicKey>(&pubkey_bytes) {
             let ed = ed25519_dalek::VerifyingKey::from_bytes(
                 hybrid_pk.ed25519.as_slice().try_into().unwrap_or(&[0u8; 32])
             ).ok();
             let dil = crate::crypto::pq::DilithiumPublicKey::from_bytes(hybrid_pk.dilithium).ok();
             (ed, dil)
        } else {
             // Legacy/Ed25519-only pubkey
             let ed = ed25519_dalek::VerifyingKey::from_bytes(
                 pubkey_bytes.as_slice().try_into().unwrap_or(&[0u8; 32])
             ).ok();
             (ed, None)
        };

        // SECURITY: verify using cryptographic signature verification
        let ok = crate::crypto::hybrid::verify_with_migration_policy(
            &payload,
            &sig_bytes,
            ed_pk.as_ref(),
            dil_pk.as_ref(),
            &phase
        ).is_ok();

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
                    v.view,
                    v.block_id,
                    v.voter,
                    hex::encode(&v.sig)
                );

                match slashing_manager
                    .slash_validator(
                        &v.voter,
                        SlashingReason::InvalidSignature,
                        SlashingSeverity::Major,
                        &evidence,
                    )
                    .await
                {
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
                log::warn!(
                    "Cannot slash validator {} - database not available",
                    v.voter
                );
            }

            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!(
                    "Cryptographic signature verification failed for voter {} on view {}",
                    v.voter, v.view
                ),
            )));
        }

        log::debug!(
            " Vote signature verified: voter={}, view={}, block={}",
            v.voter,
            v.view,
            v.block_id
        );
        // --- End of new verification logic ---

        // SECURITY: Equivocation detection - validator voting for multiple blocks in same view
        match self
            .bft_state
            .record_signature(&v.voter, v.view, &v.block_id.to_string())
            .await
        {
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
                        log::error!(
                            " This is a severe protocol violation - validator will be slashed!"
                        );

                        // SLASHING: Punish validator for equivocation (Critical severity - 100% stake penalty)
                        // Equivocation is the most severe BFT violation and warrants full slashing
                        if let Some(pool) = self.bft_state.get_pg_pool_option() {
                            let slashing_manager = SlashingManager::new(pool.clone());
                            let evidence = format!(
                                "Equivocation: validator {} double-voted in view {}. \
 Previous block: {}, Current block: {}. Observed at: {}",
                                ev.validator, ev.round, ev.existing, ev.conflicting, ev.observed_at
                            );

                            match slashing_manager
                                .slash_validator(
                                    &v.voter,
                                    SlashingReason::Equivocation,
                                    SlashingSeverity::Critical,
                                    &evidence,
                                )
                                .await
                            {
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
            self.broadcaster
                .broadcast(&crate::network::bft_msg::BftMessage::QC(qc.clone()))
                .await?;
            // handle QC locally
            self.handle_qc(qc).await?;
        }

        Ok(())
    }

    // ---- replace handle_qc in src/bft/consensus.rs ----
    pub async fn handle_qc(
        &self,
        qc: QuorumCertificate,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
        let _tx_ids = crate::dag::dag::get_txids_for_block(qc.block_id)
            .await
            .unwrap_or_default();
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
    pub async fn receive_qc(
        &self,
        qc: QuorumCertificate,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
            let mut interval =
                tokio::time::interval(std::time::Duration::from_millis(check_interval_ms));

            log::info!(
                "LIVENESS: Started view timeout monitor (check every {}ms, timeout {}ms)",
                check_interval_ms,
                timeout_ms
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
        inner
            .last_view_start
            .map(|start| start.elapsed().as_millis() as u64)
    }
}

pub fn finalize_block(tx_ids: Vec<Uuid>, validators: &[BFTNode]) -> Block {
    Block {
        id: Uuid::new_v4(),
        timestamp: Utc::now(),
        tx_ids,
        validator_signatures: validators
            .iter()
            .map(|v| v.sign_block(&Uuid::new_v4()))
            .collect(),
        proposer: if validators.is_empty() {
            "unknown".to_string()
        } else {
            validators[0].name.clone()
        },
        height: 0, // Will be set by blockchain
    }
}

// src/bft/leader_rotation.rs
use crate::bft::consensus::NodeId;
use crate::vrf::{select_leader, vrf_prove, vrf_verify};

/// VRF-based leader selection with stake-weighted probability
///
/// Each validator generates a VRF for the view and the one with the
/// lowest VRF value (weighted by stake) becomes the leader.
/// Falls back to round-robin if VRF fails.
///
/// # Arguments
/// * `nodes` - List of all validator node IDs
/// * `stakes` - List of (node_id, stake) tuples
/// * `my_node_id` - This validator's node ID
/// * `my_secret_key` - This validator's VRF secret key
/// * `view` - Current view number
///
/// # Returns
/// The selected leader's NodeId
pub fn proposer_for_view_vrf(
    nodes: &[NodeId],
    stakes: &[(NodeId, u64)],
    my_node_id: &NodeId,
    my_secret_key: Option<&[u8]>,
    view: u64,
) -> NodeId {
    if nodes.is_empty() {
        return "".to_string();
    }

    // If we have VRF key, try VRF-based selection
    if let Some(secret) = my_secret_key {
        let input = format!("view_{}", view);
        if let Ok(vrf_output) = vrf_prove(secret, input.as_bytes()) {
            // Calculate total stake
            let total_stake: u64 = stakes.iter().map(|(_, s)| s).sum();

            // Find my stake using the actual node ID
            if let Some((_, my_stake)) = stakes.iter().find(|(node, _)| node == my_node_id) {
                if select_leader(&vrf_output.value, total_stake, *my_stake) {
                    return my_node_id.clone(); // I am the leader
                }
            }
        }
    }

    // Fallback to round-robin for compatibility
    proposer_for_view(nodes, my_node_id, view)
}

/// Deterministic VRF-based leader election for distributed consensus
///
/// All nodes can verify who should be leader by checking VRF proofs.
/// This is more secure than simple round-robin as it prevents prediction.
///
/// # Arguments
/// * `nodes` - List of all validator node IDs
/// * `stakes` - List of (node_id, stake) tuples
/// * `vrf_outputs` - Map of node_id -> VRF output for this view
/// * `view` - Current view number (used for logging)
///
/// # Returns
/// The elected leader's NodeId based on VRF outputs
pub fn elect_leader_vrf(
    nodes: &[NodeId],
    stakes: &[(NodeId, u64)],
    vrf_outputs: &std::collections::HashMap<NodeId, crate::vrf::VrfOutput>,
    view: u64,
) -> Option<NodeId> {
    if nodes.is_empty() || vrf_outputs.is_empty() {
        return None;
    }

    let mut best_leader: Option<(NodeId, u128)> = None;

    for node in nodes {
        if let Some(output) = vrf_outputs.get(node) {
            // Get stake for this node
            let stake = stakes
                .iter()
                .find(|(n, _)| n == node)
                .map(|(_, s)| *s)
                .unwrap_or(0);

            if stake == 0 {
                continue;
            }

            // Calculate weighted VRF value (lower is better, weighted by stake)
            // Convert VRF value to a number for comparison
            let vrf_value = output
                .value
                .iter()
                .take(16)
                .fold(0u128, |acc, &b| (acc << 8) | b as u128);

            // Weight by inverse of stake (higher stake = lower weighted value = more likely to win)
            let weighted_value = vrf_value / (stake as u128).max(1);

            match &best_leader {
                None => best_leader = Some((node.clone(), weighted_value)),
                Some((_, best_value)) if weighted_value < *best_value => {
                    best_leader = Some((node.clone(), weighted_value));
                }
                _ => {}
            }
        }
    }

    log::debug!(
        "VRF leader election for view {}: {:?}",
        view,
        best_leader.as_ref().map(|(n, _)| n)
    );
    best_leader.map(|(node, _)| node)
}

/// Verify and elect leader with full VRF verification
///
/// This version verifies each VRF proof before considering the node.
/// Use when you need cryptographic guarantees that proofs are valid.
pub fn elect_leader_vrf_verified(
    nodes: &[NodeId],
    stakes: &[(NodeId, u64)],
    public_keys: &std::collections::HashMap<NodeId, Vec<u8>>,
    vrf_outputs: &std::collections::HashMap<NodeId, crate::vrf::VrfOutput>,
    view: u64,
) -> Option<NodeId> {
    if nodes.is_empty() || vrf_outputs.is_empty() {
        return None;
    }

    let input = format!("view_{}", view);
    let mut best_leader: Option<(NodeId, u128)> = None;

    for node in nodes {
        if let (Some(output), Some(pubkey)) = (vrf_outputs.get(node), public_keys.get(node)) {
            // Verify the VRF proof
            match vrf_verify(pubkey, input.as_bytes(), output) {
                Ok(true) => {
                    // Get stake for this node
                    let stake = stakes
                        .iter()
                        .find(|(n, _)| n == node)
                        .map(|(_, s)| *s)
                        .unwrap_or(0);

                    if stake == 0 {
                        continue;
                    }

                    // Calculate weighted VRF value
                    let vrf_value = output
                        .value
                        .iter()
                        .take(16)
                        .fold(0u128, |acc, &b| (acc << 8) | b as u128);

                    let weighted_value = vrf_value / (stake as u128).max(1);

                    match &best_leader {
                        None => best_leader = Some((node.clone(), weighted_value)),
                        Some((_, best_value)) if weighted_value < *best_value => {
                            best_leader = Some((node.clone(), weighted_value));
                        }
                        _ => {}
                    }
                }
                Ok(false) => {
                    log::warn!("Invalid VRF proof from node {} for view {}", node, view);
                }
                Err(e) => {
                    log::error!("VRF verification error for node {}: {}", node, e);
                }
            }
        }
    }

    best_leader.map(|(node, _)| node)
}

/// Deterministic proposer selection (round-robin).
/// Input `nodes` must be a stable, deterministic list of validator ids (sorted canonical order).
/// Returns the proposer NodeId for the given `view`.
pub fn proposer_for_view(nodes: &[NodeId], myself: &NodeId, view: u64) -> NodeId {
    let mut ordered = nodes.to_vec();
    // ensure we include myself if not already present
    if !ordered.contains(myself) {
        ordered.push(myself.clone());
    }
    ordered.sort(); // stable order
    let idx = (view as usize) % ordered.len();
    ordered[idx].clone()
}

/// Alternate helper returning index for testing.
pub fn proposer_index(nodes: &[NodeId], view: u64) -> usize {
    let mut ordered = nodes.to_vec();
    ordered.sort();
    (view as usize) % ordered.len()
}

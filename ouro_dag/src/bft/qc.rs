// src/bft/qc.rs
use crate::bft::consensus::{BlockId, QuorumCertificate, View};
use std::collections::HashSet;

/// Compute f from total nodes n. f = floor((n-1)/3)
pub fn f_from_n(n: usize) -> usize {
    (n.saturating_sub(1)) / 3
}

/// Quorum size (2f+1) given n total nodes.
pub fn quorum_size(n: usize) -> usize {
    2 * f_from_n(n) + 1
}

/// Create a QuorumCertificate from a set of signers
pub fn form_qc(block_id: BlockId, view: View, signers: HashSet<String>) -> QuorumCertificate {
    let signers_vec = signers.into_iter().collect::<Vec<_>>();
    QuorumCertificate {
        block_id,
        view,
        signers: signers_vec,
    }
}

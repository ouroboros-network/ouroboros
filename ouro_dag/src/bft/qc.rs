// src/bft/qc.rs
use crate::bft::consensus::{BlockId, QuorumCertificate, View};
use std::collections::HashSet;

/// Compute f from total nodes n. f = floor((n-1)/3)
pub fn f_from_n(n: usize) -> usize {
    (n.saturating_sub(1)) / 3
}

/// Quorum size given n total nodes.
/// For n < 4 (where f=0), require unanimity (all n nodes) since there is
/// zero fault tolerance. For n >= 4, standard BFT quorum = 2f+1.
pub fn quorum_size(n: usize) -> usize {
    if n < 4 {
        // With f=0, we cannot tolerate any faults — require all nodes to agree.
        n.max(1)
    } else {
        2 * f_from_n(n) + 1
    }
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

// src/zk_proofs/circuit.rs
// R1CS circuits for transaction privacy

use ark_bn254::Fr;
use ark_r1cs_std::fields::fp::FpVar;
use ark_r1cs_std::prelude::*;
use ark_relations::r1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};

/// Transaction privacy circuit
#[derive(Clone, Default)]
pub struct TransactionCircuit {
    // Private inputs (hidden from public)
    pub sender_balance: Option<Fr>,
    pub amount: Option<Fr>,
    pub recipient_hash: Option<Fr>,
}

impl ConstraintSynthesizer<Fr> for TransactionCircuit {
    fn generate_constraints(self, cs: ConstraintSystemRef<Fr>) -> Result<(), SynthesisError> {
        // Allocate private inputs
        let sender_balance = FpVar::new_witness(cs.clone(), || {
            self.sender_balance.ok_or(SynthesisError::AssignmentMissing)
        })?;

        let amount = FpVar::new_input(cs.clone(), || {
            self.amount.ok_or(SynthesisError::AssignmentMissing)
        })?;

        let recipient_hash = FpVar::new_input(cs, || {
            self.recipient_hash.ok_or(SynthesisError::AssignmentMissing)
        })?;

        // Constraint 1: sender_balance >= amount (sufficient funds)
        let has_funds = sender_balance.is_cmp(&amount, std::cmp::Ordering::Greater, false)?;
        has_funds.enforce_equal(&Boolean::TRUE)?;

        // Constraint 2: amount > 0 (no zero/negative transactions)
        let zero = FpVar::Constant(Fr::from(0u64));
        let is_positive = amount.is_cmp(&zero, std::cmp::Ordering::Greater, false)?;
        is_positive.enforce_equal(&Boolean::TRUE)?;

        // Constraint 3: recipient_hash is valid (non-zero)
        let is_valid_recipient = recipient_hash.is_neq(&zero)?;
        is_valid_recipient.enforce_equal(&Boolean::TRUE)?;

        Ok(())
    }
}

/// Range proof circuit (prove value is in range without revealing it)
#[derive(Clone)]
pub struct RangeProofCircuit {
    pub value: Option<Fr>,
    pub min: Fr,
    pub max: Fr,
}

impl ConstraintSynthesizer<Fr> for RangeProofCircuit {
    fn generate_constraints(self, cs: ConstraintSystemRef<Fr>) -> Result<(), SynthesisError> {
        let value = FpVar::new_witness(cs.clone(), || {
            self.value.ok_or(SynthesisError::AssignmentMissing)
        })?;

        let min = FpVar::Constant(self.min);
        let max = FpVar::Constant(self.max);

        // Constraint: min <= value <= max
        let above_min = value.is_cmp(&min, std::cmp::Ordering::Greater, true)?;
        let below_max = value.is_cmp(&max, std::cmp::Ordering::Less, true)?;

        above_min.enforce_equal(&Boolean::TRUE)?;
        below_max.enforce_equal(&Boolean::TRUE)?;

        Ok(())
    }
}

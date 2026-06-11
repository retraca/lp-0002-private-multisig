//! Host prover for LP-0002 private multisig.

use anyhow::Result;
use risc0_zkvm::{ExecutorEnv, Receipt};
use serde::{Deserialize, Serialize};

use crate::methods::PRIVATE_MULTISIG_GUEST_ELF;

#[derive(Serialize, Deserialize)]
pub struct ProverInput {
    pub nsk: [u8; 32],
    pub member_index: usize,
    pub member_commitments: Vec<[u8; 32]>,
    pub multisig_id: [u8; 32],
    pub proposal_id: [u8; 32],
}

pub fn prove(input: ProverInput) -> Result<Receipt> {
    let env = ExecutorEnv::builder()
        .write(&input)?
        .build()?;
    let prover = risc0_zkvm::default_prover();
    let receipt = prover.prove(env, PRIVATE_MULTISIG_GUEST_ELF)?.receipt;
    Ok(receipt)
}

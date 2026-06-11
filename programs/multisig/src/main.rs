//! Guest binary entry point for LP-0002 private multisig on-chain program.
//! Deploy with: cargo +risc0 build --release --target riscv32im-risc0-zkvm-elf
//!              wallet deploy-program target/riscv32im-risc0-zkvm-elf/release/private_multisig
//!
//! Proof verification model: the vote receipt is passed as a zkVM assumption.
//! The SPEL program calls env::verify(IMAGE_ID, journal_words) to bind the proof
//! to this specific circuit -- the correct LEZ-native pattern (see privacy_preserving_circuit.rs).

#![no_main]

use borsh::BorshDeserialize;
use nssa_core::account::{AccountWithMetadata, Data};
use private_multisig_program::{
    apply_execute, apply_vote, MultisigState, Proposal, VoteJournal,
    ERR_INVALID_THRESHOLD, ERR_PROOF_INVALID, ERR_TOO_MANY_MEMBERS, ERR_TOO_MANY_PROPOSALS,
    MAX_MEMBERS, MAX_PROPOSALS,
};
use risc0_zkvm::guest::env;
use spel_framework::prelude::*;

include!(concat!(env!("OUT_DIR"), "/methods.rs"));
use PRIVATE_MULTISIG_GUEST_ID as IMAGE_ID;

risc0_zkvm::guest::entry!(main);

#[lez_program]
mod multisig {

    /// Initialize a new M-of-N multisig with member commitments and threshold.
    #[instruction]
    pub fn initialize(
        #[account(init)] mut multisig_account: AccountWithMetadata,
        threshold: u8,
        member_commitments: Vec<[u8; 32]>,
    ) -> SpelResult {
        if member_commitments.is_empty() || member_commitments.len() > MAX_MEMBERS {
            return Err(SpelError::Custom {
                code: ERR_TOO_MANY_MEMBERS,
                message: "invalid member count".to_string(),
            });
        }
        if threshold == 0 || threshold as usize > member_commitments.len() {
            return Err(SpelError::Custom {
                code: ERR_INVALID_THRESHOLD,
                message: "invalid threshold".to_string(),
            });
        }
        let state = MultisigState {
            threshold,
            member_commitments,
            proposals: Vec::new(),
        };
        multisig_account.account.data =
            Data::try_from(borsh::to_vec(&state).map_err(|e| SpelError::SerializationError {
                message: e.to_string(),
            })?)
            .map_err(|e| SpelError::SerializationError {
                message: format!("state too large: {e:?}"),
            })?;
        Ok(SpelOutput::execute(vec![multisig_account], vec![]))
    }

    /// Submit a new proposal with opaque action bytes.
    #[instruction]
    pub fn submit_proposal(
        #[account(mut)] mut multisig_account: AccountWithMetadata,
        proposal_id: [u8; 32],
        action_bytes: Vec<u8>,
    ) -> SpelResult {
        let mut state =
            MultisigState::try_from_slice(multisig_account.account.data.as_ref())
                .map_err(|_| SpelError::Custom {
                    code: ERR_PROOF_INVALID,
                    message: "state deserialise failed".to_string(),
                })?;

        if state.proposals.len() >= MAX_PROPOSALS {
            return Err(SpelError::Custom {
                code: ERR_TOO_MANY_PROPOSALS,
                message: "proposal capacity reached".to_string(),
            });
        }
        state.proposals.push(Proposal {
            id: proposal_id,
            action_bytes,
            vote_count: 0,
            executed: false,
            spent_nullifiers: Vec::new(),
        });

        multisig_account.account.data =
            Data::try_from(borsh::to_vec(&state).map_err(|e| SpelError::SerializationError {
                message: e.to_string(),
            })?)
            .map_err(|e| SpelError::SerializationError {
                message: format!("state too large: {e:?}"),
            })?;
        Ok(SpelOutput::execute(vec![multisig_account], vec![]))
    }

    /// Cast a vote via a RISC0 ZK proof of membership.
    ///
    /// The receipt for IMAGE_ID must be provided as a zkVM assumption before calling.
    /// `journal_bytes` is borsh-serialized: [u8;32] multisig_id, [u8;32] proposal_id,
    /// [u8;32] nullifier, [u8;32] member_set_root.
    #[instruction]
    pub fn vote(
        #[account(mut)] mut multisig_account: AccountWithMetadata,
        proposal_id: [u8; 32],
        journal_bytes: Vec<u8>,
    ) -> SpelResult {
        let mut state =
            MultisigState::try_from_slice(multisig_account.account.data.as_ref())
                .map_err(|_| SpelError::Custom {
                    code: ERR_PROOF_INVALID,
                    message: "state deserialise failed".to_string(),
                })?;

        let journal_words: Vec<u32> =
            risc0_zkvm::serde::to_vec(&journal_bytes).map_err(|_| SpelError::Custom {
                code: ERR_PROOF_INVALID,
                message: "journal serialise failed".to_string(),
            })?;
        env::verify(IMAGE_ID, &journal_words).map_err(|_| SpelError::Custom {
            code: ERR_PROOF_INVALID,
            message: "assumption verification failed".to_string(),
        })?;

        #[derive(borsh::BorshDeserialize)]
        struct VoteJournalRaw {
            multisig_id: [u8; 32],
            proposal_id: [u8; 32],
            nullifier: [u8; 32],
            member_set_root: [u8; 32],
        }

        let j = VoteJournalRaw::try_from_slice(&journal_bytes).map_err(|_| SpelError::Custom {
            code: ERR_PROOF_INVALID,
            message: "journal decode failed".to_string(),
        })?;

        let journal = VoteJournal {
            multisig_id: j.multisig_id,
            proposal_id: j.proposal_id,
            nullifier: j.nullifier,
            member_set_root: j.member_set_root,
        };

        let multisig_id: [u8; 32] = *multisig_account.account_id.value();
        apply_vote(&mut state, &journal, multisig_id, proposal_id)?;

        multisig_account.account.data =
            Data::try_from(borsh::to_vec(&state).map_err(|e| SpelError::SerializationError {
                message: e.to_string(),
            })?)
            .map_err(|e| SpelError::SerializationError {
                message: format!("state too large: {e:?}"),
            })?;
        Ok(SpelOutput::execute(vec![multisig_account], vec![]))
    }

    /// Execute a proposal once the threshold is met.
    #[instruction]
    pub fn execute(
        #[account(mut)] mut multisig_account: AccountWithMetadata,
        proposal_id: [u8; 32],
    ) -> SpelResult {
        let mut state =
            MultisigState::try_from_slice(multisig_account.account.data.as_ref())
                .map_err(|_| SpelError::Custom {
                    code: ERR_PROOF_INVALID,
                    message: "state deserialise failed".to_string(),
                })?;

        apply_execute(&mut state, proposal_id)?;

        multisig_account.account.data =
            Data::try_from(borsh::to_vec(&state).map_err(|e| SpelError::SerializationError {
                message: e.to_string(),
            })?)
            .map_err(|e| SpelError::SerializationError {
                message: format!("state too large: {e:?}"),
            })?;
        Ok(SpelOutput::execute(vec![multisig_account], vec![]))
    }
}

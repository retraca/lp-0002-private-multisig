//! LP-0002 vote-circuit program: the membership proof for private multisig votes.
//!
//! Runs as the INITIAL call of a privacy-preserving transaction, executed and
//! proven CLIENT-SIDE by the voter. The instruction data of the initial call
//! (containing the voter's nsk) never appears on-chain: the sequencer sees only
//! the PPE circuit output (public account post-states, nullifiers).
//!
//! The program:
//!   1. Reads the multisig account state (pre-state supplied by the client and
//!      checked against live chain state by the sequencer at submission).
//!   2. Recomputes the member commitment SHA256("member" || nsk || multisig_id)
//!      and asserts it matches the registered commitment at `member_index`.
//!   3. Derives `member_set_root` (binds the proof to the exact membership set)
//!      and the per-proposal nullifier
//!      SHA256("multisig/v1/vote" || nsk || proposal_id || multisig_id).
//!   4. Declares a ChainedCall into the multisig program's `vote` instruction.
//!      The PPE outer circuit proves this linkage; the multisig program accepts
//!      votes only from this program (caller_program_id check).
//!
//! The journal reveals which multisig and proposal received a vote and the
//! nullifier -- never which member voted.

#![no_main]

use borsh::BorshDeserialize;
use nssa_core::{account::AccountWithMetadata, program::ChainedCall};
use private_multisig_program::{MultisigState, ERR_MEMBER_NOT_REGISTERED, ERR_PROOF_INVALID};
use sha2::{Digest, Sha256};
use spel_framework::prelude::*;

risc0_zkvm::guest::entry!(main);

/// Wire-format mirror of the multisig program's SPEL-generated instruction
/// enum. Variant order must match the declaration order in
/// `programs/multisig/src/main.rs`: 0=initialize, 1=submit_proposal, 2=vote,
/// 3=execute. Only `Vote` is constructed here; risc0 serde encodes the variant
/// index plus fields, so unit placeholders keep the indices aligned.
#[derive(serde::Serialize)]
enum MultisigInstruction {
    #[allow(dead_code)]
    Initialize,
    #[allow(dead_code)]
    SubmitProposal,
    Vote([u8; 32], Vec<u8>),
    #[allow(dead_code)]
    Execute,
}

fn member_commitment(nsk: &[u8; 32], multisig_id: &[u8; 32]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"member");
    h.update(nsk);
    h.update(multisig_id);
    h.finalize().into()
}

fn member_set_root(commitments: &[[u8; 32]]) -> [u8; 32] {
    let mut h = Sha256::new();
    for c in commitments {
        h.update(c);
    }
    h.finalize().into()
}

fn vote_nullifier(nsk: &[u8; 32], proposal_id: &[u8; 32], multisig_id: &[u8; 32]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"multisig/v1/vote");
    h.update(nsk);
    h.update(proposal_id);
    h.update(multisig_id);
    h.finalize().into()
}

#[lez_program]
mod vote_circuit {

    /// Prove membership and deliver a vote to the multisig program.
    ///
    /// `nsk` is a PRIVATE input: initial-call instruction data is never
    /// revealed on-chain in a privacy-preserving transaction.
    /// `multisig_program_id` selects the target program; the target itself
    /// pins this program's ID in its state, so pointing the call elsewhere
    /// cannot forge votes.
    ///
    /// `voter_note` is a fresh zero-balance private account created by this
    /// transaction. It gives the privacy-preserving transaction its required
    /// commitment/nullifier pair and makes a vote indistinguishable from any
    /// other private transaction. This program passes it through unchanged.
    #[instruction]
    pub fn submit_vote(
        #[account(mut)] multisig_account: AccountWithMetadata,
        #[account(mut)] voter_note: AccountWithMetadata,
        multisig_program_id: [u32; 8],
        nsk: [u8; 32],
        member_index: u32,
        proposal_id: [u8; 32],
    ) -> SpelResult {
        let state = MultisigState::try_from_slice(multisig_account.account.data.as_ref())
            .map_err(|_| SpelError::Custom {
                code: ERR_PROOF_INVALID,
                message: "multisig state deserialise failed".to_string(),
            })?;

        let multisig_id: [u8; 32] = *multisig_account.account_id.value();

        // Membership check: the recomputed commitment must match the
        // registered one. This is the heart of the proof -- it can only
        // succeed for a holder of a registered nsk.
        let expected = state
            .member_commitments
            .get(member_index as usize)
            .ok_or(SpelError::Custom {
                code: ERR_MEMBER_NOT_REGISTERED,
                message: "member index out of range".to_string(),
            })?;
        if member_commitment(&nsk, &multisig_id) != *expected {
            return Err(SpelError::Custom {
                code: ERR_MEMBER_NOT_REGISTERED,
                message: "nsk does not match registered commitment".to_string(),
            });
        }

        let root = member_set_root(&state.member_commitments);
        let nullifier = vote_nullifier(&nsk, &proposal_id, &multisig_id);

        // borsh VoteJournal: multisig_id, proposal_id, nullifier, member_set_root
        let mut journal_bytes = Vec::with_capacity(128);
        journal_bytes.extend_from_slice(&multisig_id);
        journal_bytes.extend_from_slice(&proposal_id);
        journal_bytes.extend_from_slice(&nullifier);
        journal_bytes.extend_from_slice(&root);

        let chained_call = ChainedCall::new(
            multisig_program_id,
            vec![multisig_account.clone()],
            &MultisigInstruction::Vote(proposal_id, journal_bytes),
        );

        // Post-states must mirror pre-states 1:1; this program reads both
        // accounts without modifying them -- the chained vote call performs
        // the multisig state update, and the PPE circuit handles the voter
        // note commitment.
        Ok(SpelOutput::execute(
            vec![multisig_account, voter_note],
            vec![chained_call],
        ))
    }
}

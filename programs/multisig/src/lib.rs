//! On-chain private M-of-N multisig program for LP-0002 (LEZ v0.2.0).
//!
//! Member identities are fully private: approvals leave no public trace of
//! which members voted. The on-chain state reveals only the threshold and
//! whether it was reached.
//!
//! Architecture (privacy-preserving initial call, single program):
//!   - Members register at initialization: each submits a commitment
//!     SHA256("member" || nsk || multisig_id) where nsk is the member's REAL
//!     shielded-account nullifier secret key (HD-derived by their wallet).
//!   - To vote, a member submits a privacy-preserving transaction whose
//!     initial call is THIS program's `Vote` instruction. Initial-call
//!     instruction data (containing the nsk) never appears on-chain: the
//!     sequencer sees only the composite proof and the public post-state.
//!   - In-circuit live-account binding: the vote rides the member's LIVE
//!     shielded voting account. The guest asserts the rider's account id is
//!     derived from the SAME nsk (`AccountId::for_regular_private_account(
//!     npk(nsk), VOTE_IDENTIFIER)`) and that the account is not default
//!     (i.e. it exists on chain), and the LEZ privacy circuit proves the
//!     rider's pre-state commitment is in the live commitment tree. An
//!     anonymous vote therefore cannot be cast without controlling a live
//!     shielded account enrolled in the member set.
//!   - The program tracks per-proposal nullifiers
//!     SHA256("multisig/v1/vote" || nsk || proposal_id || multisig_id) to
//!     reject double votes. When the vote count reaches the threshold, the
//!     proposal can be executed (a threshold-gated parameter change: the
//!     proposal's action bytes become the executed action).
//!
//! LEZ nonce constraint: the multisig account itself is program-owned after
//! `Initialize` (claimed via a signed transaction, key discarded), so
//! proposals, votes and execution need no wallet signatures on it. The
//! member's voting account is a regular shielded account; the privacy circuit
//! rotates its commitment and nonce on every vote like any private transfer,
//! which is exactly what makes a vote indistinguishable from one.

use borsh::{BorshDeserialize, BorshSerialize};
use risc0_zkvm::sha::{Impl, Sha256 as _};
use serde::{Deserialize, Serialize};

/// The private-account `Identifier` (u128) under which every member holds
/// their voting account: `AccountId::for_regular_private_account(npk, VOTE_IDENTIFIER)`.
/// The guest binds the vote's rider to this exact identifier.
pub const VOTE_IDENTIFIER: u128 = 0;

pub const ERR_PROOF_INVALID: u32 = 6001;
pub const ERR_MULTISIG_MISMATCH: u32 = 6002;
pub const ERR_PROPOSAL_NOT_FOUND: u32 = 6003;
pub const ERR_NULLIFIER_SPENT: u32 = 6004;
pub const ERR_MEMBER_NOT_REGISTERED: u32 = 6005;
pub const ERR_PROPOSAL_ALREADY_EXECUTED: u32 = 6006;
pub const ERR_THRESHOLD_NOT_MET: u32 = 6007;
pub const ERR_ALREADY_INITIALIZED: u32 = 6008;
pub const ERR_TOO_MANY_MEMBERS: u32 = 6009;
pub const ERR_INVALID_THRESHOLD: u32 = 6010;
pub const ERR_TOO_MANY_PROPOSALS: u32 = 6011;
/// The vote's rider account id is not derived from the voting nsk.
pub const ERR_RIDER_MISMATCH: u32 = 6012;
/// The vote's rider account is default/uninitialized (not a live account).
pub const ERR_RIDER_NOT_LIVE: u32 = 6013;

pub const MAX_MEMBERS: usize = 20;
pub const MAX_PROPOSALS: usize = 100;

/// Program error: a documented code plus a human-readable message. In-guest
/// these abort the proof via panic (the panic string carries `ERR_<code>`,
/// making every failure deterministic and greppable); host-side they are
/// ordinary values for unit tests and the CLI.
#[derive(Debug, PartialEq, Eq)]
pub struct MultisigError {
    pub code: u32,
    pub message: &'static str,
}

impl MultisigError {
    pub const fn new(code: u32, message: &'static str) -> Self {
        Self { code, message }
    }
}

impl core::fmt::Display for MultisigError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "ERR_{} {}", self.code, self.message)
    }
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    Impl::hash_bytes(bytes).as_bytes().try_into().unwrap()
}

/// A member's registration commitment: SHA256("member" || nsk || multisig_id).
/// Only this one-way hash is published; the nsk (and hence the member's
/// shielded account) is never linkable from it.
pub fn member_commitment(nsk: &[u8; 32], multisig_id: &[u8; 32]) -> [u8; 32] {
    let mut buf = Vec::with_capacity(6 + 64);
    buf.extend_from_slice(b"member");
    buf.extend_from_slice(nsk);
    buf.extend_from_slice(multisig_id);
    sha256(&buf)
}

/// The member set root: SHA256(commitment[0] || ... || commitment[N-1]).
/// Binds a vote to the exact registered set.
pub fn compute_member_set_root(commitments: &[[u8; 32]]) -> [u8; 32] {
    let mut buf = Vec::with_capacity(commitments.len() * 32);
    for c in commitments {
        buf.extend_from_slice(c);
    }
    sha256(&buf)
}

/// Per-proposal vote nullifier:
/// SHA256("multisig/v1/vote" || nsk || proposal_id || multisig_id).
/// Same member + same proposal => same nullifier => second vote rejected.
pub fn vote_nullifier(nsk: &[u8; 32], proposal_id: &[u8; 32], multisig_id: &[u8; 32]) -> [u8; 32] {
    let mut buf = Vec::with_capacity(16 + 96);
    buf.extend_from_slice(b"multisig/v1/vote");
    buf.extend_from_slice(nsk);
    buf.extend_from_slice(proposal_id);
    buf.extend_from_slice(multisig_id);
    sha256(&buf)
}

/// Instruction set. Serialized with risc0 serde (`Program::serialize_instruction`).
/// `Vote`'s fields (the nsk!) are initial-call instruction data of a
/// privacy-preserving transaction and never appear on-chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MultisigInstruction {
    /// Claim the multisig account (signed with its key, then program-owned
    /// forever) and register threshold + member commitments.
    Initialize {
        threshold: u8,
        commitments: Vec<[u8; 32]>,
    },
    /// Record a new proposal (unsigned: account is program-owned).
    SubmitProposal {
        proposal_id: [u8; 32],
        action: Vec<u8>,
    },
    /// Anonymous approval (privacy-preserving transaction, nsk private).
    Vote {
        nsk: [u8; 32],
        member_index: u32,
        proposal_id: [u8; 32],
    },
    /// Execute once vote_count >= threshold (unsigned).
    Execute { proposal_id: [u8; 32] },
}

/// Per-proposal state.
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct Proposal {
    pub id: [u8; 32],
    /// Opaque action bytes (e.g. a serialized parameter change).
    pub action_bytes: Vec<u8>,
    /// Number of distinct valid votes received.
    pub vote_count: u8,
    /// Whether the proposal has been executed.
    pub executed: bool,
    /// Spent nullifiers for this proposal -- prevents double-voting.
    pub spent_nullifiers: Vec<[u8; 32]>,
}

/// Multisig state stored in the (program-owned) multisig account's data.
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct MultisigState {
    /// M: minimum approvals needed.
    pub threshold: u8,
    /// Registered member commitments: SHA256("member" || nsk || multisig_id).
    pub member_commitments: Vec<[u8; 32]>,
    /// Active and completed proposals.
    pub proposals: Vec<Proposal>,
}

impl MultisigState {
    pub fn validate_new(threshold: u8, commitments: &[[u8; 32]]) -> Result<(), MultisigError> {
        if commitments.len() > MAX_MEMBERS {
            return Err(MultisigError::new(ERR_TOO_MANY_MEMBERS, "too many members"));
        }
        if threshold == 0 || threshold as usize > commitments.len() {
            return Err(MultisigError::new(
                ERR_INVALID_THRESHOLD,
                "threshold must be 1..=N",
            ));
        }
        Ok(())
    }
}

/// Core proposal-submission validation.
pub fn apply_submit_proposal(
    state: &mut MultisigState,
    proposal_id: [u8; 32],
    action_bytes: Vec<u8>,
) -> Result<(), MultisigError> {
    if state.proposals.len() >= MAX_PROPOSALS {
        return Err(MultisigError::new(ERR_TOO_MANY_PROPOSALS, "too many proposals"));
    }
    if state.proposals.iter().any(|p| p.id == proposal_id) {
        return Err(MultisigError::new(
            ERR_ALREADY_INITIALIZED,
            "proposal id already exists",
        ));
    }
    state.proposals.push(Proposal {
        id: proposal_id,
        action_bytes,
        vote_count: 0,
        executed: false,
        spent_nullifiers: Vec::new(),
    });
    Ok(())
}

/// Core vote validation against mutable multisig state. Pure function so the
/// state machine is unit-testable outside the zkVM; the guest calls this
/// after the membership and rider-binding asserts.
pub fn apply_vote(
    state: &mut MultisigState,
    nsk: &[u8; 32],
    member_index: u32,
    proposal_id: [u8; 32],
    multisig_id: [u8; 32],
) -> Result<(), MultisigError> {
    let expected = state
        .member_commitments
        .get(member_index as usize)
        .ok_or(MultisigError::new(
            ERR_MEMBER_NOT_REGISTERED,
            "member index out of range",
        ))?;
    if member_commitment(nsk, &multisig_id) != *expected {
        return Err(MultisigError::new(
            ERR_MEMBER_NOT_REGISTERED,
            "nsk does not match registered commitment",
        ));
    }

    let nullifier = vote_nullifier(nsk, &proposal_id, &multisig_id);

    let proposal = state
        .proposals
        .iter_mut()
        .find(|p| p.id == proposal_id)
        .ok_or(MultisigError::new(ERR_PROPOSAL_NOT_FOUND, "proposal not found"))?;

    if proposal.executed {
        return Err(MultisigError::new(
            ERR_PROPOSAL_ALREADY_EXECUTED,
            "proposal already executed",
        ));
    }
    if proposal.spent_nullifiers.contains(&nullifier) {
        return Err(MultisigError::new(
            ERR_NULLIFIER_SPENT,
            "nullifier already spent (double vote)",
        ));
    }

    proposal.spent_nullifiers.push(nullifier);
    proposal.vote_count += 1;
    Ok(())
}

/// Core execute validation against mutable multisig state.
pub fn apply_execute(
    state: &mut MultisigState,
    proposal_id: [u8; 32],
) -> Result<(), MultisigError> {
    let threshold = state.threshold;
    let proposal = state
        .proposals
        .iter_mut()
        .find(|p| p.id == proposal_id)
        .ok_or(MultisigError::new(ERR_PROPOSAL_NOT_FOUND, "proposal not found"))?;

    if proposal.executed {
        return Err(MultisigError::new(
            ERR_PROPOSAL_ALREADY_EXECUTED,
            "proposal already executed",
        ));
    }
    if proposal.vote_count < threshold {
        return Err(MultisigError::new(ERR_THRESHOLD_NOT_MET, "threshold not met"));
    }

    proposal.executed = true;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo_state() -> (MultisigState, Vec<[u8; 32]>, [u8; 32]) {
        let multisig_id = [7u8; 32];
        let nsks: Vec<[u8; 32]> = vec![[1u8; 32], [2u8; 32], [3u8; 32]];
        let commitments: Vec<[u8; 32]> = nsks
            .iter()
            .map(|nsk| member_commitment(nsk, &multisig_id))
            .collect();
        let mut state = MultisigState {
            threshold: 2,
            member_commitments: commitments,
            proposals: Vec::new(),
        };
        apply_submit_proposal(&mut state, [9u8; 32], b"set-param=42".to_vec()).unwrap();
        (state, nsks, multisig_id)
    }

    #[test]
    fn two_of_three_lifecycle() {
        let (mut state, nsks, mid) = demo_state();
        apply_vote(&mut state, &nsks[0], 0, [9u8; 32], mid).unwrap();
        assert_eq!(
            apply_execute(&mut state, [9u8; 32]).unwrap_err().code,
            ERR_THRESHOLD_NOT_MET
        );
        apply_vote(&mut state, &nsks[1], 1, [9u8; 32], mid).unwrap();
        apply_execute(&mut state, [9u8; 32]).unwrap();
        assert!(state.proposals[0].executed);
        assert_eq!(state.proposals[0].vote_count, 2);
        assert_eq!(state.proposals[0].spent_nullifiers.len(), 2);
    }

    #[test]
    fn double_vote_rejected() {
        let (mut state, nsks, mid) = demo_state();
        apply_vote(&mut state, &nsks[0], 0, [9u8; 32], mid).unwrap();
        assert_eq!(
            apply_vote(&mut state, &nsks[0], 0, [9u8; 32], mid).unwrap_err().code,
            ERR_NULLIFIER_SPENT
        );
    }

    #[test]
    fn unregistered_nsk_rejected() {
        let (mut state, _, mid) = demo_state();
        assert_eq!(
            apply_vote(&mut state, &[42u8; 32], 0, [9u8; 32], mid).unwrap_err().code,
            ERR_MEMBER_NOT_REGISTERED
        );
    }

    #[test]
    fn execute_twice_rejected() {
        let (mut state, nsks, mid) = demo_state();
        apply_vote(&mut state, &nsks[0], 0, [9u8; 32], mid).unwrap();
        apply_vote(&mut state, &nsks[2], 2, [9u8; 32], mid).unwrap();
        apply_execute(&mut state, [9u8; 32]).unwrap();
        assert_eq!(
            apply_execute(&mut state, [9u8; 32]).unwrap_err().code,
            ERR_PROPOSAL_ALREADY_EXECUTED
        );
    }

    #[test]
    fn nullifiers_unlinkable_across_proposals() {
        let (mut state, nsks, mid) = demo_state();
        apply_submit_proposal(&mut state, [10u8; 32], b"p2".to_vec()).unwrap();
        apply_vote(&mut state, &nsks[0], 0, [9u8; 32], mid).unwrap();
        apply_vote(&mut state, &nsks[0], 0, [10u8; 32], mid).unwrap();
        assert_ne!(
            state.proposals[0].spent_nullifiers[0],
            state.proposals[1].spent_nullifiers[0]
        );
    }

    #[test]
    fn invalid_threshold_rejected() {
        assert_eq!(
            MultisigState::validate_new(0, &[[1u8; 32]]).unwrap_err().code,
            ERR_INVALID_THRESHOLD
        );
        assert_eq!(
            MultisigState::validate_new(2, &[[1u8; 32]]).unwrap_err().code,
            ERR_INVALID_THRESHOLD
        );
    }
}

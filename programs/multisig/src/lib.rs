//! On-chain private M-of-N multisig program for LP-0002.
//!
//! Member identities are fully private: approvals leave no public trace of
//! which members voted. The on-chain state reveals only the threshold and
//! whether it was reached.
//!
//! Architecture (chained-call composition over the LEZ privacy-preserving
//! execution pipeline):
//!   - Members register at initialization: each submits a commitment
//!     SHA256("member" || nsk || multisig_id) where nsk is a fresh client-side key.
//!   - To vote, a member submits a privacy-preserving transaction whose initial
//!     call is the vote-circuit program (`programs/vote_circuit`). That program
//!     receives the nsk as a PRIVATE input (initial-call instruction data never
//!     appears on-chain), recomputes the member commitment against the multisig
//!     account state, derives the per-proposal nullifier and the member set
//!     root, and declares a ChainedCall into this program's `vote` instruction.
//!   - `vote` trusts its caller identity: the PPE outer circuit proves the
//!     chained-call linkage (caller_program_id cannot be spoofed), so checking
//!     `ctx.caller_program_id == state.vote_circuit_program_id` is equivalent
//!     to verifying the membership proof itself.
//!   - The program tracks per-proposal nullifiers. When the vote count reaches
//!     the threshold, the proposal can be executed.
//!
//! LEZ nonce constraint: members never spend from their shielded accounts to
//! participate in governance; the voting key material (nsk) is separate.

use borsh::{BorshDeserialize, BorshSerialize};
use sha2::{Digest, Sha256};
use spel_framework::error::SpelError;

/// Compute the member set root used by the vote circuit:
/// SHA256(commitment[0] || ... || commitment[N-1]).
pub fn compute_member_set_root(commitments: &[[u8; 32]]) -> [u8; 32] {
    let mut h = Sha256::new();
    for c in commitments {
        h.update(c);
    }
    h.finalize().into()
}

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
/// `vote` was invoked by something other than the registered vote-circuit
/// program. Only chained calls from that program carry a valid membership proof.
pub const ERR_UNAUTHORIZED_CALLER: u32 = 6012;

pub const MAX_MEMBERS: usize = 20;
pub const MAX_PROPOSALS: usize = 100;

/// Per-proposal state.
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct Proposal {
    pub id: [u8; 32],
    /// Opaque action bytes (e.g. a serialised downstream instruction).
    pub action_bytes: Vec<u8>,
    /// Number of distinct valid votes received.
    pub vote_count: u8,
    /// Whether the proposal has been executed.
    pub executed: bool,
    /// Spent nullifiers for this proposal -- prevents double-voting.
    pub spent_nullifiers: Vec<[u8; 32]>,
}

/// Multisig state stored on-chain.
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct MultisigState {
    /// M: minimum approvals needed.
    pub threshold: u8,
    /// Program ID of the vote-circuit program authorized to deliver votes
    /// (chained-call caller). Set once at initialization.
    pub vote_circuit_program_id: [u32; 8],
    /// Registered member commitments: SHA256("member" || nsk || multisig_id).
    pub member_commitments: Vec<[u8; 32]>,
    /// Active and completed proposals.
    pub proposals: Vec<Proposal>,
}

/// Decoded vote journal (public outputs of the vote circuit, delivered as
/// chained-call instruction data).
pub struct VoteJournal {
    pub multisig_id: [u8; 32],
    pub proposal_id: [u8; 32],
    pub nullifier: [u8; 32],
    pub member_set_root: [u8; 32],
}

/// Core vote validation against mutable multisig state.
/// Separated from instruction plumbing so the state-machine logic is
/// unit-testable without a chained-call context.
pub fn apply_vote(
    state: &mut MultisigState,
    journal: &VoteJournal,
    multisig_id: [u8; 32],
    proposal_id: [u8; 32],
) -> Result<(), SpelError> {
    if journal.multisig_id != multisig_id {
        return Err(SpelError::Custom { code: ERR_MULTISIG_MISMATCH, message: "multisig id mismatch".to_string() });
    }
    if journal.proposal_id != proposal_id {
        return Err(SpelError::Custom { code: ERR_PROPOSAL_NOT_FOUND, message: "proposal id mismatch".to_string() });
    }

    let expected_root = compute_member_set_root(&state.member_commitments);
    if journal.member_set_root != expected_root {
        return Err(SpelError::Custom { code: ERR_MEMBER_NOT_REGISTERED, message: "member set root mismatch".to_string() });
    }

    let proposal = state.proposals.iter_mut()
        .find(|p| p.id == proposal_id)
        .ok_or(SpelError::Custom { code: ERR_PROPOSAL_NOT_FOUND, message: "proposal not found".to_string() })?;

    if proposal.executed {
        return Err(SpelError::Custom { code: ERR_PROPOSAL_ALREADY_EXECUTED, message: "proposal already executed".to_string() });
    }

    if proposal.spent_nullifiers.contains(&journal.nullifier) {
        return Err(SpelError::Custom { code: ERR_NULLIFIER_SPENT, message: "nullifier already spent".to_string() });
    }

    proposal.spent_nullifiers.push(journal.nullifier);
    proposal.vote_count += 1;

    Ok(())
}

/// Core execute validation against mutable multisig state.
pub fn apply_execute(
    state: &mut MultisigState,
    proposal_id: [u8; 32],
) -> Result<(), SpelError> {
    let proposal = state.proposals.iter_mut()
        .find(|p| p.id == proposal_id)
        .ok_or(SpelError::Custom { code: ERR_PROPOSAL_NOT_FOUND, message: "proposal not found".to_string() })?;

    if proposal.executed {
        return Err(SpelError::Custom { code: ERR_PROPOSAL_ALREADY_EXECUTED, message: "proposal already executed".to_string() });
    }
    if proposal.vote_count < state.threshold {
        return Err(SpelError::Custom { code: ERR_THRESHOLD_NOT_MET, message: "threshold not met".to_string() });
    }

    proposal.executed = true;
    Ok(())
}

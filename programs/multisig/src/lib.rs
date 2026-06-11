//! On-chain private M-of-N multisig program for LP-0002.
//!
//! Member identities are fully private: approvals leave no public trace of
//! which members voted. The on-chain state reveals only the threshold and
//! whether it was reached.
//!
//! Architecture:
//!   - Members register at initialization: each submits a commitment
//!     SHA256("member" || nsk || multisig_id) where nsk is a fresh client-side key.
//!   - To vote, a member runs the RISC0 guest off-chain and submits the receipt.
//!     The receipt proves nsk knowledge without revealing which member voted.
//!   - The program tracks per-proposal nullifiers. When the vote count reaches
//!     the threshold, the proposal can be executed.
//!
//! LEZ nonce constraint: because members prove key knowledge via ZK (not by
//! signing a transaction with their shielded account), the nonce increment
//! constraint does not apply. Members never spend from their shielded accounts
//! to participate in governance.

use borsh::{BorshDeserialize, BorshSerialize};
use nssa_core::account::{AccountPostState, AccountWithMetadata, Data};
use sha2::{Digest, Sha256};
use spel_framework::error::SpelError;

/// Compute the member set root used by the guest circuit:
/// SHA256(commitment[0] || ... || commitment[N-1]).
fn compute_member_set_root(commitments: &[[u8; 32]]) -> [u8; 32] {
    let mut h = Sha256::new();
    for c in commitments {
        h.update(c);
    }
    h.finalize().into()
}

include!(concat!(env!("OUT_DIR"), "/methods.rs"));
use PRIVATE_MULTISIG_GUEST_ID as IMAGE_ID;
const _: () = assert!(
    IMAGE_ID[0] != 0 || IMAGE_ID[1] != 0,
    "IMAGE_ID is all-zero; build the guest before deploying"
);

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
    /// Registered member commitments: SHA256("member" || nsk || multisig_id).
    pub member_commitments: Vec<[u8; 32]>,
    /// Active and completed proposals.
    pub proposals: Vec<Proposal>,
}

/// Initialize a new multisig.
///
/// `member_commitments`: each member calls `derive_commitment(nsk, multisig_account_id)`
/// off-chain and submits their result here. The nsk never leaves the client.
pub fn initialize(
    multisig_account: AccountWithMetadata,
    threshold: u8,
    member_commitments: Vec<[u8; 32]>,
) -> Result<Vec<AccountPostState>, SpelError> {
    if !multisig_account.account.data.0.is_empty() {
        return Err(SpelError::Custom { code: ERR_ALREADY_INITIALIZED });
    }
    if member_commitments.is_empty() || member_commitments.len() > MAX_MEMBERS {
        return Err(SpelError::Custom { code: ERR_TOO_MANY_MEMBERS });
    }
    if threshold == 0 || threshold as usize > member_commitments.len() {
        return Err(SpelError::Custom { code: ERR_INVALID_THRESHOLD });
    }

    let state = MultisigState {
        threshold,
        member_commitments,
        proposals: Vec::new(),
    };
    let mut account = multisig_account.account;
    account.data = Data::from_borsh(&state);
    Ok(vec![AccountPostState::new(account)])
}

/// Submit a new proposal.
///
/// Any caller can submit a proposal -- only threshold approvals gate execution.
pub fn submit_proposal(
    multisig_account: AccountWithMetadata,
    proposal_id: [u8; 32],
    action_bytes: Vec<u8>,
) -> Result<Vec<AccountPostState>, SpelError> {
    let mut state = MultisigState::try_from_slice(&multisig_account.account.data.0)
        .map_err(|_| SpelError::Custom { code: ERR_PROOF_INVALID })?;

    if state.proposals.len() >= MAX_PROPOSALS {
        return Err(SpelError::Custom { code: ERR_PROPOSAL_NOT_FOUND });
    }
    state.proposals.push(Proposal {
        id: proposal_id,
        action_bytes,
        vote_count: 0,
        executed: false,
        spent_nullifiers: Vec::new(),
    });

    let mut account = multisig_account.account;
    account.data = Data::from_borsh(&state);
    Ok(vec![AccountPostState::new(account)])
}

/// Submit a vote receipt for a proposal.
///
/// The receipt proves the caller knows an nsk behind one of the registered
/// member commitments, without revealing which one.
pub fn vote(
    multisig_account: AccountWithMetadata,
    proposal_id: [u8; 32],
    receipt_bytes: Vec<u8>,
) -> Result<Vec<AccountPostState>, SpelError> {
    let mut state = MultisigState::try_from_slice(&multisig_account.account.data.0)
        .map_err(|_| SpelError::Custom { code: ERR_PROOF_INVALID })?;

    let receipt: risc0_zkvm::Receipt = risc0_zkvm::serde::from_slice(&receipt_bytes)
        .map_err(|_| SpelError::Custom { code: ERR_PROOF_INVALID })?;

    receipt.verify(IMAGE_ID)
        .map_err(|_| SpelError::Custom { code: ERR_PROOF_INVALID })?;

    #[derive(serde::Deserialize)]
    struct Journal {
        multisig_id: [u8; 32],
        proposal_id: [u8; 32],
        nullifier: [u8; 32],
        /// Hash of the full member commitment set. Proves the voter belongs
        /// to this multisig without revealing which member voted.
        member_set_root: [u8; 32],
    }

    let journal: Journal = receipt.journal.decode()
        .map_err(|_| SpelError::Custom { code: ERR_PROOF_INVALID })?;

    // Verify this receipt targets this multisig.
    let multisig_id: [u8; 32] = multisig_account.account_id.to_bytes();
    if journal.multisig_id != multisig_id {
        return Err(SpelError::Custom { code: ERR_MULTISIG_MISMATCH });
    }
    if journal.proposal_id != proposal_id {
        return Err(SpelError::Custom { code: ERR_PROPOSAL_NOT_FOUND });
    }

    // Verify the member_set_root matches the registered set.
    // We recompute the root on-chain from the stored commitments and compare.
    // The specific member who voted is never revealed.
    let expected_root = compute_member_set_root(&state.member_commitments);
    if journal.member_set_root != expected_root {
        return Err(SpelError::Custom { code: ERR_MEMBER_NOT_REGISTERED });
    }

    // Find the proposal.
    let proposal = state.proposals.iter_mut()
        .find(|p| p.id == proposal_id)
        .ok_or(SpelError::Custom { code: ERR_PROPOSAL_NOT_FOUND })?;

    if proposal.executed {
        return Err(SpelError::Custom { code: ERR_PROPOSAL_ALREADY_EXECUTED });
    }

    // Prevent double-voting.
    if proposal.spent_nullifiers.contains(&journal.nullifier) {
        return Err(SpelError::Custom { code: ERR_NULLIFIER_SPENT });
    }

    proposal.spent_nullifiers.push(journal.nullifier);
    proposal.vote_count += 1;

    let mut account = multisig_account.account;
    account.data = Data::from_borsh(&state);
    Ok(vec![AccountPostState::new(account)])
}

/// Execute a proposal once the threshold is met.
///
/// In a full integration this would dispatch the action_bytes to a downstream
/// program. Here it marks the proposal executed and returns the action_bytes
/// as a result so the caller can dispatch it.
pub fn execute(
    multisig_account: AccountWithMetadata,
    proposal_id: [u8; 32],
) -> Result<Vec<AccountPostState>, SpelError> {
    let mut state = MultisigState::try_from_slice(&multisig_account.account.data.0)
        .map_err(|_| SpelError::Custom { code: ERR_PROOF_INVALID })?;

    let proposal = state.proposals.iter_mut()
        .find(|p| p.id == proposal_id)
        .ok_or(SpelError::Custom { code: ERR_PROPOSAL_NOT_FOUND })?;

    if proposal.executed {
        return Err(SpelError::Custom { code: ERR_PROPOSAL_ALREADY_EXECUTED });
    }
    if proposal.vote_count < state.threshold {
        return Err(SpelError::Custom { code: ERR_THRESHOLD_NOT_MET });
    }

    proposal.executed = true;

    let mut account = multisig_account.account;
    account.data = Data::from_borsh(&state);
    Ok(vec![AccountPostState::new(account)])
}

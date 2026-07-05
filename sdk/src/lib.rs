//! Client SDK for LP-0002 private M-of-N multisig (LEZ v0.2.0).
//!
//! Gives Logos module builders everything needed to interact with the
//! on-chain program without depending on the CLI:
//!   - the instruction set and state types (re-exported from the program
//!     crate, so client and guest can never drift apart),
//!   - key/commitment/nullifier derivation identical to the in-guest code,
//!   - state decoding for `getAccount` reads.
//!
//! Transaction construction and submission use the LEZ `wallet` crate
//! (`WalletCore::send_privacy_preserving_tx` for `Vote`, public transactions
//! for the rest); see `cli/src/main.rs` for a complete reference client.

use anyhow::{anyhow, Result};
use borsh::BorshDeserialize;

pub use private_multisig_program::{
    apply_execute, apply_submit_proposal, apply_vote, compute_member_set_root, member_commitment,
    vote_nullifier, MultisigError, MultisigInstruction, MultisigState, Proposal, MAX_MEMBERS,
    MAX_PROPOSALS, VOTE_IDENTIFIER,
};

/// Derive the member commitment for a given nsk and multisig account ID.
/// Matches the guest exactly: SHA256("member" || nsk || multisig_id).
/// Share the commitment publicly; keep the nsk secret.
pub fn derive_commitment(nsk: &[u8; 32], multisig_id: &[u8; 32]) -> [u8; 32] {
    member_commitment(nsk, multisig_id)
}

/// Decode on-chain multisig account data (from `getAccount`) into state.
pub fn decode_state(account_data: &[u8]) -> Result<MultisigState> {
    MultisigState::try_from_slice(account_data)
        .map_err(|e| anyhow!("not a multisig account: {e}"))
}

/// Render state as human-readable text (what `multisig chain state` prints).
pub fn render_state(state: &MultisigState) -> String {
    let mut out = format!(
        "threshold: {}\nmembers: {}\nproposals: {}\n",
        state.threshold,
        state.member_commitments.len(),
        state.proposals.len()
    );
    for p in &state.proposals {
        out.push_str(&format!(
            "- proposal {}: votes={} executed={} action=0x{} nullifiers={}\n",
            hex::encode(p.id),
            p.vote_count,
            p.executed,
            hex::encode(&p.action_bytes),
            p.spent_nullifiers.len(),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_roundtrip() {
        let state = MultisigState {
            threshold: 2,
            member_commitments: vec![[1u8; 32], [2u8; 32]],
            proposals: vec![],
        };
        let bytes = borsh::to_vec(&state).unwrap();
        let decoded = decode_state(&bytes).unwrap();
        assert_eq!(decoded.threshold, 2);
        assert_eq!(decoded.member_commitments.len(), 2);
    }
}

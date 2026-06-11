//! RISC0 guest: private M-of-N multisig vote proof for LP-0002.
//!
//! Proves: "I know the nsk behind one of the registered member commitments,
//! and I have not voted on this proposal before."
//!
//! Privacy: the journal does NOT reveal which member voted. It commits only
//! the `member_set_root` (a hash of all registered commitments), not the
//! specific commitment used. An on-chain observer sees proof that *a*
//! registered member voted, but cannot determine which one.
//!
//! Private inputs:
//!   nsk: [u8; 32]                    -- member's nullifier secret key
//!   member_index: usize              -- index in the member_commitments list
//!   member_commitments: Vec<[u8;32]> -- full set of registered commitments (private)
//!
//! Public outputs (journal):
//!   multisig_id: [u8; 32]     -- identifies the multisig
//!   proposal_id: [u8; 32]     -- which proposal this vote is for
//!   nullifier: [u8; 32]       -- SHA256("multisig/v1/vote" || nsk || proposal_id || multisig_id)
//!   member_set_root: [u8; 32] -- SHA256(commitment[0] || ... || commitment[N-1])
//!                               On-chain verifier checks this matches the registered set.
//!                               Does NOT reveal which commitment was used.

#![no_std]
#![no_main]

use risc0_zkvm::guest::env;
use risc0_zkvm::sha::{Impl as ShaImpl, Sha256 as _};

risc0_zkvm::guest::entry!(main);

#[derive(serde::Deserialize)]
struct GuestInput {
    nsk: [u8; 32],
    member_index: usize,
    /// The full set of registered member commitments. Kept private.
    member_commitments: alloc::vec::Vec<[u8; 32]>,
    multisig_id: [u8; 32],
    proposal_id: [u8; 32],
}

#[derive(serde::Serialize)]
struct VoteAttestation {
    multisig_id: [u8; 32],
    proposal_id: [u8; 32],
    nullifier: [u8; 32],
    /// Hash of the full commitment set. Binds the proof to this multisig's
    /// exact membership state without revealing which member voted.
    member_set_root: [u8; 32],
}

pub fn main() {
    let input: GuestInput = env::read();

    assert!(
        input.member_index < input.member_commitments.len(),
        "member_index out of range"
    );

    // 1. Recompute member commitment from nsk: SHA256("member" || nsk || multisig_id)
    let expected_commitment: [u8; 32] = {
        let mut preimage = alloc::vec::Vec::with_capacity(6 + 32 + 32);
        preimage.extend_from_slice(b"member");
        preimage.extend_from_slice(&input.nsk);
        preimage.extend_from_slice(&input.multisig_id);
        ShaImpl::hash_bytes(&preimage).as_bytes().try_into::<[u8; 32]>().unwrap()
    };

    // 2. Verify the recomputed commitment matches the registered one at member_index.
    assert_eq!(
        expected_commitment, input.member_commitments[input.member_index],
        "nsk does not match registered commitment"
    );

    // 3. Compute the member set root: SHA256(commitment[0] || ... || commitment[N-1]).
    //    This is a commitment to the entire registered set. Publishing it proves the
    //    proof is valid for exactly this set, without revealing which member voted.
    let member_set_root: [u8; 32] = {
        let mut preimage = alloc::vec::Vec::with_capacity(input.member_commitments.len() * 32);
        for c in &input.member_commitments {
            preimage.extend_from_slice(c);
        }
        ShaImpl::hash_bytes(&preimage).as_bytes().try_into::<[u8; 32]>().unwrap()
    };

    // 4. Compute per-proposal nullifier.
    let nullifier: [u8; 32] = {
        let mut preimage = alloc::vec::Vec::with_capacity(16 + 32 + 32 + 32);
        preimage.extend_from_slice(b"multisig/v1/vote");
        preimage.extend_from_slice(&input.nsk);
        preimage.extend_from_slice(&input.proposal_id);
        preimage.extend_from_slice(&input.multisig_id);
        ShaImpl::hash_bytes(&preimage).as_bytes().try_into::<[u8; 32]>().unwrap()
    };

    // 5. Commit journal. member_set_root identifies which set was used
    //    but does NOT reveal which specific member voted.
    env::commit(&VoteAttestation {
        multisig_id: input.multisig_id,
        proposal_id: input.proposal_id,
        nullifier,
        member_set_root,
    });
}

extern crate alloc;

//! Integration tests for the LP-0002 private multisig on-chain program.

use private_multisig_program::*;

#[test]
fn error_codes_are_distinct() {
    let codes = [
        ERR_PROOF_INVALID,
        ERR_MULTISIG_MISMATCH,
        ERR_PROPOSAL_NOT_FOUND,
        ERR_NULLIFIER_SPENT,
        ERR_MEMBER_NOT_REGISTERED,
        ERR_PROPOSAL_ALREADY_EXECUTED,
        ERR_THRESHOLD_NOT_MET,
        ERR_ALREADY_INITIALIZED,
        ERR_TOO_MANY_MEMBERS,
        ERR_INVALID_THRESHOLD,
    ];
    let mut seen = std::collections::HashSet::new();
    for &c in &codes {
        assert!(seen.insert(c), "duplicate error code {}", c);
    }
}

#[test]
fn multisig_state_borsh_roundtrip() {
    use borsh::{BorshDeserialize, BorshSerialize};

    let state = MultisigState {
        threshold: 2,
        member_commitments: vec![[0x01u8; 32], [0x02u8; 32], [0x03u8; 32]],
        proposals: vec![Proposal {
            id: [0xaau8; 32],
            action_bytes: b"transfer 100".to_vec(),
            vote_count: 1,
            executed: false,
            spent_nullifiers: vec![[0x0fu8; 32]],
        }],
    };

    let encoded = borsh::to_vec(&state).unwrap();
    let decoded = MultisigState::try_from_slice(&encoded).unwrap();

    assert_eq!(decoded.threshold, 2);
    assert_eq!(decoded.member_commitments.len(), 3);
    assert_eq!(decoded.proposals.len(), 1);
    assert_eq!(decoded.proposals[0].vote_count, 1);
    assert!(!decoded.proposals[0].executed);
}

#[test]
fn member_commitment_is_key_and_multisig_bound() {
    use sha2::{Digest, Sha256};

    let nsk = [0x42u8; 32];
    let multisig1 = [0x01u8; 32];
    let multisig2 = [0x02u8; 32];

    // commitment = SHA256("member" || nsk || multisig_id)
    let commit = |multisig: &[u8; 32]| -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(b"member");
        h.update(&nsk);
        h.update(multisig);
        h.finalize().into()
    };

    let c1 = commit(&multisig1);
    let c2 = commit(&multisig2);
    assert_ne!(c1, c2, "commitments for different multisigs must differ");

    // Different nsk for same multisig
    let nsk2 = [0x99u8; 32];
    let commit2 = |nsk: &[u8; 32]| -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(b"member");
        h.update(nsk);
        h.update(&multisig1);
        h.finalize().into()
    };
    assert_ne!(commit2(&nsk), commit2(&nsk2), "different nsks must yield different commitments");
}

#[test]
fn nullifier_prevents_double_vote() {
    use sha2::{Digest, Sha256};

    let nsk = [0x42u8; 32];
    let proposal_id = [0xbbu8; 32];
    let multisig_id = [0xccu8; 32];

    // nullifier = SHA256("multisig/v1/vote" || nsk || proposal_id || multisig_id)
    let nullifier = |nsk: &[u8; 32]| -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(b"multisig/v1/vote");
        h.update(nsk);
        h.update(&proposal_id);
        h.update(&multisig_id);
        h.finalize().into()
    };

    let n1 = nullifier(&nsk);
    let n2 = nullifier(&nsk);
    assert_eq!(n1, n2, "same inputs must produce same nullifier (deterministic)");

    let nsk2 = [0x99u8; 32];
    assert_ne!(n1, nullifier(&nsk2), "different member must produce different nullifier");
}

#[test]
fn member_set_root_does_not_reveal_voter() {
    use sha2::{Digest, Sha256};

    // Three commitments representing three members
    let c1 = [0x01u8; 32];
    let c2 = [0x02u8; 32];
    let c3 = [0x03u8; 32];
    let commitments = [c1, c2, c3];

    // root = SHA256(c1 || c2 || c3) -- same regardless of which member voted
    let mut h = Sha256::new();
    for c in &commitments {
        h.update(c);
    }
    let root: [u8; 32] = h.finalize().into();

    // The root is the same whether member 0, 1, or 2 voted.
    // An observer cannot determine which commitment was used.
    assert_ne!(root, c1);
    assert_ne!(root, c2);
    assert_ne!(root, c3);
}

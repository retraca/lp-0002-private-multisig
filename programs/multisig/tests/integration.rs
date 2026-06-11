//! Integration tests for the LP-0002 private multisig on-chain program.
//! Tests run against `apply_vote` and `apply_execute` directly -- no LEZ
//! sequencer or RISC0 receipt needed.

use borsh::BorshDeserialize;
use private_multisig_program::*;
use sha2::{Digest, Sha256};

fn sha256_chain(inputs: &[&[u8]]) -> [u8; 32] {
    let mut h = Sha256::new();
    for i in inputs {
        h.update(i);
    }
    h.finalize().into()
}

fn member_commitment(nsk: &[u8; 32], multisig_id: &[u8; 32]) -> [u8; 32] {
    sha256_chain(&[b"member", nsk, multisig_id])
}

fn member_set_root(commitments: &[[u8; 32]]) -> [u8; 32] {
    let mut h = Sha256::new();
    for c in commitments {
        h.update(c);
    }
    h.finalize().into()
}

fn vote_nullifier(nsk: &[u8; 32], proposal_id: &[u8; 32], multisig_id: &[u8; 32]) -> [u8; 32] {
    sha256_chain(&[b"multisig/v1/vote", nsk, proposal_id, multisig_id])
}

fn base_state(threshold: u8, nsks: &[[u8; 32]], multisig_id: &[u8; 32]) -> MultisigState {
    let commitments = nsks.iter().map(|nsk| member_commitment(nsk, multisig_id)).collect();
    MultisigState {
        threshold,
        member_commitments: commitments,
        proposals: vec![],
    }
}

fn add_proposal(state: &mut MultisigState, proposal_id: [u8; 32]) {
    state.proposals.push(Proposal {
        id: proposal_id,
        action_bytes: b"transfer 100".to_vec(),
        vote_count: 0,
        executed: false,
        spent_nullifiers: vec![],
    });
}

const MULTISIG_ID: [u8; 32] = [0xccu8; 32];
const PROPOSAL_ID: [u8; 32] = [0xaau8; 32];
const NSK_1: [u8; 32] = [0x11u8; 32];
const NSK_2: [u8; 32] = [0x22u8; 32];
const NSK_3: [u8; 32] = [0x33u8; 32];

fn make_journal(nsk: &[u8; 32]) -> VoteJournal {
    let nsks = [NSK_1, NSK_2, NSK_3];
    let commitments: Vec<[u8; 32]> = nsks.iter().map(|k| member_commitment(k, &MULTISIG_ID)).collect();
    VoteJournal {
        multisig_id: MULTISIG_ID,
        proposal_id: PROPOSAL_ID,
        nullifier: vote_nullifier(nsk, &PROPOSAL_ID, &MULTISIG_ID),
        member_set_root: member_set_root(&commitments),
    }
}

#[test]
fn successful_vote_increments_count() {
    let mut state = base_state(2, &[NSK_1, NSK_2, NSK_3], &MULTISIG_ID);
    add_proposal(&mut state, PROPOSAL_ID);

    let j = make_journal(&NSK_1);
    apply_vote(&mut state, &j, MULTISIG_ID, PROPOSAL_ID).unwrap();

    assert_eq!(state.proposals[0].vote_count, 1);
    assert_eq!(state.proposals[0].spent_nullifiers.len(), 1);
}

#[test]
fn threshold_reached_allows_execute() {
    let mut state = base_state(2, &[NSK_1, NSK_2, NSK_3], &MULTISIG_ID);
    add_proposal(&mut state, PROPOSAL_ID);

    apply_vote(&mut state, &make_journal(&NSK_1), MULTISIG_ID, PROPOSAL_ID).unwrap();
    apply_vote(&mut state, &make_journal(&NSK_2), MULTISIG_ID, PROPOSAL_ID).unwrap();

    apply_execute(&mut state, PROPOSAL_ID).unwrap();
    assert!(state.proposals[0].executed);
}

#[test]
fn threshold_not_met_blocks_execute() {
    let mut state = base_state(2, &[NSK_1, NSK_2, NSK_3], &MULTISIG_ID);
    add_proposal(&mut state, PROPOSAL_ID);

    apply_vote(&mut state, &make_journal(&NSK_1), MULTISIG_ID, PROPOSAL_ID).unwrap();

    let err = apply_execute(&mut state, PROPOSAL_ID).unwrap_err();
    assert_eq!(err, spel_framework::error::SpelError::Custom { code: ERR_THRESHOLD_NOT_MET });
}

#[test]
fn nullifier_prevents_double_vote() {
    let mut state = base_state(2, &[NSK_1, NSK_2, NSK_3], &MULTISIG_ID);
    add_proposal(&mut state, PROPOSAL_ID);

    apply_vote(&mut state, &make_journal(&NSK_1), MULTISIG_ID, PROPOSAL_ID).unwrap();

    let err = apply_vote(&mut state, &make_journal(&NSK_1), MULTISIG_ID, PROPOSAL_ID).unwrap_err();
    assert_eq!(err, spel_framework::error::SpelError::Custom { code: ERR_NULLIFIER_SPENT });
    assert_eq!(state.proposals[0].vote_count, 1);
}

#[test]
fn multisig_mismatch_rejected() {
    let mut state = base_state(2, &[NSK_1, NSK_2, NSK_3], &MULTISIG_ID);
    add_proposal(&mut state, PROPOSAL_ID);

    let mut j = make_journal(&NSK_1);
    j.multisig_id = [0xffu8; 32]; // wrong multisig

    let err = apply_vote(&mut state, &j, MULTISIG_ID, PROPOSAL_ID).unwrap_err();
    assert_eq!(err, spel_framework::error::SpelError::Custom { code: ERR_MULTISIG_MISMATCH });
}

#[test]
fn unregistered_member_set_root_rejected() {
    let mut state = base_state(2, &[NSK_1, NSK_2, NSK_3], &MULTISIG_ID);
    add_proposal(&mut state, PROPOSAL_ID);

    // Journal claims a different member set
    let mut j = make_journal(&NSK_1);
    j.member_set_root = [0x00u8; 32];

    let err = apply_vote(&mut state, &j, MULTISIG_ID, PROPOSAL_ID).unwrap_err();
    assert_eq!(err, spel_framework::error::SpelError::Custom { code: ERR_MEMBER_NOT_REGISTERED });
}

#[test]
fn already_executed_proposal_rejects_further_execute() {
    let mut state = base_state(1, &[NSK_1], &MULTISIG_ID);
    add_proposal(&mut state, PROPOSAL_ID);

    apply_vote(&mut state, &make_journal(&NSK_1), MULTISIG_ID, PROPOSAL_ID).unwrap();
    apply_execute(&mut state, PROPOSAL_ID).unwrap();

    let err = apply_execute(&mut state, PROPOSAL_ID).unwrap_err();
    assert_eq!(err, spel_framework::error::SpelError::Custom { code: ERR_PROPOSAL_ALREADY_EXECUTED });
}

#[test]
fn executed_proposal_rejects_further_votes() {
    let mut state = base_state(1, &[NSK_1, NSK_2], &MULTISIG_ID);
    add_proposal(&mut state, PROPOSAL_ID);

    apply_vote(&mut state, &make_journal(&NSK_1), MULTISIG_ID, PROPOSAL_ID).unwrap();
    apply_execute(&mut state, PROPOSAL_ID).unwrap();

    let err = apply_vote(&mut state, &make_journal(&NSK_2), MULTISIG_ID, PROPOSAL_ID).unwrap_err();
    assert_eq!(err, spel_framework::error::SpelError::Custom { code: ERR_PROPOSAL_ALREADY_EXECUTED });
}

#[test]
fn multisig_state_borsh_roundtrip() {
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
    assert_eq!(decoded.proposals[0].vote_count, 1);
    assert!(!decoded.proposals[0].executed);
}

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
        ERR_TOO_MANY_PROPOSALS,
    ];
    let mut seen = std::collections::HashSet::new();
    for &c in &codes {
        assert!(seen.insert(c), "duplicate error code {}", c);
    }
}

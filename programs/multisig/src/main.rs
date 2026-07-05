//! Guest binary for the LP-0002 private multisig program (LEZ v0.2.0).
//!
//! Build: cargo +risc0 build --release --target riscv32im-risc0-zkvm-elf -p private-multisig-program
//! Deploy: `multisig chain deploy --program-bin <elf>` (or the stock wallet).
//!
//! Failure model: every invalid input aborts via panic whose message carries a
//! documented `ERR_<code>` (see lib.rs). For public transactions the sequencer
//! rejects the transaction; for the privacy-preserving `Vote` the proof simply
//! cannot be produced, so an invalid vote never even reaches the chain.

use borsh::BorshDeserialize;
use lee_core::account::{Account, AccountId, AccountWithMetadata};
use lee_core::program::{read_lee_inputs, AccountPostState, Claim, ProgramInput, ProgramOutput};
use lee_core::NullifierPublicKey;
use private_multisig_program::{
    apply_execute, apply_submit_proposal, apply_vote, MultisigInstruction, MultisigState,
    ERR_ALREADY_INITIALIZED, ERR_PROOF_INVALID, ERR_RIDER_MISMATCH, ERR_RIDER_NOT_LIVE,
    VOTE_IDENTIFIER,
};

fn state_of(account: &AccountWithMetadata) -> MultisigState {
    MultisigState::try_from_slice(account.account.data.as_ref())
        .unwrap_or_else(|_| panic!("ERR_{ERR_PROOF_INVALID} multisig state deserialize failed"))
}

fn with_state(mut account: Account, state: &MultisigState) -> Account {
    account.data = borsh::to_vec(state)
        .expect("state serialize")
        .try_into()
        .expect("multisig state fits into account data limit");
    account
}

fn main() {
    let (
        ProgramInput {
            self_program_id,
            caller_program_id,
            pre_states,
            instruction,
        },
        instruction_words,
    ) = read_lee_inputs::<MultisigInstruction>();

    let post_states = match instruction {
        // Claim the (signed) multisig account and register the member set.
        MultisigInstruction::Initialize {
            threshold,
            commitments,
        } => {
            let Ok([multisig]) = <[_; 1]>::try_from(pre_states.clone()) else {
                return;
            };
            assert!(
                multisig.account == Account::default(),
                "ERR_{ERR_ALREADY_INITIALIZED} multisig account already initialized"
            );
            MultisigState::validate_new(threshold, &commitments)
                .unwrap_or_else(|e| panic!("{e}"));
            let state = MultisigState {
                threshold,
                member_commitments: commitments,
                proposals: Vec::new(),
            };
            vec![AccountPostState::new_claimed(
                with_state(multisig.account, &state),
                Claim::Authorized,
            )]
        }

        // Record a proposal on the program-owned account (unsigned).
        MultisigInstruction::SubmitProposal {
            proposal_id,
            action,
        } => {
            let Ok([multisig]) = <[_; 1]>::try_from(pre_states.clone()) else {
                return;
            };
            let mut state = state_of(&multisig);
            apply_submit_proposal(&mut state, proposal_id, action)
                .unwrap_or_else(|e| panic!("{e}"));
            vec![AccountPostState::new(with_state(multisig.account, &state))]
        }

        // Anonymous approval: privacy-preserving initial call, nsk private.
        MultisigInstruction::Vote {
            nsk,
            member_index,
            proposal_id,
        } => {
            let Ok([multisig, rider]) = <[_; 2]>::try_from(pre_states.clone()) else {
                return;
            };
            let multisig_id: [u8; 32] = multisig
                .account_id
                .as_ref()
                .try_into()
                .expect("account id is 32 bytes");

            // In-circuit live-account binding: the rider must BE the member's
            // shielded voting account derived from the SAME nsk, and it must
            // already exist on chain (the LEZ privacy circuit proves its
            // pre-state commitment is in the live commitment tree).
            let expected_rider = AccountId::for_regular_private_account(
                &NullifierPublicKey::from(&nsk),
                VOTE_IDENTIFIER,
            );
            assert!(
                rider.account_id == expected_rider,
                "ERR_{ERR_RIDER_MISMATCH} rider is not the voting account derived from this nsk"
            );
            assert!(
                rider.account != Account::default(),
                "ERR_{ERR_RIDER_NOT_LIVE} rider must be a live shielded account"
            );

            let mut state = state_of(&multisig);
            apply_vote(&mut state, &nsk, member_index, proposal_id, multisig_id)
                .unwrap_or_else(|e| panic!("{e}"));

            // The rider passes through unchanged; the privacy circuit rotates
            // its commitment + nonce like any private transfer.
            vec![
                AccountPostState::new(with_state(multisig.account, &state)),
                AccountPostState::new(rider.account),
            ]
        }

        // Threshold-gated execution (unsigned).
        MultisigInstruction::Execute { proposal_id } => {
            let Ok([multisig]) = <[_; 1]>::try_from(pre_states.clone()) else {
                return;
            };
            let mut state = state_of(&multisig);
            apply_execute(&mut state, proposal_id).unwrap_or_else(|e| panic!("{e}"));
            vec![AccountPostState::new(with_state(multisig.account, &state))]
        }
    };

    ProgramOutput::new(
        self_program_id,
        caller_program_id,
        instruction_words,
        pre_states,
        post_states,
    )
    .write();
}

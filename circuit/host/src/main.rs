mod methods;
mod prover;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use hex::FromHex;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

use methods::PRIVATE_MULTISIG_GUEST_ID;
use prover::{prove, ProverInput};

#[derive(Parser)]
#[command(name = "multisig", about = "LP-0002 private M-of-N multisig CLI")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Derive the member commitment for a given nsk and multisig account ID.
    /// Share this value with the multisig creator; keep nsk secret.
    DeriveCommitment {
        #[arg(long)]
        nsk: String,
        #[arg(long)]
        multisig_id: String,
    },
    /// Generate a vote proof for a proposal.
    Vote {
        #[arg(long)]
        nsk: String,
        #[arg(long)]
        member_index: usize,
        #[arg(long)]
        multisig_id: String,
        #[arg(long)]
        proposal_id: String,
        /// All member commitments (comma-separated hex). Fetched from chain in production.
        #[arg(long)]
        member_commitments: String,
        #[arg(long, default_value = "vote-receipt.bin")]
        out: PathBuf,
    },
    /// Verify a vote receipt offline.
    Verify {
        #[arg(long)]
        receipt: PathBuf,
        #[arg(long)]
        multisig_id: String,
        #[arg(long)]
        proposal_id: String,
    },
    /// On-chain operations (requires --features chain and lez-build at ../../../lez-build/).
    #[command(subcommand)]
    Chain(ChainCmd),
}

#[derive(Subcommand)]
enum ChainCmd {
    /// Generate a fresh account signing key and print the derived account ID.
    /// Use the account ID as the multisig ID and pass the key to `initialize`.
    Keygen,
    /// Read the multisig account state from the chain.
    State {
        #[arg(long, default_value = "http://127.0.0.1:3040")]
        sequencer: String,
        #[arg(long)]
        multisig_id: String,
    },
    /// Initialize a new multisig on-chain.
    ///
    /// Account claiming requires authorization: the transaction must be signed
    /// with the multisig account's key (generate one with `chain keygen`).
    /// The multisig account ID is derived from the signing key.
    Initialize {
        #[arg(long, default_value = "http://127.0.0.1:3040")]
        sequencer: String,
        /// Program ID (64-char hex of 8 x u32 LE, from wallet deploy-program output).
        #[arg(long)]
        program_id: String,
        /// Account signing key (64-char hex, from `chain keygen`).
        #[arg(long)]
        signing_key: String,
        /// Approval threshold (number of votes required).
        #[arg(long)]
        threshold: u8,
        /// Member commitments (comma-separated hex, derive with `multisig derive-commitment`).
        #[arg(long)]
        commitments: String,
    },
    /// Submit a new proposal.
    SubmitProposal {
        #[arg(long, default_value = "http://127.0.0.1:3040")]
        sequencer: String,
        #[arg(long)]
        program_id: String,
        #[arg(long)]
        multisig_id: String,
        /// Proposal ID (64-char hex, choose any 32-byte value).
        #[arg(long)]
        proposal_id: String,
        /// Opaque action bytes (hex).
        #[arg(long, default_value = "")]
        action: String,
    },
    /// Submit a vote receipt on-chain.
    SubmitVote {
        #[arg(long, default_value = "http://127.0.0.1:3040")]
        sequencer: String,
        #[arg(long)]
        program_id: String,
        #[arg(long)]
        multisig_id: String,
        #[arg(long)]
        proposal_id: String,
        /// Path to vote receipt (output of `multisig vote --out`).
        #[arg(long)]
        receipt: PathBuf,
    },
    /// Execute a proposal once the threshold is met.
    Execute {
        #[arg(long, default_value = "http://127.0.0.1:3040")]
        sequencer: String,
        #[arg(long)]
        program_id: String,
        #[arg(long)]
        multisig_id: String,
        #[arg(long)]
        proposal_id: String,
    },
}

fn parse_hex32(s: &str) -> Result<[u8; 32]> {
    let bytes = Vec::from_hex(s).context("invalid hex")?;
    <[u8; 32]>::try_from(bytes.as_slice())
        .map_err(|_| anyhow::anyhow!("expected 32 bytes, got {}", bytes.len()))
}

#[cfg_attr(not(feature = "chain"), allow(dead_code))]
fn parse_program_id(hex: &str) -> Result<[u32; 8]> {
    let bytes = Vec::from_hex(hex).context("invalid program_id hex")?;
    anyhow::ensure!(bytes.len() == 32, "program_id must be 64 hex chars (32 bytes)");
    let mut pid = [0u32; 8];
    for (i, chunk) in bytes.chunks(4).enumerate() {
        pid[i] = u32::from_le_bytes(chunk.try_into().unwrap());
    }
    Ok(pid)
}

fn derive_commitment(nsk: &[u8; 32], multisig_id: &[u8; 32]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"member");
    h.update(nsk);
    h.update(multisig_id);
    h.finalize().into()
}

// ---------------------------------------------------------------------------
// risc0 serde instruction encoding
//
// The SPEL #[lez_program] macro generates an Instruction enum and serializes
// it with serialize_tuple_variant(variant_index, ...). In risc0 serde format:
//   - Each u8 is one u32 word (zero-extended).
//   - [u8; N]: N words (one per byte).
//   - Vec<u8>: 1 length word + N byte words.
//   - Vec<[u8; N]>: 1 length word + len * N byte words.
//   - Enum variant: 1 word (variant index) + fields in order.
//
// Variant order follows declaration order in #[lez_program] mod:
//   0 = Initialize, 1 = SubmitProposal, 2 = Vote, 3 = Execute
// ---------------------------------------------------------------------------

#[cfg_attr(not(feature = "chain"), allow(dead_code))]
fn encode_bytes32(b: &[u8; 32], out: &mut Vec<u32>) {
    for &byte in b {
        out.push(byte as u32);
    }
}

#[cfg_attr(not(feature = "chain"), allow(dead_code))]
fn encode_vec_bytes32(items: &[[u8; 32]], out: &mut Vec<u32>) {
    out.push(items.len() as u32);
    for item in items {
        encode_bytes32(item, out);
    }
}

#[cfg_attr(not(feature = "chain"), allow(dead_code))]
fn encode_vec_u8(bytes: &[u8], out: &mut Vec<u32>) {
    out.push(bytes.len() as u32);
    for &b in bytes {
        out.push(b as u32);
    }
}

#[cfg_attr(not(feature = "chain"), allow(dead_code))]
fn instr_initialize(threshold: u8, commitments: &[[u8; 32]]) -> Vec<u32> {
    let mut w = vec![0u32]; // variant 0
    w.push(threshold as u32);
    encode_vec_bytes32(commitments, &mut w);
    w
}

#[cfg_attr(not(feature = "chain"), allow(dead_code))]
fn instr_submit_proposal(proposal_id: &[u8; 32], action: &[u8]) -> Vec<u32> {
    let mut w = vec![1u32]; // variant 1
    encode_bytes32(proposal_id, &mut w);
    encode_vec_u8(action, &mut w);
    w
}

#[cfg_attr(not(feature = "chain"), allow(dead_code))]
fn instr_vote(proposal_id: &[u8; 32], journal_bytes: &[u8]) -> Vec<u32> {
    let mut w = vec![2u32]; // variant 2
    encode_bytes32(proposal_id, &mut w);
    encode_vec_u8(journal_bytes, &mut w);
    w
}

#[cfg_attr(not(feature = "chain"), allow(dead_code))]
fn instr_execute(proposal_id: &[u8; 32]) -> Vec<u32> {
    let mut w = vec![3u32]; // variant 3
    encode_bytes32(proposal_id, &mut w);
    w
}

#[cfg(feature = "chain")]
mod chain {
    use super::*;
    use anyhow::Result;
    use common::transaction::NSSATransaction;
    use nssa::{
        AccountId, PrivateKey, PublicKey,
        public_transaction::{Message, WitnessSet},
        PublicTransaction,
    };
    use sequencer_service_rpc::{RpcClient as _, SequencerClientBuilder};

    pub fn account_id_for_key(key: &PrivateKey) -> AccountId {
        AccountId::from(&PublicKey::new_from_private_key(key))
    }

    pub fn keygen() -> (PrivateKey, AccountId) {
        let key = PrivateKey::new_os_random();
        let account_id = account_id_for_key(&key);
        (key, account_id)
    }

    /// Unsigned call: for instructions on an account already owned by the
    /// program (`#[account(mut)]`), no signature is needed.
    pub async fn send_call(
        sequencer: &str,
        program_id: [u32; 8],
        multisig_id: [u8; 32],
        instruction_data: Vec<u32>,
    ) -> Result<String> {
        let client = SequencerClientBuilder::default()
            .build(sequencer)
            .context("build sequencer client")?;

        let account_id: AccountId = AccountId::new(multisig_id);

        let message = Message::new_preserialized(
            program_id,
            vec![account_id],
            vec![], // no signers → no nonces
            instruction_data,
        );
        let witness_set = WitnessSet::for_message(&message, &[]);
        let tx = PublicTransaction::new(message, witness_set);

        let hash = client
            .send_transaction(NSSATransaction::Public(tx))
            .await
            .context("send_transaction failed")?;

        Ok(hex::encode(hash))
    }

    /// Signed call: account claiming (`#[account(init)]`) requires the
    /// transaction to be authorized by the account's key. The account ID is
    /// derived from the key; the current nonce is fetched from the chain.
    pub async fn send_signed_call(
        sequencer: &str,
        program_id: [u32; 8],
        key: &PrivateKey,
        instruction_data: Vec<u32>,
    ) -> Result<(String, AccountId)> {
        let client = SequencerClientBuilder::default()
            .build(sequencer)
            .context("build sequencer client")?;

        let account_id = account_id_for_key(key);
        let nonces = client
            .get_accounts_nonces(vec![account_id])
            .await
            .context("get_accounts_nonces failed")?;

        let message = Message::new_preserialized(
            program_id,
            vec![account_id],
            nonces,
            instruction_data,
        );
        let witness_set = WitnessSet::for_message(&message, &[key]);
        let tx = PublicTransaction::new(message, witness_set);

        let hash = client
            .send_transaction(NSSATransaction::Public(tx))
            .await
            .context("send_transaction failed")?;

        Ok((hex::encode(hash), account_id))
    }

    /// Read raw account state.
    pub async fn get_account_state(
        sequencer: &str,
        account_id_bytes: [u8; 32],
    ) -> Result<(Vec<u8>, [u32; 8])> {
        let client = SequencerClientBuilder::default()
            .build(sequencer)
            .context("build sequencer client")?;
        let account = client
            .get_account(AccountId::new(account_id_bytes))
            .await
            .context("get_account failed")?;
        Ok((account.data.as_ref().to_vec(), account.program_owner))
    }
}

#[cfg(not(feature = "chain"))]
fn chain_not_available() -> ! {
    eprintln!("Chain commands require --features chain.");
    eprintln!("Rebuild: cargo build --release --features chain");
    eprintln!("(requires lez-build workspace at ../../../lez-build/)");
    std::process::exit(1);
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.cmd {
        Cmd::DeriveCommitment { nsk, multisig_id } => {
            let nsk_bytes = parse_hex32(&nsk)?;
            let multisig_bytes = parse_hex32(&multisig_id)?;
            let commitment = derive_commitment(&nsk_bytes, &multisig_bytes);
            println!("{}", hex::encode(commitment));
        }

        Cmd::Vote { nsk, member_index, multisig_id, proposal_id, member_commitments, out } => {
            let nsk_bytes = parse_hex32(&nsk)?;
            let multisig_bytes = parse_hex32(&multisig_id)?;
            let proposal_bytes = parse_hex32(&proposal_id)?;

            let parsed: Result<Vec<[u8; 32]>> = member_commitments
                .split(',')
                .map(|s| parse_hex32(s.trim()))
                .collect();
            let parsed = parsed?;

            let our_commitment = derive_commitment(&nsk_bytes, &multisig_bytes);
            anyhow::ensure!(
                member_index < parsed.len() && parsed[member_index] == our_commitment,
                "nsk does not match commitment at index {}",
                member_index
            );

            eprintln!("Running RISC0 prover...");
            let receipt = prove(ProverInput {
                nsk: nsk_bytes,
                member_index,
                member_commitments: parsed,
                multisig_id: multisig_bytes,
                proposal_id: proposal_bytes,
            })?;

            let words = risc0_zkvm::serde::to_vec(&receipt)
                .map_err(|e| anyhow::anyhow!("serialise: {e}"))?;
            let bytes: Vec<u8> = bytemuck::cast_slice(&words).to_vec();
            std::fs::write(&out, &bytes)?;
            eprintln!("Receipt written to {}", out.display());

            #[derive(serde::Deserialize)]
            struct Journal {
                multisig_id: [u8; 32],
                proposal_id: [u8; 32],
                nullifier: [u8; 32],
                member_set_root: [u8; 32],
            }
            let j: Journal = receipt.journal.decode()?;
            println!("multisig_id:     {}", hex::encode(j.multisig_id));
            println!("proposal_id:     {}", hex::encode(j.proposal_id));
            println!("nullifier:       {}", hex::encode(j.nullifier));
            println!("member_set_root: {}", hex::encode(j.member_set_root));
        }

        Cmd::Verify { receipt, multisig_id, proposal_id } => {
            let multisig_bytes = parse_hex32(&multisig_id)?;
            let proposal_bytes = parse_hex32(&proposal_id)?;

            let raw = std::fs::read(&receipt)?;
            let words: Vec<u32> = raw.chunks_exact(4)
                .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
                .collect();
            let r: risc0_zkvm::Receipt = risc0_zkvm::serde::from_slice(&words)
                .map_err(|e| anyhow::anyhow!("deserialise: {e}"))?;

            r.verify(PRIVATE_MULTISIG_GUEST_ID).context("receipt verification failed")?;

            #[derive(serde::Deserialize)]
            struct Journal {
                multisig_id: [u8; 32],
                proposal_id: [u8; 32],
                nullifier: [u8; 32],
                member_set_root: [u8; 32],
            }
            let j: Journal = r.journal.decode()?;
            anyhow::ensure!(j.multisig_id == multisig_bytes, "multisig_id mismatch");
            anyhow::ensure!(j.proposal_id == proposal_bytes, "proposal_id mismatch");

            println!("OK");
            println!("nullifier:       {}", hex::encode(j.nullifier));
            println!("member_set_root: {}", hex::encode(j.member_set_root));
        }

        Cmd::Chain(chain_cmd) => {
            #[cfg(not(feature = "chain"))]
            chain_not_available();

            #[cfg(feature = "chain")]
            match chain_cmd {
                ChainCmd::Keygen => {
                    let (key, account_id) = chain::keygen();
                    println!("signing_key: {key}");
                    println!("multisig_id: {}", hex::encode(account_id.value()));
                    println!("account_id (base58): {account_id}");
                }
                ChainCmd::State { sequencer, multisig_id } => {
                    let mid = parse_hex32(&multisig_id)?;
                    let (data, program_owner) = chain::get_account_state(&sequencer, mid).await?;
                    let owner_hex = program_owner
                        .iter()
                        .flat_map(|w| w.to_le_bytes())
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>();
                    println!("program_owner: {owner_hex}");
                    println!("data ({} bytes): {}", data.len(), hex::encode(&data));
                }
                ChainCmd::Initialize { sequencer, program_id, signing_key, threshold, commitments } => {
                    let pid = parse_program_id(&program_id)?;
                    let key: nssa::PrivateKey = signing_key.parse()
                        .map_err(|e| anyhow::anyhow!("invalid signing key: {e:?}"))?;
                    let parsed: Result<Vec<[u8; 32]>> =
                        commitments.split(',').map(|s| parse_hex32(s.trim())).collect();
                    let parsed = parsed?;
                    let instr = instr_initialize(threshold, &parsed);
                    let (hash, account_id) =
                        chain::send_signed_call(&sequencer, pid, &key, instr).await?;
                    println!("tx: {hash}");
                    println!("multisig_id: {}", hex::encode(account_id.value()));
                }
                ChainCmd::SubmitProposal { sequencer, program_id, multisig_id, proposal_id, action } => {
                    let pid = parse_program_id(&program_id)?;
                    let mid = parse_hex32(&multisig_id)?;
                    let prop = parse_hex32(&proposal_id)?;
                    let action_bytes = if action.is_empty() {
                        vec![]
                    } else {
                        Vec::from_hex(&action).context("action must be hex")?
                    };
                    let instr = instr_submit_proposal(&prop, &action_bytes);
                    let hash = chain::send_call(&sequencer, pid, mid, instr).await?;
                    println!("tx: {hash}");
                }
                ChainCmd::SubmitVote { sequencer, program_id, multisig_id, proposal_id, receipt } => {
                    let pid = parse_program_id(&program_id)?;
                    let mid = parse_hex32(&multisig_id)?;
                    let prop = parse_hex32(&proposal_id)?;

                    let raw = std::fs::read(&receipt).context("read receipt file")?;
                    let receipt_words: Vec<u32> = raw.chunks_exact(4)
                        .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
                        .collect();
                    let r: risc0_zkvm::Receipt = risc0_zkvm::serde::from_slice(&receipt_words)
                        .map_err(|e| anyhow::anyhow!("deserialise receipt: {e}"))?;

                    #[derive(serde::Deserialize, borsh::BorshSerialize)]
                    struct Journal {
                        multisig_id: [u8; 32],
                        proposal_id: [u8; 32],
                        nullifier: [u8; 32],
                        member_set_root: [u8; 32],
                    }
                    let j: Journal = r.journal.decode()?;
                    // Pass the borsh-encoded journal as journal_bytes to the vote instruction.
                    let journal_bytes = borsh::to_vec(&j)?;
                    let instr = instr_vote(&prop, &journal_bytes);
                    let hash = chain::send_call(&sequencer, pid, mid, instr).await?;
                    println!("tx: {hash}");
                }
                ChainCmd::Execute { sequencer, program_id, multisig_id, proposal_id } => {
                    let pid = parse_program_id(&program_id)?;
                    let mid = parse_hex32(&multisig_id)?;
                    let prop = parse_hex32(&proposal_id)?;
                    let instr = instr_execute(&prop);
                    let hash = chain::send_call(&sequencer, pid, mid, instr).await?;
                    println!("tx: {hash}");
                }
            }
        }
    }

    Ok(())
}

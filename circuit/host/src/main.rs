use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use hex::FromHex;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

mod prover;
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
        /// Nullifier secret key (64-char hex). Keep private.
        #[arg(long)]
        nsk: String,
        /// Multisig account ID (64-char hex).
        #[arg(long)]
        multisig_id: String,
    },
    /// Generate a vote proof for a proposal.
    Vote {
        /// Nullifier secret key (64-char hex).
        #[arg(long)]
        nsk: String,
        /// Index of your commitment in the multisig member list.
        #[arg(long)]
        member_index: usize,
        /// Multisig account ID (64-char hex).
        #[arg(long)]
        multisig_id: String,
        /// Proposal ID (64-char hex).
        #[arg(long)]
        proposal_id: String,
        /// Sequencer JSON-RPC URL (to fetch current member_commitments).
        #[arg(long, default_value = "http://127.0.0.1:9090")]
        sequencer: String,
        /// Write receipt to this file.
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
}

fn parse_hex32(s: &str) -> Result<[u8; 32]> {
    let bytes = Vec::from_hex(s).context("invalid hex")?;
    <[u8; 32]>::try_from(bytes.as_slice())
        .map_err(|_| anyhow::anyhow!("expected 32 bytes, got {}", bytes.len()))
}

/// Member commitment: SHA256("member" || nsk || multisig_id).
/// Must match the guest circuit exactly.
fn derive_commitment(nsk: &[u8; 32], multisig_id: &[u8; 32]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"member");
    h.update(nsk);
    h.update(multisig_id);
    h.finalize().into()
}

async fn fetch_member_commitments(
    sequencer_url: &str,
    multisig_id: &[u8; 32],
) -> Result<Vec<[u8; 32]>> {
    let client = reqwest::Client::new();
    let resp: serde_json::Value = client
        .post(sequencer_url)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": "getAccountData",
            "params": { "account_id": hex::encode(multisig_id) },
            "id": 1,
        }))
        .send().await?
        .json().await?;

    let commitments_raw = resp["result"]["member_commitments"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("member_commitments not found in account data"))?;

    let mut out = Vec::with_capacity(commitments_raw.len());
    for c in commitments_raw {
        let hex_str = c.as_str().ok_or_else(|| anyhow::anyhow!("commitment must be string"))?;
        let bytes = hex::decode(hex_str)?;
        out.push(<[u8; 32]>::try_from(bytes.as_slice())
            .map_err(|_| anyhow::anyhow!("commitment must be 32 bytes"))?);
    }
    Ok(out)
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

        Cmd::Vote { nsk, member_index, multisig_id, proposal_id, sequencer, out } => {
            let nsk_bytes = parse_hex32(&nsk)?;
            let multisig_bytes = parse_hex32(&multisig_id)?;
            let proposal_bytes = parse_hex32(&proposal_id)?;

            eprintln!("Fetching member commitments from {}...", sequencer);
            let member_commitments = fetch_member_commitments(&sequencer, &multisig_bytes).await?;
            eprintln!("{} members registered", member_commitments.len());

            // Sanity check: our commitment must be at the expected index.
            let our_commitment = derive_commitment(&nsk_bytes, &multisig_bytes);
            anyhow::ensure!(
                member_index < member_commitments.len()
                    && member_commitments[member_index] == our_commitment,
                "nsk does not match the commitment at index {}; check --member-index",
                member_index
            );

            let input = ProverInput {
                nsk: nsk_bytes,
                member_index,
                member_commitments,
                multisig_id: multisig_bytes,
                proposal_id: proposal_bytes,
            };

            eprintln!("Running RISC0 prover...");
            let receipt = prove(input)?;

            let words = risc0_zkvm::serde::to_vec(&receipt)
                .map_err(|e| anyhow::anyhow!("serialise: {e}"))?;
            let bytes: Vec<u8> = bytemuck::cast_slice(&words).to_vec();
            std::fs::write(&out, &bytes)?;
            eprintln!("Receipt written to {}", out.display());

            #[derive(serde::Deserialize)]
            struct Journal { nullifier: [u8; 32], member_set_root: [u8; 32] }
            let j: Journal = receipt.journal.decode()?;
            println!("nullifier:       {}", hex::encode(j.nullifier));
            println!("member_set_root: {}", hex::encode(j.member_set_root));
        }

        Cmd::Verify { receipt, multisig_id, proposal_id } => {
            include!(concat!(env!("OUT_DIR"), "/methods.rs"));

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
    }

    Ok(())
}

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
}

fn parse_hex32(s: &str) -> Result<[u8; 32]> {
    let bytes = Vec::from_hex(s).context("invalid hex")?;
    <[u8; 32]>::try_from(bytes.as_slice())
        .map_err(|_| anyhow::anyhow!("expected 32 bytes, got {}", bytes.len()))
}

fn derive_commitment(nsk: &[u8; 32], multisig_id: &[u8; 32]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"member");
    h.update(nsk);
    h.update(multisig_id);
    h.finalize().into()
}

fn main() -> Result<()> {
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
    }

    Ok(())
}

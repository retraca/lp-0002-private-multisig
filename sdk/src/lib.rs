//! Client SDK for LP-0002 private M-of-N multisig.

use anyhow::Result;
use sha2::{Digest, Sha256};

/// Derive the member commitment for a given nsk and multisig account ID.
/// Must match the guest circuit: SHA256("member" || nsk || multisig_id).
/// Share `commitment` publicly; keep `nsk` secret.
pub fn derive_commitment(nsk: &[u8; 32], multisig_id: &[u8; 32]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"member");
    h.update(nsk);
    h.update(multisig_id);
    h.finalize().into()
}

/// Submit a vote receipt to the sequencer.
pub async fn submit_vote(
    sequencer_url: &str,
    multisig_id: &[u8; 32],
    proposal_id: &[u8; 32],
    receipt_bytes: &[u8],
) -> Result<String> {
    let client = reqwest::Client::new();
    let resp: serde_json::Value = client
        .post(sequencer_url)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": "submitVote",
            "params": {
                "multisig_id": hex::encode(multisig_id),
                "proposal_id": hex::encode(proposal_id),
                "receipt": hex::encode(receipt_bytes),
            },
            "id": 1,
        }))
        .send().await?
        .json().await?;

    if let Some(err) = resp.get("error") {
        anyhow::bail!("sequencer error: {}", err);
    }
    Ok(resp["result"]["tx_hash"].as_str().unwrap_or("").to_string())
}

/// Execute a proposal (after threshold met) via the sequencer.
pub async fn execute_proposal(
    sequencer_url: &str,
    multisig_id: &[u8; 32],
    proposal_id: &[u8; 32],
) -> Result<String> {
    let client = reqwest::Client::new();
    let resp: serde_json::Value = client
        .post(sequencer_url)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": "executeProposal",
            "params": {
                "multisig_id": hex::encode(multisig_id),
                "proposal_id": hex::encode(proposal_id),
            },
            "id": 1,
        }))
        .send().await?
        .json().await?;

    if let Some(err) = resp.get("error") {
        anyhow::bail!("sequencer error: {}", err);
    }
    Ok(resp["result"]["tx_hash"].as_str().unwrap_or("").to_string())
}

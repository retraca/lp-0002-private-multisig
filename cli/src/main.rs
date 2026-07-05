//! LP-0002 private multisig client CLI (LEZ v0.2.0).
//!
//! Key model: a member's voting secret is their shielded account's nullifier
//! secret key (nsk), HD-derived from a seed exactly the way a LEZ wallet does
//! (`SeedHolder -> SecretSpendingKey -> produce_private_key_holder(index)`).
//! Control of the nsk is control of the shielded account, so a registered
//! commitment binds membership to a real shielded account, and the guest's
//! rider assert binds every vote to that account being LIVE on chain.
//!
//! Chain commands read the wallet home from `LEE_WALLET_HOME_DIR` (the same
//! home the stock LEZ `wallet` binary uses); the sequencer address comes from
//! that home's `wallet_config.json`.

use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, Subcommand};
use common::transaction::LeeTransaction;
use key_protocol::key_management::secret_holders::SeedHolder;
use lee::privacy_preserving_transaction::circuit::ProgramWithDependencies;
use lee::program::Program;
use lee::program_deployment_transaction::{
    Message as DeployMessage, ProgramDeploymentTransaction,
};
use lee::public_transaction::{Message, WitnessSet};
use lee::{AccountId, PrivateKey, PublicKey, PublicTransaction};
use lee_core::account::{Account, Nonce};
use lee_core::program::ProgramId;
use lee_core::NullifierPublicKey;
use private_multisig_program::{MultisigInstruction, VOTE_IDENTIFIER};
use private_multisig_sdk as sdk;
use sequencer_service_rpc::RpcClient as _;
use wallet::{AccountIdentity, WalletCore};

#[derive(Parser)]
#[command(name = "multisig", about = "LP-0002 private M-of-N multisig client")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Member key derivation and wallet import.
    #[command(subcommand)]
    Member(MemberCmd),
    /// Derive a member registration commitment (nsk stays local).
    DeriveCommitment {
        #[arg(long)]
        nsk: String,
        #[arg(long)]
        multisig_id: String,
    },
    /// On-chain operations (wallet home from LEE_WALLET_HOME_DIR).
    #[command(subcommand)]
    Chain(ChainCmd),
}

#[derive(Subcommand)]
enum MemberCmd {
    /// Derive a member's voting identity from a seed (new random seed if omitted).
    New {
        /// 32-byte hex entropy for the member's HD seed.
        #[arg(long)]
        seed: Option<String>,
        /// HD index of the voting key.
        #[arg(long, default_value_t = 0)]
        index: u32,
    },
    /// Import the member's voting account into this wallet's key tree so it
    /// can ride votes (`AccountIdentity::PrivateOwned`).
    Import {
        #[arg(long)]
        seed: String,
        #[arg(long, default_value_t = 0)]
        index: u32,
    },
}

#[derive(Subcommand)]
enum ChainCmd {
    /// Generate the multisig account key (one-time bootstrap credential).
    Keygen,
    /// Print the program id of a guest ELF.
    ProgramId {
        #[arg(long)]
        program_bin: String,
    },
    /// Deploy the multisig program.
    Deploy {
        #[arg(long)]
        program_bin: String,
    },
    /// Initialize (claim) the multisig account: threshold + member commitments.
    Initialize {
        #[arg(long)]
        program_bin: String,
        #[arg(long)]
        signing_key: String,
        #[arg(long)]
        threshold: u8,
        /// Comma-separated 32-byte hex commitments.
        #[arg(long)]
        commitments: String,
    },
    /// Submit a proposal (unsigned: the multisig account is program-owned).
    SubmitProposal {
        #[arg(long)]
        program_bin: String,
        #[arg(long)]
        multisig_id: String,
        #[arg(long)]
        proposal_id: String,
        /// Hex action bytes (the parameter change this proposal gates).
        #[arg(long)]
        action: String,
    },
    /// Cast an anonymous vote (privacy-preserving transaction, real proof).
    Vote {
        #[arg(long)]
        program_bin: String,
        #[arg(long)]
        multisig_id: String,
        #[arg(long)]
        proposal_id: String,
        #[arg(long)]
        nsk: String,
        #[arg(long)]
        member_index: u32,
    },
    /// Execute a proposal once the threshold is met (unsigned).
    Execute {
        #[arg(long)]
        program_bin: String,
        #[arg(long)]
        multisig_id: String,
        #[arg(long)]
        proposal_id: String,
    },
    /// Read and decode the multisig account state.
    State {
        #[arg(long)]
        multisig_id: String,
    },
}

fn hex32(s: &str) -> Result<[u8; 32]> {
    let v = hex::decode(s.trim()).context("invalid hex")?;
    v.try_into().map_err(|_| anyhow!("expected 32 bytes"))
}

fn load_program(path: &str) -> Result<Program> {
    let bytecode = std::fs::read(path).with_context(|| format!("read {path}"))?;
    Program::new(bytecode).map_err(|e| anyhow!("load program: {e}"))
}

fn program_id_hex(id: &ProgramId) -> String {
    hex::encode(id.iter().flat_map(|w| w.to_le_bytes()).collect::<Vec<_>>())
}

/// HD-derive a member's voting identity the same way a LEZ wallet does.
fn member_holder(
    seed: &[u8; 32],
    index: u32,
) -> Result<key_protocol::key_management::KeyChain> {
    let mnemonic = bip39::Mnemonic::from_entropy(seed).context("seed entropy")?;
    let seed_holder = SeedHolder::from_mnemonic(&mnemonic, "");
    let ssk = seed_holder.produce_top_secret_key_holder();
    let holder = ssk.produce_private_key_holder(Some(index));
    let npk = holder.generate_nullifier_public_key();
    let vpk = holder.generate_viewing_public_key();
    Ok(key_protocol::key_management::KeyChain {
        secret_spending_key: ssk,
        private_key_holder: holder,
        nullifier_public_key: npk,
        viewing_public_key: vpk,
    })
}

async fn send_public(
    wallet: &WalletCore,
    program_id: ProgramId,
    account_ids: Vec<AccountId>,
    nonces: Vec<Nonce>,
    signers: &[&PrivateKey],
    instruction: MultisigInstruction,
) -> Result<String> {
    let message = Message::try_new(program_id, account_ids, nonces, instruction)
        .map_err(|e| anyhow!("build message: {e}"))?;
    let witness_set = WitnessSet::for_message(&message, signers);
    let tx = PublicTransaction::new(message, witness_set);
    let hash = wallet
        .sequencer_client
        .send_transaction(LeeTransaction::Public(tx))
        .await
        .map_err(|e| anyhow!("send_transaction: {e}"))?;
    Ok(hex::encode(hash))
}

#[tokio::main]
async fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Member(MemberCmd::New { seed, index }) => {
            let entropy: [u8; 32] = match seed {
                Some(s) => hex32(&s)?,
                None => rand::random(),
            };
            let kc = member_holder(&entropy, index)?;
            let nsk = kc.private_key_holder.nullifier_secret_key;
            let voting_id =
                AccountId::for_regular_private_account(&kc.nullifier_public_key, VOTE_IDENTIFIER);
            println!("seed: {}", hex::encode(entropy));
            println!("index: {index}");
            println!("nsk: {}", hex::encode(nsk));
            println!("voting_account: Private/{voting_id}");
        }
        Cmd::Member(MemberCmd::Import { seed, index }) => {
            let entropy = hex32(&seed)?;
            let kc = member_holder(&entropy, index)?;
            let voting_id =
                AccountId::for_regular_private_account(&kc.nullifier_public_key, VOTE_IDENTIFIER);
            let mut wallet = WalletCore::from_env()?;
            wallet.storage_mut().key_chain_mut().add_imported_private_account(
                kc,
                None,
                VOTE_IDENTIFIER,
                Account::default(),
            );
            wallet.store_persistent_data()?;
            println!("imported voting_account: Private/{voting_id}");
        }
        Cmd::DeriveCommitment { nsk, multisig_id } => {
            let c = sdk::derive_commitment(&hex32(&nsk)?, &hex32(&multisig_id)?);
            println!("{}", hex::encode(c));
        }
        Cmd::Chain(chain) => run_chain(chain).await?,
    }
    Ok(())
}

async fn run_chain(cmd: ChainCmd) -> Result<()> {
    match cmd {
        ChainCmd::Keygen => {
            let key = PrivateKey::new_os_random();
            let account_id = AccountId::from(&PublicKey::new_from_private_key(&key));
            println!("signing_key: {key}");
            println!("multisig_id: {}", hex::encode(account_id.value()));
            println!("multisig_account: Public/{account_id}");
        }
        ChainCmd::ProgramId { program_bin } => {
            let program = load_program(&program_bin)?;
            println!("program_id: {}", program_id_hex(&program.id()));
        }
        ChainCmd::Deploy { program_bin } => {
            let wallet = WalletCore::from_env()?;
            let program = load_program(&program_bin)?;
            println!("program_id: {}", program_id_hex(&program.id()));
            let message = DeployMessage::new(program.elf().to_vec());
            let tx = ProgramDeploymentTransaction::new(message);
            let hash = wallet
                .sequencer_client
                .send_transaction(LeeTransaction::ProgramDeployment(tx))
                .await
                .map_err(|e| anyhow!("send_transaction: {e}"))?;
            println!("tx: {}", hex::encode(hash));
        }
        ChainCmd::Initialize {
            program_bin,
            signing_key,
            threshold,
            commitments,
        } => {
            let wallet = WalletCore::from_env()?;
            let program = load_program(&program_bin)?;
            let key = PrivateKey::try_new(hex32(&signing_key)?)
                .map_err(|e| anyhow!("signing key: {e}"))?;
            let account_id = AccountId::from(&PublicKey::new_from_private_key(&key));
            let commitments: Vec<[u8; 32]> = commitments
                .split(',')
                .map(hex32)
                .collect::<Result<_>>()?;
            let nonces = wallet
                .sequencer_client
                .get_accounts_nonces(vec![account_id])
                .await
                .map_err(|e| anyhow!("get_accounts_nonces: {e}"))?;
            let tx = send_public(
                &wallet,
                program.id(),
                vec![account_id],
                nonces,
                &[&key],
                MultisigInstruction::Initialize {
                    threshold,
                    commitments,
                },
            )
            .await?;
            println!("multisig_id: {}", hex::encode(account_id.value()));
            println!("tx: {tx}");
        }
        ChainCmd::SubmitProposal {
            program_bin,
            multisig_id,
            proposal_id,
            action,
        } => {
            let wallet = WalletCore::from_env()?;
            let program = load_program(&program_bin)?;
            let tx = send_public(
                &wallet,
                program.id(),
                vec![AccountId::new(hex32(&multisig_id)?)],
                vec![],
                &[],
                MultisigInstruction::SubmitProposal {
                    proposal_id: hex32(&proposal_id)?,
                    action: hex::decode(action.trim()).context("action hex")?,
                },
            )
            .await?;
            println!("tx: {tx}");
        }
        ChainCmd::Vote {
            program_bin,
            multisig_id,
            proposal_id,
            nsk,
            member_index,
        } => {
            let wallet = WalletCore::from_env()?;
            let program = load_program(&program_bin)?;
            let program_with_deps: ProgramWithDependencies = program.into();
            let nsk = hex32(&nsk)?;
            let voting_id = AccountId::for_regular_private_account(
                &NullifierPublicKey::from(&nsk),
                VOTE_IDENTIFIER,
            );

            // Fail fast before proving: the voting account must be live and
            // tracked by this wallet (the guest's rider assert would reject a
            // non-live rider in-circuit anyway).
            if wallet
                .check_private_account_initialized(voting_id)
                .await?
                .is_none()
            {
                bail!(
                    "voting account Private/{voting_id} is not live/tracked; \
                     run `multisig member import` and fund it first"
                );
            }

            let instruction = Program::serialize_instruction(MultisigInstruction::Vote {
                nsk,
                member_index,
                proposal_id: hex32(&proposal_id)?,
            })
            .map_err(|e| anyhow!("serialize instruction: {e}"))?;

            // Account order must match the guest: [multisig (public), rider (private)].
            let accounts = vec![
                AccountIdentity::PublicNoSign(AccountId::new(hex32(&multisig_id)?)),
                AccountIdentity::PrivateOwned(voting_id),
            ];

            println!("proving vote locally (nsk never leaves this machine)...");
            let (tx_hash, _) = wallet
                .send_privacy_preserving_tx(accounts, instruction, &program_with_deps)
                .await
                .map_err(|e| {
                    anyhow!(
                        "vote rejected (not a member / double vote / proposal missing / \
                         voting account not live): {e}"
                    )
                })?;
            println!("tx: {tx_hash}");
        }
        ChainCmd::Execute {
            program_bin,
            multisig_id,
            proposal_id,
        } => {
            let wallet = WalletCore::from_env()?;
            let program = load_program(&program_bin)?;
            let tx = send_public(
                &wallet,
                program.id(),
                vec![AccountId::new(hex32(&multisig_id)?)],
                vec![],
                &[],
                MultisigInstruction::Execute {
                    proposal_id: hex32(&proposal_id)?,
                },
            )
            .await?;
            println!("tx: {tx}");
        }
        ChainCmd::State { multisig_id } => {
            let wallet = WalletCore::from_env()?;
            let account = wallet
                .sequencer_client
                .get_account(AccountId::new(hex32(&multisig_id)?))
                .await
                .map_err(|e| anyhow!("get_account: {e}"))?;
            println!("program_owner: {}", program_id_hex(&account.program_owner));
            match sdk::decode_state(account.data.as_ref()) {
                Ok(state) => print!("{}", sdk::render_state(&state)),
                Err(_) => println!("data: {} bytes (not initialized)", account.data.as_ref().len()),
            }
        }
    }
    Ok(())
}

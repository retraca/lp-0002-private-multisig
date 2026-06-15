# LP-0002 Private M-of-N Multisig

Private M-of-N multisig for the Logos Execution Zone. Members hold shielded accounts; approvals leave no on-chain trace of who voted.

## Design

**LEZ nonce constraint**: LEZ private accounts increment their nonce on every spend, and are owned by the privacy protocol. Direct signing is incompatible with multisig participation. This implementation avoids that constraint: members register a separate `member_commitment = SHA256("member" || nsk || multisig_id)` at setup time. The nsk is client-side only; it never touches the chain.

**Vote privacy (chained-call composition)**: A vote is a privacy-preserving transaction whose initial call is the **vote-circuit program** (`programs/vote_circuit`). The voter executes and proves it locally: the nsk is initial-call instruction data, which never appears on-chain in the PPE model. The vote-circuit program recomputes the member commitment against live multisig state, derives the nullifier and the member set root, and declares a **ChainedCall** into the multisig program's `vote` instruction. The LEZ PPE outer circuit proves the chained-call linkage (caller identity cannot be spoofed), so the multisig program's check `caller_program_id == registered vote-circuit program` is equivalent to verifying the membership proof. On-chain observers see one privacy-preserving transaction and the updated multisig state — never which member voted, and not even that the transaction was a vote rather than a private transfer (a fresh zero-balance private "voter note" gives each vote the same commitment/nullifier shape as any private transaction).

**Double-vote prevention**: Nullifier = `SHA256("multisig/v1/vote" || nsk || proposal_id || multisig_id)`. Same member, same proposal, same nullifier -- rejected on second attempt.

**Account claiming**: `initialize` claims the multisig account, which requires the transaction to be signed with the account's key (`chain keygen` generates one). After claiming, the account is program-owned and the key is never needed again; proposals, votes, and execution need no wallet signatures.

## Components

| Path | Role |
|------|------|
| `programs/multisig` | LEZ on-chain program (`initialize`, `submit_proposal`, `vote`, `execute`) |
| `programs/vote_circuit` | Vote-circuit program: membership proof + chained vote delivery |
| `circuit/guest` | Standalone RISC0 circuit for offline vote receipts (off-chain coordination) |
| `circuit/host` | CLI: offline (`derive-commitment / vote / verify`) + on-chain (`chain …`) |
| `sdk` | Client SDK |

## Quick start

```bash
./demo.sh --dev                 # offline: commitments, two vote proofs, verification

# full on-chain lifecycle against the hosted testnet (real proofs):
cargo build --release --features chain
SEQUENCER=https://testnet.lez.logos.co ./demo.sh --chain
```

## On-chain usage

```bash
# 0. Generate the multisig account key (one-time bootstrap credential)
multisig chain keygen
# -> signing_key + multisig_id (members derive commitments against this ID)

# 1. Each member derives their commitment (nsk stays local)
multisig derive-commitment --nsk <64-hex> --multisig-id <multisig_id>

# 2. Initialize on-chain (signed; registers threshold, members, vote-circuit program)
multisig chain initialize --sequencer <url> \
  --program-id <multisig-program-id> \
  --vote-circuit-program-id <vote-circuit-program-id> \
  --signing-key <hex> --threshold 2 --commitments <c1,c2,c3>

# 3. Submit a proposal (unsigned: account is program-owned)
multisig chain submit-proposal --sequencer <url> --program-id <hex> \
  --multisig-id <hex> --proposal-id <hex> --action <hex>

# 4. Vote: one privacy-preserving transaction per member.
#    Proving runs locally; the nsk never leaves the machine.
multisig chain vote --sequencer <url> --program-id <hex> \
  --multisig-id <hex> --proposal-id <hex> \
  --nsk <64-hex> --member-index <N>

# 5. Execute once the threshold is met (unsigned)
multisig chain execute --sequencer <url> --program-id <hex> \
  --multisig-id <hex> --proposal-id <hex>

# Inspect state at any point
multisig chain state --sequencer <url> --multisig-id <hex>
```

Deployed on the hosted LEZ testnet (`https://testnet.lez.logos.co`) — program IDs, account IDs, and transaction hashes in [docs/TESTNET_EVIDENCE.md](docs/TESTNET_EVIDENCE.md).

## Error codes

| Code | Meaning |
|------|---------|
| 6001 | ERR_PROOF_INVALID |
| 6002 | ERR_MULTISIG_MISMATCH |
| 6003 | ERR_PROPOSAL_NOT_FOUND |
| 6004 | ERR_NULLIFIER_SPENT |
| 6005 | ERR_MEMBER_NOT_REGISTERED |
| 6006 | ERR_PROPOSAL_ALREADY_EXECUTED |
| 6007 | ERR_THRESHOLD_NOT_MET |
| 6008 | ERR_ALREADY_INITIALIZED |
| 6009 | ERR_TOO_MANY_MEMBERS |
| 6010 | ERR_INVALID_THRESHOLD |
| 6011 | ERR_TOO_MANY_PROPOSALS |
| 6012 | ERR_UNAUTHORIZED_CALLER |

## License

MIT or Apache-2.0

## Why not Semaphore or MACI?

**Semaphore** is an EVM Groth16 circuit that proves group membership via a nullifier. It is chain-agnostic in principle, but its identity commitment is a Pedersen hash over a Poseidon-hashed secret, whereas LEZ already provides SHA256-based commitments; mixing two hash primitives inside one guest adds proof-size overhead. Semaphore is also group-membership only: it has no first-class multisig state machine (threshold, proposals, per-proposal nullifier sets), so the custom state machine would be needed regardless.

**MACI** (Minimum Anti-Collusion Infrastructure) solves a different problem: preventing vote-buying by making votes private even to the voter after submission via an operator-mediated tally. It is designed for quadratic voting with many participants, not M-of-N threshold execution. The multisig case needs the opposite guarantee: members must be able to confirm their own vote was counted before execution.

## Security

See [docs/SECURITY.md](docs/SECURITY.md) for the full threat model: trusted setup, membership-set privacy under chain analysis, sequencer adversary, nullifier unlinkability, and known limitations.

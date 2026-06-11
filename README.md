# LP-0002 Private M-of-N Multisig

Private M-of-N multisig for the Logos Execution Zone. Members hold shielded accounts; approvals leave no on-chain trace of who voted.

## Design

**LEZ nonce constraint**: LEZ private accounts increment their nonce on every spend, and are owned by the privacy protocol. Direct signing is incompatible with multisig participation. This implementation avoids that constraint: members register a separate `member_commitment = SHA256("member" || nsk || multisig_id)` at setup time. The nsk is client-side only; it never touches the chain.

**Vote privacy**: To vote, a member runs the RISC0 guest off-chain. The guest proves nsk knowledge and produces a nullifier. The on-chain verifier sees the proof, the nullifier, and the member commitment -- but not which member (the commitment is one of N registered values; the verifier just checks it's in the set).

**Double-vote prevention**: Nullifier = `SHA256("multisig/v1/vote" || nsk || proposal_id || multisig_id)`. Same member, same proposal, same nullifier -- rejected on second attempt.

## Components

| Path | Role |
|------|------|
| `circuit/guest` | RISC0 zkVM guest circuit |
| `circuit/host` | CLI: `multisig derive-commitment / vote / verify` |
| `programs/multisig` | LEZ on-chain program (`initialize`, `submit_proposal`, `vote`, `execute`) |
| `sdk` | Client SDK |

## Quick start

```bash
docker compose up -d
./demo.sh --dev   # RISC0_DEV_MODE=1 for instant proofs
```

## CLI usage

```bash
# Step 1: each member derives their commitment (nsk stays local)
multisig derive-commitment --nsk <64-hex> --multisig-id <64-hex>

# Step 2: creator initializes multisig with threshold + all commitments

# Step 3: submit a proposal
multisig submit-proposal --multisig-id <hex> --proposal-id <hex> --action <hex>

# Step 4: members vote
multisig vote \
  --nsk <64-hex> \
  --member-index <N> \
  --multisig-id <hex> \
  --proposal-id <hex> \
  --out vote.bin

# Step 5: verify offline
multisig verify --receipt vote.bin --multisig-id <hex> --proposal-id <hex>

# Step 6: execute (after threshold met)
multisig execute --multisig-id <hex> --proposal-id <hex>
```

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

## License

MIT or Apache-2.0

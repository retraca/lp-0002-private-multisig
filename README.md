# LP-0002 Private M-of-N Multisig

Private M-of-N multisig for the Logos Execution Zone (LEZ v0.2.0). Members hold
shielded accounts; approvals leave no on-chain trace of who voted; the chain
records only that a threshold was met.

## Design

**LEZ nonce + program_owner constraints.** LEZ private accounts are owned by
the privacy protocol and rotate nonce/commitment on every use, so they cannot
be claimed by a multisig program the way the public `lez-multisig` PoC claims
fresh zero-nonce keypairs. This implementation sidesteps both constraints:

- The **multisig account** is a regular public account claimed by the program
  at `initialize` (one signature with its throwaway key; program-owned
  forever after — proposals, votes and execution need no signatures on it).
- A **member's identity** is their shielded account's nullifier secret key
  (`nsk`), HD-derived by their wallet. Only a one-way commitment
  `SHA256("member" || nsk || multisig_id)` is registered on-chain; the nsk —
  and the account it controls — is never linkable from it.
- **Voting never spends the member's shielded balance and never exposes the
  account**: the vote rides the account through the standard LEZ privacy
  circuit, which rotates its commitment exactly like a private transfer, so a
  vote is indistinguishable from ordinary shielded traffic.

**Anonymous votes (privacy-preserving initial call).** A vote is a
privacy-preserving transaction whose initial call is this program's `Vote`
instruction. The instruction data — including the nsk — is a private witness:
the voter executes and proves the program locally, and the sequencer sees only
the composite STARK plus the public multisig post-state. In-circuit the
program:

1. recomputes `SHA256("member" || nsk || multisig_id)` and matches it against
   the registered commitment set (membership, without revealing which member);
2. **binds the vote to the member's LIVE shielded account** (review-grade
   binding, not derivation-only): the transaction's private rider must satisfy
   `rider.account_id == AccountId::for_regular_private_account(npk(nsk), 0)`
   and be non-default, and the LEZ privacy circuit proves the rider's
   pre-state commitment is in the live commitment tree;
3. derives the proposal-bound nullifier
   `SHA256("multisig/v1/vote" || nsk || proposal_id || multisig_id)` and
   rejects double votes;
4. increments the public vote count.

**Double-vote prevention**: same member + same proposal ⇒ same nullifier ⇒ the
second proof aborts in-circuit with `ERR_6004` (it cannot even be generated).

**Execution**: once `vote_count >= threshold`, `Execute` finalizes the
proposal's action bytes (a threshold-gated parameter change). On-chain state
after the full lifecycle shows the threshold, the vote count and the opaque
nullifiers — never which members approved.

## Components

| Path | Role |
|------|------|
| `programs/multisig` | On-chain program (guest): `Initialize`, `SubmitProposal`, `Vote`, `Execute` |
| `cli` | `multisig` client: key derivation, wallet import, all four instructions, state reads |
| `sdk` | Types + derivation + state decoding for Logos module builders |
| `basecamp-app` | Logos Basecamp app GUI |
| `lp-0002-private-multisig.idl.json` | SPEL IDL for the program |

## Quick start (the evaluator path)

Prerequisites: a Rust toolchain (`rustup`), the RISC0 toolchain
(`curl -sSfL https://risczero.com/install | bash && rzup install rust && rzup install r0vm`),
`python3`, `git`.

```bash
./demo.sh          # REAL proofs (RISC0_DEV_MODE=0) — the submission-grade run
./demo.sh --dev    # fake receipts — fast logic run (what CI runs)
```

The script builds LEZ v0.2.0 (standalone sequencer + wallet, first run only,
~30-60 min), builds this repo, boots a local sequencer, and drives the full
2-of-3 lifecycle: initialize → proposal → fund the two voting accounts → two
anonymous votes (each a real STARK by default) → a sequencer `kill -9` +
restart proving partial approvals are resumable → a double-vote attempt that
MUST fail in-circuit → execute → final state assertion. Exit 0 = every step
verified.

## On-chain usage (CLI)

All `chain` commands read the wallet home from `LEE_WALLET_HOME_DIR` (same
home as the stock LEZ `wallet`); the sequencer address comes from that home's
`wallet_config.json`.

```bash
# 0. Build the guest and note its program id
cd programs/multisig && cargo +risc0 build --release --target riscv32im-risc0-zkvm-elf && cd -
multisig chain program-id --program-bin $ELF

# 1. Deploy (once per chain)
multisig chain deploy --program-bin $ELF

# 2. Each member derives a voting identity (nsk stays local, forever)
multisig member new                       # prints seed, nsk, voting account
multisig member import --seed <seed>      # tracks the voting account in this wallet
# fund the voting account (any shielded transfer makes it LIVE):
wallet auth-transfer send --from Public/<funder> --to Private/<voting-account> --amount 5

# 3. Create the multisig: fresh account key + everyone's commitments
multisig chain keygen                     # -> signing_key + multisig_id
multisig derive-commitment --nsk <nsk> --multisig-id <multisig_id>
multisig chain initialize --program-bin $ELF --signing-key <key> \
  --threshold 2 --commitments <c0,c1,c2>

# 4. Propose (unsigned: the account is program-owned)
multisig chain submit-proposal --program-bin $ELF --multisig-id <id> \
  --proposal-id <32-byte-hex> --action <hex>

# 5. Vote — one privacy-preserving transaction per member, proved locally
multisig chain vote --program-bin $ELF --multisig-id <id> \
  --proposal-id <pid> --nsk <nsk> --member-index 0

# 6. Execute once the threshold is met (unsigned)
multisig chain execute --program-bin $ELF --multisig-id <id> --proposal-id <pid>

# Inspect state at any time
multisig chain state --multisig-id <id>
```

Deployed on the hosted LEZ testnet (`https://testnet.lez.logos.co`, pinned to
LEZ v0.2.0): program id, account ids and transaction hashes in
[docs/TESTNET_EVIDENCE.md](docs/TESTNET_EVIDENCE.md).

## Error codes

Deterministic and documented; in-guest failures abort the proof with the code
in the panic string (`ERR_<code> <message>`).

| Code | Meaning |
|------|---------|
| 6001 | ERR_PROOF_INVALID — state deserialize failed |
| 6002 | ERR_MULTISIG_MISMATCH |
| 6003 | ERR_PROPOSAL_NOT_FOUND |
| 6004 | ERR_NULLIFIER_SPENT — double vote |
| 6005 | ERR_MEMBER_NOT_REGISTERED — bad nsk or index |
| 6006 | ERR_PROPOSAL_ALREADY_EXECUTED |
| 6007 | ERR_THRESHOLD_NOT_MET |
| 6008 | ERR_ALREADY_INITIALIZED — account or proposal id reuse |
| 6009 | ERR_TOO_MANY_MEMBERS |
| 6010 | ERR_INVALID_THRESHOLD |
| 6011 | ERR_TOO_MANY_PROPOSALS |
| 6012 | ERR_RIDER_MISMATCH — rider not derived from the voting nsk |
| 6013 | ERR_RIDER_NOT_LIVE — rider account not live on chain |

## Basecamp app

`basecamp-app/` contains the Logos Basecamp GUI with local build instructions;
packaged assets are attached to the GitHub release (see
[basecamp-app/README.md](basecamp-app/README.md)).

## Benchmarks

Proof generation time and per-operation compute costs (RISC0 cycle counts) are
documented in [docs/BENCHMARKS.md](docs/BENCHMARKS.md).

## Why not Semaphore or MACI?

**Semaphore** proves group membership behind a nullifier on EVM/Groth16. Its
identity commitment stacks Pedersen and Poseidon hashes, while LEZ natively
speaks SHA256 (accelerated in the RISC0 zkVM); reusing its circuits would mix
hash primitives for no gain, and Semaphore has no multisig state machine
(threshold, proposals, per-proposal nullifier sets) — that logic is needed
regardless. This implementation keeps the Semaphore *pattern* (commitment set
+ membership proof + domain-separated nullifier) natively on LEZ.

**MACI** solves collusion resistance for large-scale voting via an
operator-mediated tally, and makes votes private even from the voter after
submission. An M-of-N multisig needs the opposite: members must be able to
confirm their vote counted before execution, with no trusted operator.

## Security

See [docs/SECURITY.md](docs/SECURITY.md) for the threat model: trusted setup
(none), membership-set privacy under chain analysis, sequencer adversary,
nullifier unlinkability, and known limitations.

## License

MIT — see [LICENSE](LICENSE).

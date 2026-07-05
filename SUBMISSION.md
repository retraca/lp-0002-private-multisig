# LP-0002 Submission Summary — Private M-of-N Multisig

- **Builder:** @retraca
- **Repo:** `retraca/lp-0002-private-multisig` (MIT), default branch `main`
- **Target:** LEZ **v0.2.0** (the version the hosted testnet is pinned to, commit `a58fbce2`)
- **Program id (testnet):** `6ce658db7384e1f6a90528715404856d55f2e977667d0a5989c8ed633d55afad`
- **Live evidence:** [docs/TESTNET_EVIDENCE.md](docs/TESTNET_EVIDENCE.md) — 2-of-3 instance `BVBgRAv7v2z9U13gxeKfEGogjjiAEnAhfVNabv6aUPQw`, proposal approved by threshold and executed, all re-queryable
- **Evaluator entry point:** `./demo.sh` (real proofs, local standalone sequencer; `--dev` for the fast logic run CI uses)
- **Write-up:** [docs/SOLUTION.md](docs/SOLUTION.md) · **Threat model:** [docs/SECURITY.md](docs/SECURITY.md) · **Benchmarks:** [docs/BENCHMARKS.md](docs/BENCHMARKS.md) · **Verification ledger:** [docs/CRITERIA_LEDGER.md](docs/CRITERIA_LEDGER.md)

## Success criteria checklist

Statuses verified 2026-07-05/06; the ledger has the exact command + evidence
per row.

### Functionality
- [x] Anonymous approval by shielded-account members — privacy-preserving tx, nsk private witness, **in-circuit live-account rider binding** (guest asserts + LEZ circuit commitment-tree membership). Live votes `b3a437b7…`, `4e0322d1…`.
- [x] Threshold confirmed without recording which members approved — final on-chain state holds only vote_count + opaque nullifiers (decode it yourself: TESTNET_EVIDENCE.md).
- [x] Double-vote prevention via nullifiers — second vote by the same member aborts **in-circuit** (`ERR_6004`); no transaction is even produced. Demonstrated live on testnet and in every demo run.
- [x] Execution unlinkable to any member's shielded account.
- [x] Client-side proof generation — 1,572,864 cycles/vote (single segments), minutes on commodity hardware.
- [x] Reference integration on LEZ testnet — threshold-gated parameter change (`set fee_bps = 25`) executed by a 2-of-3 with shielded member accounts.
- [x] ≥1 multisig instance on testnet with a proposal submitted, threshold-approved, executed; reproducible (`scripts/testnet-run.sh`) with evidence.
- [x] Full documentation + clean public repository.

### Usability
- [x] SDK (`sdk/`) — instruction set, state decoding, derivations for Logos module builders; the CLI is the reference client.
- [x] Basecamp app GUI (`basecamp-app/`) with local build instructions and downloadable assets ([release v0.2.0](https://github.com/retraca/lp-0002-private-multisig/releases/tag/v0.2.0)).
- [x] SPEL IDL (`lp-0002-private-multisig.idl.json`) — 4 instructions, account layouts, 13 documented error codes.

### Reliability
- [x] Proof-generation failures surface a clear error (`Failed to prove program: Guest panicked: ERR_6004 …`).
- [x] Partial approvals preserved + resumable — approvals live on-chain; demo kills the sequencer `-9` mid-flow at 1-of-2 and resumes to threshold after restart.
- [x] Deterministic, documented error codes 6001–6013 for all invalid-proof and double-vote scenarios.

### Performance
- [x] Per-operation compute documented ([docs/BENCHMARKS.md](docs/BENCHMARKS.md)): vote = 524,288-cycle guest session + 1,048,576-cycle privacy-circuit session; public ops take the no-proof path within the sequencer's 32M-cycle budget.

### Supportability
- [x] Deployed + tested on the LEZ testnet (v0.2.0).
- [x] E2E vs a standalone sequencer in CI (`.github/workflows/ci.yml`, `e2e-sequencer` job runs `./demo.sh --dev` from a clean checkout).
- [x] CI green on the default branch.
- [x] README documents deployment, program addresses, CLI + Basecamp usage.
- [x] Reproducible demo script against a real local sequencer with `RISC0_DEV_MODE=0` (`./demo.sh`, exit 0 from a fresh clone).
- [ ] Recorded, narrated video demo — silent cut + narration script (docs/VIDEO_NARRATION.md) ready; **builder voice-over pending**.

## How this compares (per reviewer feedback on prior submissions)

The six items flagged on PR #91 (the strongest competitor's first round), as
they stand here: **CU cost** measured, not TBD (cycles above); **resume across
restarts** demonstrated inside the single demo script (sequencer kill -9 at
1-of-2); **e2e in CI** green; **README walkthrough** complete; **Basecamp
assets** hosted as a release downloadable; **anonymous-approval binding** is
in-circuit against the member's live shielded account — the rider must derive
from the same nsk as the membership commitment AND its pre-state commitment
is proven live in the chain's commitment tree by the LEZ privacy circuit
(stronger than a derivation-only argument).

Two additional properties of this design worth noting:
- Deployed and evidenced on **v0.2.0 final** — the version the hosted testnet
  currently runs (rc5-era ledgers predate the redeploy).
- An invalid vote (double vote, non-member) **fails at proof time**: it costs
  the attacker the work and the chain nothing, and error codes are still
  deterministic and documented.

## Terms

Original work by @retraca, licensed MIT. I hold the rights to all submitted
code and agree to the λ-prize terms.

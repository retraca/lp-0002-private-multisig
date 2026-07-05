# LP-0002 Criteria Ledger

Source of truth: live spec pulled 2026-07-03 via
`gh api repos/logos-co/lambda-prize/contents/prizes/LP-0002.md`.
Statuses: VERIFIED (ran this session, saw it pass) · BUILDER-ONLY (needs the
builder) · CLAIMED (doc says done, not re-run) · PARTIAL · MISSING.

Progress 2026-07-05: full v0.2.0 port landed on branch v020-port. Dev-mode
e2e (deploy → init → proposal → fund riders → 2 anonymous votes → kill-9
resume → double-vote rejected ERR_6004 → execute) PASSED on the build VM,
DEMO_RC=0 (~/demo-dev.out 22:13 UTC). Real-proof run + testnet redeploy in
flight. Unit tests 6+1 green. In-circuit live-rider binding implemented
(guest asserts + LEZ privacy circuit membership).

Context that resets everything: the hosted testnet was wiped and now runs
**LEZ v0.2.0** (tag `v0.2.0`, commit `a58fbce2`). All prior v0.1.2 evidence in
docs/TESTNET_EVIDENCE.md is SUPERSEDED. Reviewer feedback on competing PR #91
(weboko, 2026-06-24) defines six extra bars: CU cost, partial-approval resume,
e2e-in-CI vs standalone sequencer, full README walkthrough, hosted Basecamp
downloadables, in-circuit live-account binding. My PR #87 was closed solely for
the missing video (mart1n-xyz, 2026-06-12).

## Functionality

| ID | Criterion (verbatim) | Status | How verified (command) | Evidence (result + commit) | Last verified |
|----|---------------------|--------|------------------------|---------------------------|---------------|
| F1 | Any M-of-N member holding a shielded LEZ account can submit an approval without revealing their identity to on-chain observers or other members. | PARTIAL | — | Anonymous approval works (vote-circuit PPE tx, nsk private input), but nsk is a free-standing secret: binding to a *shielded account* is registration-time only. Needs in-circuit live-account binding (weboko reef #6). | — |
| F2 | The on-chain verifier confirms a threshold of M approvals was reached without recording which members approved. | CLAIMED | `./demo.sh --chain` state check | Implemented (`apply_vote`/`apply_execute`, journal carries nullifier+root only) and ran on the OLD v0.1.2 testnet. Not re-run this session, chain since wiped. | — |
| F3 | A member cannot approve the same proposal twice (double-vote prevention via nullifiers or equivalent). | CLAIMED | second vote with same nsk → ERR_NULLIFIER_SPENT 6004 | Implemented; not re-run this session. | — |
| F4 | A completed execution is unlinkable to any individual member's shielded account. | CLAIMED | inspect on-chain state post-execute | State stores nullifiers only; not re-verified on current chain. | — |
| F5 | Proof generation runs client-side on a standard laptop. | CLAIMED | `multisig chain vote` local prove | Host CLI proves locally; timing not re-measured this session. | — |
| F6 | A reference integration is delivered: a working demo of a threshold-gated action (e.g., treasury transfer or parameter change) on LEZ testnet using shielded member accounts. | MISSING | — | Old v0.1.2 run superseded by chain wipe; must re-run on v0.2.0 with shielded member accounts. | — |
| F7 | At least 1 multisig instance is created on LEZ testnet, with at least one proposal submitted, approved by threshold, and executed; the deployment must be reproducible and evidence must be provided. | MISSING | — | Same: redeploy + full 2-of-3 lifecycle on v0.2.0 testnet needed, evidence re-captured. | — |
| F8 | Full documentation and a clean public repository are delivered. | PARTIAL | — | Repo public + MIT; docs exist but stale (v0.1.2 evidence, no CU costs, no consolidated write-up). | — |

## Usability

| ID | Criterion (verbatim) | Status | How verified (command) | Evidence | Last verified |
|----|---------------------|--------|------------------------|----------|---------------|
| U1 | Provide a module/SDK that can be used to build Logos modules for interacting with the program. | PARTIAL | `cargo build -p multisig-sdk` | `sdk/` exists; must verify it builds on v0.2.0 and is adequate for module builders. | — |
| U2 | Provide a Logos Basecamp app GUI with local build instructions, downloadable assets, and loadable in Logos app (Basecamp). | PARTIAL | — | `basecamp-app/` (html+module.json) exists; weboko requires assets hosted as separate downloadables (e.g. GitHub release). Not verified loadable this session. | — |
| U3 | Provide an IDL for the LEZ program, using the SPEL framework. | CLAIMED | inspect `lp-0002-private-multisig.idl.json` | IDL exists (updated for v2 signatures, commit 1bc5d39); not re-validated this session. | — |

## Reliability

| ID | Criterion (verbatim) | Status | How verified (command) | Evidence | Last verified |
|----|---------------------|--------|------------------------|----------|---------------|
| R1 | The system handles proof generation failures gracefully and surfaces a clear error to the member. | CLAIMED | force a prove failure, observe error | Error paths exist in host CLI; not exercised this session. | — |
| R2 | A partial set of approvals (fewer than M) is preserved and resumable across client restarts. | PARTIAL | kill client+sequencer mid-flow, restart, resume | By construction approvals live on-chain (survive client restarts), but no demonstration script exists. weboko reef #2. | — |
| R3 | The verifier program returns deterministic, documented error codes for all invalid-proof and double-vote scenarios. | CLAIMED | trigger 6001–6012 in integration tests | Codes documented in README; tests exist (`programs/multisig/tests`); not re-run this session. | — |

## Performance

| ID | Criterion (verbatim) | Status | How verified | Evidence | Last verified |
|----|---------------------|--------|--------------|----------|---------------|
| P1 | Document the compute unit (CU) cost of each on-chain operation on LEZ devnet/testnet. | MISSING | — | Not measured. weboko reef #1. jeefxM reported RISC0 cycle counts (262,144 approve inner; 1,048,576 outer) — measure equivalents for our ops. | — |

## Supportability

| ID | Criterion (verbatim) | Status | How verified | Evidence | Last verified |
|----|---------------------|--------|--------------|----------|---------------|
| S1 | The program is deployed and tested on LEZ devnet/testnet. | MISSING | — | v0.1.2 deploy wiped; redeploy on v0.2.0. | — |
| S2 | End-to-end integration tests run against a LEZ sequencer (standalone mode) and are included in CI. | PARTIAL | `.github/workflows/ci.yml` | CI boots a sequencer (v0.1.2 pin); must re-verify against v0.2.0 after the port. | — |
| S3 | CI must be green on the default branch. | VERIFIED* | `gh run list --repo retraca/lp-0002-private-multisig --limit 1` | success on main (2026-06-15 HEAD 3935a88) — but on the OLD v0.1.2 code; must stay green through the port. | 2026-07-03 |
| S4 | A README documents end-to-end usage: deployment steps, program addresses, and step-by-step instructions for interacting with the program via CLI and Basecamp app. | PARTIAL | read README | CLI walkthrough present; program addresses stale (wiped chain); Basecamp walkthrough thin (weboko reef #4). | — |
| S5 | A reproducible end-to-end demo script is provided and works against a real local sequencer with RISC0_DEV_MODE=0. | CLAIMED | fresh clone → `./demo.sh` | demo.sh exists; must re-verify on v0.2.0 local standalone sequencer, real proofs, exit 0. | — |
| S6 | A recorded video demo of the end-to-end flow is included in the submission; the recording must show terminal output (including proof generation) to confirm RISC0_DEV_MODE=0 was active. | MISSING | — | Prior videos untracked/local-only; PR #87 was closed for exactly this. Must record silent cut + narration script; voice-over is BUILDER-ONLY. | — |

## Submission Requirements

| ID | Requirement (verbatim, condensed) | Status | How verified | Evidence | Last verified |
|----|----------------------------------|--------|--------------|----------|---------------|
| SR1 | Public repository with all circuit code, LEZ program code, and client-side tooling under MIT or Apache-2.0. | VERIFIED | `gh repo view retraca/lp-0002-private-multisig --json visibility,licenseInfo` | PUBLIC, MIT. | 2026-07-03 |
| SR2 | Verifier program deployed on LEZ testnet with a verified program ID. | MISSING | — | Chain wiped; redeploy on v0.2.0. | — |
| SR3 | End-to-end demo video with builder narration (silent screencast not sufficient). | MISSING→BUILDER-ONLY | — | Silent cut + narration script = this loop; voice = builder. | — |
| SR4 | Reproducible deployment steps and evidence for ≥1 multisig instance on testnet with ≥1 proposal submitted, approved by threshold, executed. | MISSING | — | Redo on v0.2.0, capture tx hashes. | — |
| SR5 | Write-up: threshold proof scheme, nullifier design, LEZ account model compatibility (nonce and program_owner), security assumptions, known limitations, integration instructions. | PARTIAL | read docs | SECURITY.md + README cover most; consolidate into one write-up incl. program_owner handling. | — |
| SR6 | Proof generation time and on-chain verification gas cost benchmarks. | MISSING | — | Measure with P1. | — |

## Borrowed intel (recon notes)

- Testnet = LEZ v0.2.0 tag (commit a58fbce2), announced ~2026-07-01; genesis keys
  known from `lez/testnet_initial_state/src/lib.rs` (LP-0008 work, wallet home
  `~/tn-v020` on build VM, check-health ✅ 2026-07-02).
- jeefxM #97 (rc5): in-circuit binding = rider account asserted in-guest
  (`account_id == for_regular_private_account(npk(secret), VOTE_IDENTIFIER)` +
  non-default). Our equivalent: voter_note must BE the member's live shielded
  voting account, account-id-derivation asserted in-guest; LEZ privacy circuit
  then proves live commitment-tree membership (stronger than derivation-only).
- jeefxM resume demo: `approval_count` survives sequencer kill -9 + restart on
  same RocksDB, then completes. Ours: same shape (state is on-chain).
- Tranquil-Flow #92: CU reported as explicit "CU counters unavailable" rationale
  + payload metrics; their #68 was rejected partly for localnet-only evidence.

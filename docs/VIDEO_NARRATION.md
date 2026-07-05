# LP-0002 Demo Video — Narration Script

One narration block per on-screen step. The recording is the real `./demo.sh`
run (RISC0_DEV_MODE=0, local standalone LEZ v0.2.0 sequencer) plus a testnet
segment. Speak each block as its step banner appears; the video's idle gaps
are compressed, so keep the pace natural and let the output breathe.

---

**Intro (title card / step 1 banner)**

Hi, I'm Gonçalo. This is my submission for Lambda Prize LP-0002 — a private
M-of-N multisig for the Logos Execution Zone. Members hold shielded accounts,
approvals leave no trace of who voted, and the chain records only that a
threshold was met. Everything you'll see is real: a real local LEZ v0.2.0
sequencer, and real RISC0 proofs — RISC0_DEV_MODE is zero, and you'll see the
prover output on screen.

**Step 1-2 — build**

The demo script is the same one in the repo root that evaluators run from a
clean clone. It builds the LEZ v0.2.0 standalone sequencer and wallet from
the upstream tag, then my program — a single RISC0 guest with four
instructions — plus the client CLI.

**Step 3 — boot the sequencer**

A throwaway local sequencer boots on a fresh data directory with
RISC0_DEV_MODE=0 — it verifies real STARKs. One-second blocks.

**Step 4 — genesis funder**

The wallet imports the genesis account that LEZ v0.2.0 bakes into its initial
state. It funds the members' voting accounts in a minute.

**Step 5 — member identities**

Three members derive voting identities. The key detail: each member's secret
is their shielded account's nullifier secret key, HD-derived exactly the way
a LEZ wallet derives it. Controlling the secret is controlling the shielded
account — that's what binds membership to real shielded accounts.

**Step 6 — deploy, initialize, propose**

The program deploys, and the multisig initializes as 2-of-3. What goes
on-chain is only the threshold and three one-way commitments — a hash of each
member's secret and the multisig id. Then a proposal: a parameter change,
"set fee_bps = 25", recorded on the program-owned account. No signatures
needed from here on — the account belongs to the program.

**Step 7 — fund the voting accounts**

Each voting member needs their shielded voting account live on chain — this
is the in-circuit live-account binding. Two shielded transfers, each a real
proof, make member 0 and member 1's accounts live.

**Step 8a — the first anonymous vote**

Member 0 votes. Watch what's happening: the vote is a privacy-preserving
transaction proved locally. The secret key and the member index are private
inputs — they never leave this machine. In-circuit, the program checks the
membership commitment, checks the vote rides the member's live shielded
account, and derives a nullifier bound to this proposal. This is a real
STARK — here's the prover running. [pause while cycles print]
The vote lands: count is one. The chain shows a nullifier — not who voted.

**Step 8b — kill the sequencer**

Reliability check: kill dash nine the sequencer mid-flow, restart it on the
same data directory. The partial approval — one of two — survived. Approvals
live on-chain, so members can stop and resume across any client or sequencer
restart.

**Step 8c — the second vote, threshold**

Member 1 votes the same way. Count reaches two — that's the threshold.

**Step 8d — double vote rejected**

Now member 0 tries to vote again. Same member, same proposal — the program
derives the same nullifier and aborts inside the circuit with error 6004.
The proof cannot even be generated: an invalid vote never reaches the chain.

**Step 9 — execute**

With the threshold met, execute finalizes the proposal. The final state:
votes two, executed true, two spent nullifiers, and the action bytes. At no
point did the chain record which members approved — that's the whole point.

**Testnet segment**

The same program is deployed on the hosted LEZ testnet — here's the program
id, the multisig instance, and the full lifecycle: proposal, two anonymous
approvals with distinct nullifiers, and the execution, all re-queryable on
testnet.lez.logos.co. Transaction hashes are in the repo's testnet evidence
doc.

**Close (criteria checklist card)**

To recap against the prize criteria: anonymous approvals from shielded
accounts, threshold verification without recording voters, nullifier
double-vote prevention, unlinkable execution, client-side proving, a
reproducible demo against a real sequencer at RISC0_DEV_MODE=0, resumable
partial approvals, documented error codes, and the testnet deployment with
evidence. The repo has the write-up, SDK, IDL, Basecamp app, and benchmarks.
Thanks for reviewing.

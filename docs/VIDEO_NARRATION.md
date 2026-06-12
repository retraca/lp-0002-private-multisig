# LP-0002 demo video — narration script

Read this over the rendered terminal video (`lp0002-demo.mp4`). Timings are
approximate; the video is the real run with the long proving sections
compressed, so pause/resume as needed. Speak in your own words — this is a
guide, not a script to read robotically.

The prize requires *narration* ("a silent screencast is not sufficient"), so
the voice track is what turns this recording into a valid submission.

---

## 0:00 — Opening (over the title banner)

"This is LP-0002, a private M-of-N multisig for the Logos Execution Zone.
The problem with a normal on-chain multisig is that it leaks everything: who
the members are, and exactly who approved each action. That's a surveillance
surface. What I built lets members vote privately — the chain learns that a
threshold of approvals was reached, but never which members voted.

Everything you're about to see runs against the live Logos testnet with real
zero-knowledge proofs. Notice RISC0_DEV_MODE is off — these are real proofs,
not mocks."

## ~0:30 — Step 1, keygen

"First I generate the multisig account's key. On Logos, claiming a fresh
account requires a signature from that account's own key, so this is a
one-time bootstrap credential. After the account is created, it's owned by
the program and this key is never needed again."

## ~0:45 — Step 2, commitments

"Each of the three members derives a private commitment from their secret key
— their nsk. This is the only step that touches the secret, and it runs
locally. The nsk never goes on-chain and is never sent anywhere. Only the
commitment — a hash — is shared."

## ~1:00 — Step 3, initialize

"Now I initialize the 2-of-3 multisig on-chain. This registers the threshold,
the three member commitments, and the ID of the vote-circuit program that's
allowed to deliver votes — I'll come back to why that matters. You can see
the transaction confirm on the explorer, and the account state now shows it's
owned by the multisig program."

## ~1:30 — Step 4, proposal

"Anyone can submit a proposal — here, 'transfer 100'. Only reaching the
threshold of approvals gates execution, so submitting a proposal needs no
special permission."

## ~1:50 — Step 5, the first vote (the key moment)

"This is the heart of it. When a member votes, their client runs a small
program called the vote-circuit and proves it locally. The member's secret
key goes in as a *private* input — in the Logos privacy model, that input
never appears on-chain. The program recomputes the member's commitment,
checks it's in the registered set, derives a one-time nullifier, and then
chains a call into the multisig program to record the vote.

The reason this works: Logos public transactions can't carry proofs, so the
multisig program can't verify a receipt directly. Instead it trusts its
caller — and the privacy pipeline cryptographically proves that the caller
really was the vote-circuit program. So checking the caller is equivalent to
verifying the membership proof.

You're watching a real proof generate right now — this is the part that takes
a few minutes. When it lands, the vote count goes to one, and the chain has
no idea which of the three members it was."

## ~ (after vote 1 lands) — Step 6, second vote

"Member one votes the same way. Another real proof, another private vote. Now
the count is two — the threshold is met. And critically, each vote also
creates a throwaway private note, so on-chain a vote looks identical to any
other private transfer. An observer can't even tell these transactions are
votes."

## ~ — Step 7, execute

"With the threshold met, anyone can execute the proposal. The state flips to
executed, with two spent nullifiers recorded — which also means no member can
vote twice. And that's the full lifecycle: created, proposed, approved by
two of three members privately, and executed — all with real proofs on the
live testnet, and the chain never learned who voted."

## Closing

"The full code, the on-chain transaction hashes for everything you just saw,
and a one-command reproduction are in the repository. Thanks for watching."

---

## After recording your voice

Hand me the voice track (or the final muxed file) and I'll help check it
covers the criteria, then we add the video link to the solution doc and
reopen the PR.

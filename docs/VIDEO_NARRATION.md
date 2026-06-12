# LP-0002 demo video — narration script

Read this over the silent terminal recording (`lp0002-demo.mp4`). The terminal
shows only commands and results — all the explaining is your voice. Each
section below is keyed to what's on screen at that moment, so you narrate what
the viewer is actually seeing. Speak in your own words; this is a guide.

The prize requires *your* narration ("a silent screencast is not sufficient"),
so the voice track is what makes this a valid submission.

---

## Opening — while the header line is on screen

"This is LP-0002, a private multisig for the Logos Execution Zone. A normal
on-chain multisig leaks who the members are and exactly who approved each
action. This one keeps votes private: the chain learns a threshold was
reached, never which members voted. Everything here is the live testnet with
real zero-knowledge proofs — dev mode is off."

## "create the multisig account" — over `chain keygen`

"First I create the multisig's account key. On Logos, claiming a fresh account
needs a signature from its own key, so this is a one-time bootstrap — after
the account exists, the program owns it and the key isn't needed again."

## "each member derives a commitment" — over the three derive-commitment lines

"Each of the three members turns their secret key into a public commitment.
This is the only step that touches a secret, and it's local — the secret key
never goes on chain and is never sent anywhere. Only these commitments are
shared."

## "create the 2-of-3 multisig on-chain" — over `chain initialize` + state line

"Now I initialize the two-of-three multisig: the threshold, the three
commitments, and the program allowed to deliver votes. It confirms, and the
decoded state shows threshold two, zero votes so far."

## "anyone can propose" — over `submit-proposal` + state line

"Anyone can submit a proposal — here, transfer one hundred. Submitting is
open; only reaching the threshold of approvals will let it execute."

## "member 0 votes" — over the vote command and the "Proving…" lines

"Now a member votes. Their client runs a small vote-circuit program and proves
it locally. The secret key goes in as a private input that never appears on
chain. This 'Proving' line is a real zero-knowledge proof generating right
now — it takes a few minutes. When it lands, the count goes to one, and the
chain has no idea which of the three members voted."

## "member 1 votes" — over the second vote + state line

"A second member votes the same way — another real proof. The count reaches
two, so the threshold is met. Each vote also creates a throwaway private note,
so on chain a vote is indistinguishable from any ordinary private transfer."

## "threshold met — execute" — over `execute` + final state line

"With two approvals in, anyone can execute. The state flips to executed, with
two spent nullifiers — which is also what stops any member from voting twice."

## Closing — over the final two `#` lines

"That's the whole lifecycle: created, proposed, approved by two of three
members privately, and executed — real proofs, live testnet, and the chain
never learned who voted. The code, every transaction hash, and a one-command
reproduction are in the repository. Thanks for watching."

---

## Recording your voice

Simplest path on macOS: open `lp0002-demo.mp4`, start a QuickTime screen
recording (capture the playing video + your mic), and talk through the above.
The two proof steps hold for a while on screen, giving you room to explain.
Send me the result and I'll attach it to the submission and reopen the PR.

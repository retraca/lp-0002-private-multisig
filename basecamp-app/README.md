# Private Multisig — Basecamp App

A Logos Basecamp mini-app GUI for the LP-0002 private M-of-N multisig.

## What it does

- **Live multisig state** — reads the multisig account from any LEZ sequencer
  (`getAccount` JSON-RPC) and decodes the Borsh state client-side: threshold,
  member count, and per-proposal votes / executed flag / nullifier count. The
  page shows *that* a threshold was met, never *who* voted — same privacy
  boundary as the chain itself.
- **Derive commitment** — enter your NSK and the multisig ID, get back
  `SHA256("member" || nsk || multisig_id)` computed with the Web Crypto API.
  The NSK input is type `password` and never leaves the page.
- **Vote command builder** — assembles the exact `multisig chain vote` CLI
  invocation for your proposal/member. Vote proofs are real STARKs generated
  by the CLI (RISC0_DEV_MODE=0); a browser cannot produce them.

## Local build instructions

There is no build step — the app is a single self-contained `index.html`
(vanilla JS + Web Crypto).

## Load in Logos app (Basecamp)

1. Download `basecamp-app.zip` from this repository's GitHub release assets
   (or use this directory directly from a clone) and unzip it.
2. In the Logos desktop app, open the Basecamp module.
3. Load the app directory as a local app (`module.json` describes it;
   `index.html` is the entry point).

It also runs in any browser: `open index.html` (state reads work against any
sequencer URL you enter, e.g. `https://testnet.lez.logos.co` or a local
`http://127.0.0.1:3040`).

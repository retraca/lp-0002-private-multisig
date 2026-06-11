# Private Multisig — Basecamp App

A Logos Basecamp mini-app for the LP-0002 private M-of-N multisig.

## Load in Logos app (Basecamp)

1. In the Logos desktop app, open the Basecamp module.
2. Click **Load local app** and point it at this directory.
3. The app loads `index.html` directly from the filesystem — no build step.

## What it does

**Derive commitment** — enter your NSK and the multisig ID, get back your `member_commitment = SHA256("member" || nsk || multisig_id)`. Share the commitment with the multisig creator. The NSK never leaves the page.

**Vote** — the page shows the CLI command to generate a vote receipt offline. Submit the receipt bytes to the on-chain `vote` instruction via the chain interface.

## Security note

This app computes commitments client-side in the browser using the Web Crypto API. The NSK input is type `password` and is never stored or transmitted.

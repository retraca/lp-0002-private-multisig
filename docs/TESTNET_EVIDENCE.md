# LP-0002 testnet deployment evidence

Date: 2026-06-12. Sequencer: `https://testnet.lez.logos.co` (hosted LEZ testnet). Explorer: `https://explorer.testnet.lez.logos.co`.

All transactions below are verifiable with:

```bash
curl -s -X POST https://testnet.lez.logos.co -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","method":"getTransaction","params":["<tx_hash>"],"id":1}'
```

and the account state with:

```bash
multisig chain state --sequencer https://testnet.lez.logos.co --multisig-id <hex>
```

## 1. Program deployments

| Program | Program ID | Deployment tx |
|---|---|---|
| `programs/multisig/private_multisig.bin` | `fb2d6afe695b3d03736f6a7f869d980884afc61f24d5199194f0891555a8a8e3` | `43703d962099e5ed7d6467e22fa11d60c2b67634c91cafe1388d639bd91ffc92` |
| `programs/vote_circuit/vote_circuit.bin` | `7af8104a46999ed81962d5eb0dc4482db84a1352bacc95e86210fe1a46f87063` | `4924d2b9a6bd6c3b776b750344cb0d9bbdd7ddf972e7f73c56e011d2cd96f9f8` |

## 2. Multisig instance (2-of-3): full lifecycle with REAL proofs

Run at `RISC0_DEV_MODE=0` end to end. Demo member nsks `0x11…`, `0x22…`, `0x33…`.

| Step | Value |
|---|---|
| Multisig account | `91e43105fd0fdf07b64d0dfd975063dca813bda832411692cd8227884147536a` |
| Initialize tx (signed) | `b4c679341fd1b3298b48d1f3e07284d7547238fe26c8bebe71b7ee8bb8e35d6c` |
| Proposal tx (`aaaa…01`, "transfer 100") | `3383acef3b9cf01d580d67d5259159388590dda6421e5aac9213668c5bd79f40` |
| Vote 1 (privacy-preserving tx, member 0) | `1050f4f2efc76de225bdb8ab6b24958076a16e8cfbda68697fbf559f18598622` |
| Vote 2 (privacy-preserving tx, member 1) | `74374c0125ca1613c025331bb5e046e6678bb73ca1a551a3868015a2f85c000f` |
| Execute tx | `8f3c5b4581b4880243bc1bd3ac6a15ce6b8c395ec32595e9118470a69097b6f2` |

Final state read back from the testnet decodes as `MultisigState { threshold: 2, vote_circuit_program_id: 7af8104a…, member_commitments: [3], proposals: [Proposal { id: aaaa…01, action: "transfer 100", vote_count: 2, executed: true, spent_nullifiers: [2] }] }`.

## 3. How votes work on-chain (chained-call composition)

LEZ public transactions carry no RISC0 receipts, so a program cannot resolve an
`env::verify` assumption in public execution (`sys_verify_integrity: no receipt
found`, reproduced against a local standalone sequencer). Votes therefore travel
through the **privacy-preserving execution (PPE) pipeline**:

1. The voter runs `multisig chain vote`. The CLI executes and proves the
   **vote-circuit program** locally: the nsk is initial-call instruction data,
   which never appears on-chain (the PPE output exposes only public pre/post
   states, commitments, and nullifiers).
2. The vote-circuit program recomputes
   `member_commitment = SHA256("member" || nsk || multisig_id)` against the
   registered set in live multisig state, derives the per-proposal nullifier
   and member set root, and declares a `ChainedCall` into the multisig
   program's `vote` instruction.
3. The PPE outer circuit proves both program executions and their linkage
   (caller_program_id cannot be spoofed). The multisig program accepts votes
   only from the vote-circuit program registered at `initialize`.
4. The sequencer verifies ONE composite succinct proof and applies the public
   state diff. A vote transaction additionally creates a fresh zero-balance
   private "voter note", giving it the same on-chain shape as any private
   transfer.

Negative paths verified against a local standalone sequencer (failures surface
client-side during proving, before any transaction is sent):

- Re-vote after execution → `Program error 6006: proposal already executed`
  (observed live; the nullifier-spent path `6004` on an open proposal is
  covered by the state-machine unit tests).
- Non-member nsk → `Program error 6005: nsk does not match registered
  commitment` (observed live).
- Vote submitted as a plain public transaction → rejected: caller check fails
  (`6012 ERR_UNAUTHORIZED_CALLER`), because top-level callers have the zeroed
  caller program ID.

## 4. Authorization model

`#[account(init)]` account claiming requires the transaction to be authorized
by the account's key: an unsigned initialize fails with
`InvalidProgramBehavior(ClaimedUnauthorizedAccount)`. The CLI flow:

1. `multisig chain keygen` generates a fresh schnorr (BIP340) key; the multisig
   account ID is `SHA256("/LEE/v0.3/AccountId/Public/" || pubkey)`.
2. `multisig chain initialize --signing-key <hex> …` fetches the nonce, signs,
   and submits. The key is a one-time bootstrap credential; after claiming, the
   account belongs to the program. Proposals, votes, and execution are
   unsigned.

## 5. Performance

| Operation | Cost |
|---|---|
| `initialize` / `submit_proposal` / `execute` (public tx) | ~4-10 ms zkVM executor time on the sequencer (well under the 32M-cycle public execution budget) |
| `chain vote` client-side proving (`RISC0_DEV_MODE=0`, Apple M2) | ~8-12 minutes (vote-circuit proof + multisig proof + PPE outer succinct proof) |
| Vote verification on the sequencer | one succinct receipt verification (same cost as any privacy-preserving transaction) |

## 6. Superseded v1 evidence

An earlier program version (`9abec04f2a082b6bf70f5a38f2dc967cc7605b3159a6713d93e62f76b0a55725`,
deploy tx `82de65cf312a272e3a2a81929d2fb0042b09c5b7ff9d1d225eb6682dcb235005`)
used `env::verify` for votes and could not accept them via public transactions.
Its instance (`6c0238c2…b424`, initialize tx `419ddedd…452c`, proposal tx
`38faa9a3…023a`) remains on the testnet as historical evidence of the
deployment path. The v2 architecture above supersedes it.

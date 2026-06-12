# LP-0002 testnet deployment evidence

Date: 2026-06-12. Sequencer: `https://testnet.lez.logos.co` (hosted LEZ testnet, ~block 50316 at the time of this run).

All transactions below are verifiable with:

```bash
curl -s -X POST https://testnet.lez.logos.co -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","method":"getTransaction","params":["<tx_hash>"],"id":1}'
```

and the account state with:

```bash
multisig chain state --sequencer https://testnet.lez.logos.co --multisig-id <hex>
```

## 1. Program deployment

| Field | Value |
|---|---|
| Program binary | `programs/multisig/private_multisig.bin` (R0BF) |
| Program ID | `9abec04f2a082b6bf70f5a38f2dc967cc7605b3159a6713d93e62f76b0a55725` |
| Deployment tx | `82de65cf312a272e3a2a81929d2fb0042b09c5b7ff9d1d225eb6682dcb235005` |

`getTransaction` for the deployment hash returns the full `ProgramDeployment` transaction including the R0BF bytecode.

## 2. Multisig instance (2-of-3)

| Field | Value |
|---|---|
| Multisig account | `6c0238c2e32736760b8db1502767ac8a19fbff331377ccc754180dafdd66b424` (`8GcznRfTHvJDyamTE1ZPx3wTS2TbwC119Ubyd2UXtNAo`) |
| Initialize tx | `419ddedd049594817b9e9367d62aee487d020bb14f92e4f1ff24f7bebce9452c` |
| Threshold | 2 |
| Member commitments | `f277db82…292380`, `86a5efc0…0fe667`, `651da2dc…e858e8` (derived from demo nsks `0x11…`, `0x22…`, `0x33…`) |

Account state after inclusion (read back from the testnet):

```
program_owner: 9abec04f2a082b6bf70f5a38f2dc967cc7605b3159a6713d93e62f76b0a55725
data (105 bytes): 0203000000 f277db82…292380 86a5efc0…0fe667 651da2dc…e858e8 00000000
```

Borsh-decoded: `MultisigState { threshold: 2, member_commitments: [3 entries], proposals: [] }`. The account is claimed by (owned by) the multisig program.

## 3. Proposal submission

| Field | Value |
|---|---|
| Proposal ID | `aaaa000000000000000000000000000000000000000000000000000000000001` |
| Action bytes | `7472616e7366657220313030` ("transfer 100") |
| Submit tx | `38faa9a3e103982b41aec911e6fb551dacb83c0d4818ac71c71ec1829f0c023a` |

Account state after inclusion shows `proposals: [Proposal { id: aaaa…01, action: "transfer 100", vote_count: 0, executed: false, spent_nullifiers: [] }]` (159 bytes of state data).

This transaction was **unsigned** (empty witness set): once the account is program-owned, `#[account(mut)]` instructions need no wallet signature, which is what allows later votes to be submitted without linking a member's wallet.

## Authorization model (found while producing this evidence)

`#[account(init)]` account claiming requires the transaction to be **authorized by the account's key**: an unsigned initialize fails with `InvalidProgramBehavior(ClaimedUnauthorizedAccount)`. The CLI therefore:

1. `multisig chain keygen` generates a fresh schnorr (BIP340) key; the multisig account ID is `SHA256("/LEE/v0.3/AccountId/Public/" || pubkey)`.
2. `multisig chain initialize --signing-key <hex> …` fetches the account nonce, signs the message, and submits. After this single signed transaction the account belongs to the program and the key is never needed again.

This key is a one-time bootstrap credential for the account, not a member identity: member privacy is unaffected.

## Known limitation: on-chain vote submission

The `vote` instruction verifies the member's RISC0 receipt with `env::verify(IMAGE_ID, journal)`, which resolves the proof as a zkVM **assumption**. LEZ public transactions carry no receipts (`PublicTransaction = Message + WitnessSet`, no proof field), and the sequencer's public-execution path adds no assumptions to the executor environment (`nssa/src/program.rs::execute`). Submitting a vote receipt today fails deterministically with:

```
ProgramExecutionFailed("sys_verify_integrity: no receipt found to resolve assumption: …")
```

(reproduced against a local standalone sequencer, sequencer log available).

The LEZ-native resolution is the **privacy-preserving transaction path**: the client executes and proves the program call locally, adding the vote receipt as an assumption (`nssa/src/privacy_preserving_transaction/circuit.rs` line 113 supports exactly this), and submits one composite proof that the sequencer verifies. Wiring the vote submission through that path is the remaining work item for full on-chain threshold execution; `initialize`, `submit_proposal`, and `execute` are unaffected.

Cycle-budget note: verifying a Groth16 receipt in-guest (no assumptions) does not fit the 32M-cycle public execution budget (`MAX_NUM_CYCLES_PUBLIC_EXECUTION`), which rules out the simpler alternative.

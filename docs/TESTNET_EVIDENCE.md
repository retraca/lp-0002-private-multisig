# LP-0002 Testnet Deployment Evidence

Date: **2026-07-05** (22:20–22:42 UTC). Sequencer: `https://testnet.lez.logos.co`
(hosted LEZ testnet, pinned to **LEZ v0.2.0**, tag commit `a58fbce2`). All
proofs real: `RISC0_DEV_MODE=0`.

> The 2026-06-12 v0.1.2 evidence that used to live in this file is
> **superseded**: the hosted testnet was since wiped and redeployed on
> LEZ v0.2.0, and the program was ported and redeployed (new program id).

## Program

| | |
|---|---|
| Program id | `6ce658db7384e1f6a90528715404856d55f2e977667d0a5989c8ed633d55afad` |
| Deploy tx | `f37ac02deaabf5470450cc1f7f2643bfb5fc7d8de8f2783959b420a515959837` |
| Source | `programs/multisig` @ branch HEAD (built with `cargo +risc0 build --release --target riscv32im-risc0-zkvm-elf`) |

The program id is the RISC0 image id — recompute it from source with
`multisig chain program-id --program-bin <elf>` and compare.

## Multisig instance (2-of-3)

| | |
|---|---|
| Multisig account | hex `9bcd3516c505db20d37051192c8a02a4b999a474754f0da42bfa6c247860f72c` = base58 `BVBgRAv7v2z9U13gxeKfEGogjjiAEnAhfVNabv6aUPQw` |
| Initialize tx | `bb18ee5b1adb9f83f9ff3f875fc780479a7e62523d5b99710d5262727cb684a5` |
| Threshold | 2 of 3 |

## Members (demo identities — throwaway seeds, published for reproducibility)

| Member | Voting account (live shielded rider) | Funding tx |
|---|---|---|
| 0 | `Private/Aova5zFCCeEwW7nskg5gD74STSwx2dmWp1poChdDHGft` | `a36411765b15998004b8e7e1a6f295810dc3e8d34ee0d8b6d66de162bb3918fc` |
| 1 | `Private/AdyUtqkL7u2cDgtUx7N3ewrGkUrCqRz8zq4X8w9w53CL` | `dcf59b373819b51509a60f288e180bcf1a38e26f325fcba25c0217e765460e2b` |
| 2 | `Private/FU7EH6EWRA175AsNEMW7bMgti5R8yvRDDsKnTVwAcdhW` | (never votes — threshold reached without them) |

Funder: genesis public account `6iArKUXxhUJqS7kCaPNhwMWt3ro71PDyBj7jwAyE2VQV`
(baked into LEZ v0.2.0 `testnet_initial_state`).

## Proposal lifecycle (all on-chain, re-queryable)

| Step | Tx hash |
|---|---|
| Submit proposal `7a11e7c47bd80355e12a7c904fb61833cc056e2188da471902f35ba06de40001` (action `set fee_bps = 25`) | `203edcaff500ef7f683263c818822a23a59d3c91e095e32d0534cedba5069c4a` |
| Anonymous vote 1 (real STARK, privacy-preserving tx) | `b3a437b76f0573e00be7a772eecf5b8aa6480fabd083fb932478b489ae4fa8e7` |
| Anonymous vote 2 (real STARK, privacy-preserving tx) | `4e0322d153838641f479f593650d44f3b30c1b3b3946365199b1fea9663254ca` |
| Double-vote attempt by voter of vote 1 | **rejected in-circuit**: `Guest panicked: ERR_6004 nullifier already spent (double vote)` — the proof cannot be generated, no tx exists |
| Execute (threshold 2 reached) | `cc8b134a0e1431b3f4c6ef8bbc442db90acd6825fe3e841846609c1f255ce874` |

## Verify it yourself

Final on-chain state (any machine, no tooling beyond curl + python):

```bash
curl -s -X POST https://testnet.lez.logos.co -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"getAccount","params":["BVBgRAv7v2z9U13gxeKfEGogjjiAEnAhfVNabv6aUPQw"]}'
```

Decoded (as of 2026-07-05, via `sdk::decode_state` or the Basecamp app):

```
program_owner: 6ce658db7384e1f6a90528715404856d55f2e977667d0a5989c8ed633d55afad
threshold=2 members=3 proposals=1
proposal 7a11e7c47bd80355… votes=2 executed=True action="set fee_bps = 25"
nullifiers=[823c595053927ed2…, 4b7ae574da0494c4…]
```

Note what the state does NOT contain: any link between the two nullifiers and
the three member commitments or voting accounts. The chain records that a
threshold of 2 was met — not which members approved.

Each tx: `{"method":"getTransaction","params":["<hash>"]}` on the same endpoint.

## Reproduce the deployment

`scripts/testnet-run.sh` (the exact script that produced this evidence) drives
the full flow against the hosted testnet: build → deploy → keygen → initialize
→ proposal → import + fund voting accounts → two anonymous votes → double-vote
rejection → execute. Fresh ids each run: generate new member seeds
(`multisig member new`) and a new proposal id, and fund from any spendable
public account (`FUNDER_ID` env).

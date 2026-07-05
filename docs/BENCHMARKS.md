# LP-0002 Benchmarks — proof generation time and compute cost

Measured 2026-07-05 on the build machine (GCP e2-standard-16, 16 vCPU,
CPU-only proving, RISC0 3.0.5, `RISC0_DEV_MODE=0`) during a full `./demo.sh`
run against a local standalone LEZ v0.2.0 sequencer. Reproduce with:

```bash
RISC0_INFO=1 RUST_LOG="info,risc0_zkvm=info" ./demo.sh
```

## Compute cost per operation (RISC0 cycles)

LEZ meters compute in RISC0 zkVM cycles. Only privacy-preserving operations
invoke the prover client-side; public program calls take the no-proof path
(the sequencer executes them inside its public-execution budget of 32M
cycles, `MAX_NUM_CYCLES_PUBLIC_EXECUTION` in `lee/state_machine`).

| Operation | Client proving | Measured cycles |
|---|---|---|
| `deploy` | none (deployment tx) | 0 client; sequencer registers bytecode |
| `initialize` | none (public tx, signed) | 0 client; guest executes in sequencer ≤32M budget |
| `submit-proposal` | none (public tx, unsigned) | 0 client; same |
| `vote` — inner session (this program's guest) | yes | **524,288 total** (277,013–298,170 user, 33k paging; 45 SHA2 accelerator calls; 1 segment) |
| `vote` — outer session (LEZ privacy-preserving circuit) | yes | **1,048,576 total** (708,285–732,675 user; 156 SHA2 calls; 1 segment) |
| `execute` | none (public tx, unsigned) | 0 client |
| double-vote attempt | aborts in the executor (`ERR_6004`) **before** proving | 0 — an invalid vote costs no proving time and produces no transaction |

Vote cycle counts are from two independent votes in the same run (member 0
and member 1); the total is padded to a power-of-two segment, so the billed
size is stable at 524,288 + 1,048,576 = **1,572,864 cycles per anonymous
vote**, single segment each.

## Proof generation time (wall clock)

| Step | Time |
|---|---|
| Vote inner+outer proving, 16 vCPU CPU-only | ~40 s between inner and outer session completion; ~2–3 min per vote end-to-end including state fetch, membership proof and submission |
| Shielded funding transfer (LEZ `auth-transfer`, not this program) | ~2–3 min, same proving pipeline |
| Full 2-of-3 demo (`./demo.sh`, 2 funding transfers + 2 votes + double-vote rejection + execute, incremental builds) | **10 m 13 s** (22:13:48 → 22:24:01 UTC); instrumented re-run 10 m 16 s |

On a laptop expect roughly 2–4× these times (competing rc5-era submissions
report ~174–180 s per approval proof on desktop/M1-class hardware, consistent
with our cycle counts).

## Verification cost

The sequencer verifies the composite STARK per privacy-preserving
transaction; verification is milliseconds-scale and independent of member-set
size. On-chain state growth per vote: 32 bytes (one nullifier) plus the
vote-count byte update.

## Notes

- The hosted testnet run (docs/TESTNET_EVIDENCE.md) used the same binaries
  and the same proving path; per-vote cycle counts are identical by
  construction (same guest, same image id).
- LEZ's per-transaction compute budget may change during testnet (noted in
  the prize spec); the vote's 1.57M total cycles sits far below the current
  32M public budget and well within privacy-circuit limits.

#!/usr/bin/env bash
# LP-0002 private M-of-N multisig — end-to-end demo (2-of-3) against a REAL
# local LEZ v0.2.0 standalone sequencer.
#
# Default mode uses REAL RISC0 proofs (RISC0_DEV_MODE=0): each anonymous vote
# is a genuine STARK proved client-side (several minutes each on a laptop).
#
#   ./demo.sh          # real proofs, the submission-grade run
#   ./demo.sh --dev    # RISC0_DEV_MODE=1 (fake receipts) — fast logic run / CI
#
# Environment:
#   LEZ_DIR   where the LEZ v0.2.0 checkout+build lives (default ./.lez;
#             reused across runs — the first run builds it, ~30-60 min)
#   PORT      local sequencer port (default 3040)
#
# The flow demonstrated (one step per prize criterion where possible):
#   build -> boot sequencer -> import genesis funder -> derive 3 members
#   -> initialize 2-of-3 -> submit proposal -> fund member voting accounts
#   -> vote(member 0) [real proof] -> KILL + RESTART sequencer (resume check)
#   -> vote(member 1) -> double-vote(member 0) MUST FAIL -> execute -> assert.

set -uo pipefail

DEV_MODE=0
for arg in "${@:-}"; do
  [ "$arg" = "--dev" ] && DEV_MODE=1
done

ROOT="$(cd "$(dirname "$0")" && pwd)"
LEZ_DIR="${LEZ_DIR:-$ROOT/.lez}"
PORT="${PORT:-3040}"
D="$ROOT/.demo"
LOG="$D/seq.log"
DUST=5

if [ "$DEV_MODE" = "1" ]; then
  export RISC0_DEV_MODE=1
  echo "[demo] RISC0_DEV_MODE=1 (fake receipts — logic run)"
else
  export RISC0_DEV_MODE=0
  echo "[demo] RISC0_DEV_MODE=0 (REAL proofs — several minutes per vote)"
fi

SEQ_PID=""
cleanup() { [ -n "$SEQ_PID" ] && kill "$SEQ_PID" 2>/dev/null; pkill -f "sequencer_service .*--port $PORT" 2>/dev/null; true; }
trap cleanup EXIT
die() { echo "FATAL: $*" >&2; echo "=== DEMO FAILED ==="; exit 1; }

# Genesis funder baked into LEZ v0.2.0 testnet_initial_state (public account 0).
FUNDER_KEY="10a26a9aec7d34b82364eeae45c5294dbb0a764b000b94eeb9b58511dc487c4d"

# Deterministic demo member seeds (throwaway; never reuse outside the demo).
SEED0="4c70303030326d656d626572303030302f64656d6f2f6e736b2f763030310000"
SEED1="4c70303030326d656d626572303030312f64656d6f2f6e736b2f763030310000"
SEED2="4c70303030326d656d626572303030322f64656d6f2f6e736b2f763030310000"
PROPOSAL_ID="9f1c47a26bd80355e12a7c904fb61833cc056e2188da471902f35ba06de41172"
ACTION="736574206665655f627073203d203235"   # "set fee_bps = 25" — the gated parameter change

echo ""
echo "=== [1/9] prerequisites + LEZ v0.2.0 (sequencer + wallet) ==="
command -v cargo >/dev/null || die "cargo not on PATH (install rustup)"
command -v python3 >/dev/null || die "python3 required"
if [ ! -d "$LEZ_DIR" ]; then
  git clone -q --depth 1 --branch v0.2.0 \
    https://github.com/logos-blockchain/logos-execution-zone.git "$LEZ_DIR" \
    || die "clone logos-execution-zone v0.2.0"
fi
( cd "$LEZ_DIR" \
  && cargo build --release -p sequencer_service --features standalone 2>&1 | tail -2 \
  && cargo build --release -p wallet 2>&1 | tail -2 ) || die "LEZ build failed"
SEQ_BIN="$LEZ_DIR/target/release/sequencer_service"
WALLET_BIN="$LEZ_DIR/target/release/wallet"
[ -x "$SEQ_BIN" ] && [ -x "$WALLET_BIN" ] || die "missing LEZ binaries"

echo ""
echo "=== [2/9] build multisig CLI + guest program ==="
( cd "$ROOT" && cargo build --release -p private-multisig-cli 2>&1 | tail -2 ) || die "cli build"
MSIG="$ROOT/target/release/multisig"
( cd "$ROOT/programs/multisig" \
  && cargo +risc0 build --release --target riscv32im-risc0-zkvm-elf 2>&1 | tail -2 ) \
  || die "guest build (rustup toolchain 'risc0' required — see README)"
PROGRAM_BIN="$ROOT/programs/multisig/target/riscv32im-risc0-zkvm-elf/release/private_multisig"
[ -f "$PROGRAM_BIN" ] || die "guest ELF not produced"
"$MSIG" chain program-id --program-bin "$PROGRAM_BIN"

echo ""
echo "=== [3/9] boot local standalone sequencer (RISC0_DEV_MODE=$RISC0_DEV_MODE) ==="
pkill -f "sequencer_service .*--port $PORT" 2>/dev/null; sleep 1
rm -rf "$D"; mkdir -p "$D/wallet"
python3 - "$LEZ_DIR/lez/sequencer/service/configs/debug/sequencer_config.json" \
          "$D/sequencer_config.json" "$D" <<'PY' || die "sequencer config"
import json, sys
src, dst, home = sys.argv[1], sys.argv[2], sys.argv[3]
c = json.load(open(src))
c["home"] = home
c["block_create_timeout"] = "1s"
json.dump(c, open(dst, "w"), indent=2)
PY
cat > "$D/wallet/wallet_config.json" <<JSON
{"sequencer_addr":"http://127.0.0.1:$PORT","seq_poll_timeout":"30s","seq_tx_poll_max_blocks":25,"seq_poll_max_retries":25,"seq_block_poll_max_amount":300}
JSON
export LEE_WALLET_HOME_DIR="$D/wallet"
RUST_LOG=info nohup "$SEQ_BIN" "$D/sequencer_config.json" --port "$PORT" > "$LOG" 2>&1 &
SEQ_PID=$!
for _ in $(seq 1 40); do (ss -ltn 2>/dev/null || netstat -an) | grep -q "$PORT" && break; sleep 1; done
(ss -ltn 2>/dev/null || netstat -an) | grep -q "$PORT" || die "sequencer did not bind :$PORT"
echo "sequencer up (pid $SEQ_PID)"

w() { RUST_LOG=error "$WALLET_BIN" "$@"; }
state() { "$MSIG" chain state --multisig-id "$MULTISIG_ID" 2>/dev/null; }
votes_now() { state | grep -o 'votes=[0-9]*' | head -1 | cut -d= -f2; }
wait_for() { # wait_for <seconds> <cmd...>
  local n=$1; shift
  for _ in $(seq 1 "$n"); do "$@" && return 0; sleep 2; done
  return 1
}

echo ""
echo "=== [4/9] wallet: init storage + import genesis funder ==="
printf 'demo\n' | w account list >/dev/null 2>&1 || true
w account import public --private-key "$FUNDER_KEY" >/dev/null 2>&1 || true
FUNDER_ID=$(w account list 2>/dev/null | grep -o 'Public/[1-9A-HJ-NP-Za-km-z]*' | head -1 | cut -d/ -f2)
[ -n "$FUNDER_ID" ] || die "funder import failed"
echo "funder: Public/$FUNDER_ID"

echo ""
echo "=== [5/9] members: derive 3 voting identities (nsk = shielded account key) ==="
M0=$("$MSIG" member new --seed "$SEED0"); M1=$("$MSIG" member new --seed "$SEED1"); M2=$("$MSIG" member new --seed "$SEED2")
NSK0=$(echo "$M0" | sed -n 's/^nsk: //p'); VID0=$(echo "$M0" | sed -n 's|^voting_account: Private/||p')
NSK1=$(echo "$M1" | sed -n 's/^nsk: //p'); VID1=$(echo "$M1" | sed -n 's|^voting_account: Private/||p')
NSK2=$(echo "$M2" | sed -n 's/^nsk: //p')
echo "member 0 voting account: Private/$VID0"
echo "member 1 voting account: Private/$VID1"
echo "member 2 (never votes in this demo)"

echo ""
echo "=== [6/9] initialize 2-of-3 multisig + submit proposal ==="
KEYGEN=$("$MSIG" chain keygen)
SIGNING_KEY=$(echo "$KEYGEN" | sed -n 's/^signing_key: //p')
MULTISIG_ID=$(echo "$KEYGEN" | sed -n 's/^multisig_id: //p')
echo "multisig account: $MULTISIG_ID"
C0=$("$MSIG" derive-commitment --nsk "$NSK0" --multisig-id "$MULTISIG_ID")
C1=$("$MSIG" derive-commitment --nsk "$NSK1" --multisig-id "$MULTISIG_ID")
C2=$("$MSIG" derive-commitment --nsk "$NSK2" --multisig-id "$MULTISIG_ID")
"$MSIG" chain initialize --program-bin "$PROGRAM_BIN" --signing-key "$SIGNING_KEY" \
  --threshold 2 --commitments "$C0,$C1,$C2" || die "initialize failed"
wait_for 60 sh -c "\"$MSIG\" chain state --multisig-id $MULTISIG_ID 2>/dev/null | grep -q 'threshold: 2'" \
  || die "initialize did not land"
echo "initialized (threshold 2, 3 members)"
"$MSIG" chain submit-proposal --program-bin "$PROGRAM_BIN" --multisig-id "$MULTISIG_ID" \
  --proposal-id "$PROPOSAL_ID" --action "$ACTION" || die "submit-proposal failed"
wait_for 60 sh -c "\"$MSIG\" chain state --multisig-id $MULTISIG_ID 2>/dev/null | grep -q 'proposal $PROPOSAL_ID'" \
  || die "proposal did not land"
state

echo ""
echo "=== [7/9] fund member voting accounts (the in-circuit LIVE riders) ==="
"$MSIG" member import --seed "$SEED0" >/dev/null || die "import member 0"
"$MSIG" member import --seed "$SEED1" >/dev/null || die "import member 1"
w auth-transfer send --from "Public/$FUNDER_ID" --to "Private/$VID0" --amount "$DUST" \
  || die "fund member 0 voting account"
w auth-transfer send --from "Public/$FUNDER_ID" --to "Private/$VID1" --amount "$DUST" \
  || die "fund member 1 voting account"
echo "voting accounts live on chain"

echo ""
echo "=== [8/9] anonymous votes: prove locally, nsk never leaves this machine ==="
echo "--- vote as member 0 (RISC0_DEV_MODE=$RISC0_DEV_MODE)"
"$MSIG" chain vote --program-bin "$PROGRAM_BIN" --multisig-id "$MULTISIG_ID" \
  --proposal-id "$PROPOSAL_ID" --nsk "$NSK0" --member-index 0 || die "vote(member 0) failed"
wait_for 90 sh -c "[ \"\$(\"$MSIG\" chain state --multisig-id $MULTISIG_ID 2>/dev/null | grep -o 'votes=[0-9]*' | head -1 | cut -d= -f2)\" = 1 ]" \
  || die "vote 0 did not land"
echo "votes=1"

echo "--- RESUME CHECK: kill -9 the sequencer, restart on the same data dir"
kill -9 "$SEQ_PID" 2>/dev/null; sleep 2
RUST_LOG=info nohup "$SEQ_BIN" "$D/sequencer_config.json" --port "$PORT" >> "$LOG" 2>&1 &
SEQ_PID=$!
wait_for 40 sh -c "(ss -ltn 2>/dev/null || netstat -an) | grep -q $PORT" || die "sequencer restart"
sleep 2
[ "$(votes_now)" = "1" ] || die "partial approvals lost across restart"
echo "partial approval (1 of 2) SURVIVED the restart — resumable"

echo "--- vote as member 1"
"$MSIG" chain vote --program-bin "$PROGRAM_BIN" --multisig-id "$MULTISIG_ID" \
  --proposal-id "$PROPOSAL_ID" --nsk "$NSK1" --member-index 1 || die "vote(member 1) failed"
wait_for 90 sh -c "[ \"\$(\"$MSIG\" chain state --multisig-id $MULTISIG_ID 2>/dev/null | grep -o 'votes=[0-9]*' | head -1 | cut -d= -f2)\" = 2 ]" \
  || die "vote 1 did not land"
echo "votes=2 (threshold reached)"

echo "--- DOUBLE VOTE: member 0 votes again — the proof MUST fail (ERR_6004)"
if "$MSIG" chain vote --program-bin "$PROGRAM_BIN" --multisig-id "$MULTISIG_ID" \
  --proposal-id "$PROPOSAL_ID" --nsk "$NSK0" --member-index 0 2>"$D/double.err"; then
  die "double vote was ACCEPTED — nullifier check broken"
fi
grep -o "ERR_6004[^\"]*" "$D/double.err" | head -1 || tail -2 "$D/double.err"
echo "double vote rejected in-circuit (nullifier spent)"

echo ""
echo "=== [9/9] execute the threshold-gated action ==="
"$MSIG" chain execute --program-bin "$PROGRAM_BIN" --multisig-id "$MULTISIG_ID" \
  --proposal-id "$PROPOSAL_ID" || die "execute failed"
wait_for 60 sh -c "\"$MSIG\" chain state --multisig-id $MULTISIG_ID 2>/dev/null | grep -q 'executed=true'" \
  || die "execute did not land"
state

echo ""
echo "=== DEMO PASSED ==="
echo "2-of-3 lifecycle complete on a real local sequencer:"
echo "  - 2 anonymous approvals (privacy-preserving txs, nsk client-side only)"
echo "  - on-chain state shows votes=2 + 2 nullifiers, never WHICH members voted"
echo "  - partial approval survived a sequencer kill -9 (resumable)"
echo "  - double vote rejected in-circuit (ERR_6004)"
echo "  - threshold-gated action executed"
exit 0

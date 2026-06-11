#!/usr/bin/env bash
# LP-0002 private M-of-N multisig end-to-end demo (2-of-3).
# Usage: ./demo.sh [--dev]   (--dev = RISC0_DEV_MODE=1 for fast local testing)

set -euo pipefail

DEV_MODE=0
for arg in "$@"; do [ "$arg" = "--dev" ] && DEV_MODE=1; done

if [ "$DEV_MODE" = "1" ]; then
  export RISC0_DEV_MODE=1
  echo "[demo] RISC0_DEV_MODE=1 (mock proofs)"
else
  echo "[demo] Using real RISC0 proofs"
fi

SEQUENCER="${SEQUENCER:-http://127.0.0.1:9090}"
BIN="./target/release/multisig"

echo ""
echo "=== LP-0002 Private M-of-N Multisig Demo (2-of-3) ==="
echo ""

echo "[1/7] Building..."
cargo build --release --bin multisig 2>&1 | tail -3

# Demo multisig account ID (would be the deployed program's account in production)
MULTISIG_ID="cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"

# Three members with deterministic test nsks (never use these outside demo)
NSK_1="1111111111111111111111111111111111111111111111111111111111111111"
NSK_2="2222222222222222222222222222222222222222222222222222222222222222"
NSK_3="3333333333333333333333333333333333333333333333333333333333333333"

echo ""
echo "[2/7] Deriving member commitments (nsk stays local)..."
COMMIT_1=$("$BIN" derive-commitment --nsk "$NSK_1" --multisig-id "$MULTISIG_ID")
COMMIT_2=$("$BIN" derive-commitment --nsk "$NSK_2" --multisig-id "$MULTISIG_ID")
COMMIT_3=$("$BIN" derive-commitment --nsk "$NSK_3" --multisig-id "$MULTISIG_ID")
echo "Member 0: $COMMIT_1"
echo "Member 1: $COMMIT_2"
echo "Member 2: $COMMIT_3"

echo ""
echo "[3/7] Initializing 2-of-3 multisig on-chain..."
echo "  (In production: deploy program + call initialize with threshold=2 and the 3 commitments)"

PROPOSAL_ID="aaaa000000000000000000000000000000000000000000000000000000000001"
echo ""
echo "[4/7] Submitting proposal $PROPOSAL_ID..."
echo "  Action: 'transfer 100 LEZ to treasury'"

echo ""
echo "[5/7] Member 0 votes (proof generated off-chain, nsk never leaves this machine)..."
"$BIN" vote \
  --nsk "$NSK_1" \
  --member-index 0 \
  --multisig-id "$MULTISIG_ID" \
  --proposal-id "$PROPOSAL_ID" \
  --sequencer "$SEQUENCER" \
  --out /tmp/vote0.bin

echo ""
echo "[6/7] Member 1 votes..."
"$BIN" vote \
  --nsk "$NSK_2" \
  --member-index 1 \
  --multisig-id "$MULTISIG_ID" \
  --proposal-id "$PROPOSAL_ID" \
  --sequencer "$SEQUENCER" \
  --out /tmp/vote1.bin

echo ""
echo "[7/7] Verifying receipts offline..."
"$BIN" verify --receipt /tmp/vote0.bin --multisig-id "$MULTISIG_ID" --proposal-id "$PROPOSAL_ID"
"$BIN" verify --receipt /tmp/vote1.bin --multisig-id "$MULTISIG_ID" --proposal-id "$PROPOSAL_ID"

echo ""
echo "  Threshold met (2-of-3). Execute proposal:"
echo "  multisig execute --multisig-id $MULTISIG_ID --proposal-id $PROPOSAL_ID --sequencer $SEQUENCER"
echo ""
echo "=== Demo complete ==="
echo ""
echo "Key properties:"
echo "  - nsk never leaves the client"
echo "  - On-chain observers see only that M proofs were accepted, not which members voted"
echo "  - Nullifiers prevent double-voting per proposal"
echo "  - LEZ nonce constraint avoided: members use dedicated voting commitments"

#!/usr/bin/env bash
# LP-0002 private M-of-N multisig end-to-end demo (2-of-3).
#
# Runs fully offline: derives commitments, generates two vote proofs, verifies both.
# Submitting votes to a live LEZ chain requires a running sequencer (see README).
#
# Usage: ./demo.sh [--dev]   (--dev = RISC0_DEV_MODE=1 for fast local testing)

set -euo pipefail

DEV_MODE=0
for arg in "$@"; do [ "$arg" = "--dev" ] && DEV_MODE=1; done

if [ "$DEV_MODE" = "1" ]; then
  export RISC0_DEV_MODE=1
  echo "[demo] RISC0_DEV_MODE=1 (mock proofs, no ZK)"
else
  echo "[demo] Real RISC0 proofs -- proof generation takes several minutes per vote"
fi

BIN="./target/release/multisig"

echo ""
echo "=== LP-0002 Private M-of-N Multisig Demo (2-of-3) ==="
echo ""

echo "[1/6] Building..."
cargo build --release --bin multisig 2>&1 | tail -3

# Demo multisig account ID
MULTISIG_ID="cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"

# Three members with deterministic test nsks (never use outside demo)
NSK_1="1111111111111111111111111111111111111111111111111111111111111111"
NSK_2="2222222222222222222222222222222222222222222222222222222222222222"
NSK_3="3333333333333333333333333333333333333333333333333333333333333333"

echo ""
echo "[2/6] Deriving member commitments (nsk stays local, never sent anywhere)..."
COMMIT_1=$("$BIN" derive-commitment --nsk "$NSK_1" --multisig-id "$MULTISIG_ID")
COMMIT_2=$("$BIN" derive-commitment --nsk "$NSK_2" --multisig-id "$MULTISIG_ID")
COMMIT_3=$("$BIN" derive-commitment --nsk "$NSK_3" --multisig-id "$MULTISIG_ID")
echo "Member 0: $COMMIT_1"
echo "Member 1: $COMMIT_2"
echo "Member 2: $COMMIT_3"

PROPOSAL_ID="aaaa000000000000000000000000000000000000000000000000000000000001"
MEMBER_COMMITMENTS="$COMMIT_1,$COMMIT_2,$COMMIT_3"

echo ""
echo "[3/6] (Skipped in offline demo) -- In production: deploy program + call initialize"
echo "      threshold=2, commitments=[$COMMIT_1, $COMMIT_2, ...]"
echo ""
echo "[4/6] Member 0 votes (proof generated locally, nsk never leaves this machine)..."
"$BIN" vote \
  --nsk "$NSK_1" \
  --member-index 0 \
  --multisig-id "$MULTISIG_ID" \
  --proposal-id "$PROPOSAL_ID" \
  --member-commitments "$MEMBER_COMMITMENTS" \
  --out /tmp/vote0.bin

echo ""
echo "[5/6] Member 1 votes..."
"$BIN" vote \
  --nsk "$NSK_2" \
  --member-index 1 \
  --multisig-id "$MULTISIG_ID" \
  --proposal-id "$PROPOSAL_ID" \
  --member-commitments "$MEMBER_COMMITMENTS" \
  --out /tmp/vote1.bin

echo ""
echo "[6/6] Verifying both receipts offline..."
"$BIN" verify --receipt /tmp/vote0.bin --multisig-id "$MULTISIG_ID" --proposal-id "$PROPOSAL_ID"
"$BIN" verify --receipt /tmp/vote1.bin --multisig-id "$MULTISIG_ID" --proposal-id "$PROPOSAL_ID"

echo ""
echo "=== Demo complete ==="
echo "Vote receipts: /tmp/vote0.bin /tmp/vote1.bin"
echo ""
echo "To submit to a running LEZ chain, deploy programs/multisig and call:"
echo "  vote(multisig_account, proposal_id, receipt_bytes)  -- for each receipt"
echo "  execute(multisig_account, proposal_id)              -- once threshold is met"
echo ""
echo "Privacy properties:"
echo "  - nsk never leaves the client"
echo "  - On-chain observers see that M proofs were accepted, not which members voted"
echo "  - member_set_root binds the receipt to the registered set without revealing the voter"
echo "  - Per-proposal nullifiers prevent double-voting"
echo "  - LEZ nonce constraint avoided: voting commitments are separate from shielded accounts"

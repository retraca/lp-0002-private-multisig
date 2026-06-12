#!/usr/bin/env bash
# LP-0002 private M-of-N multisig end-to-end demo (2-of-3).
#
# Offline mode (default): derives commitments, generates two vote proofs, verifies both.
# Chain mode (--chain):   full on-chain lifecycle -- initialize, proposal, two votes
#                         delivered as privacy-preserving transactions (chained-call
#                         composition through the vote-circuit program), execute.
#
# Usage:
#   ./demo.sh [--dev] [--chain] [--sequencer <url>]
#
#   --dev        RISC0_DEV_MODE=1 (fast mock proofs, no ZK work)
#   --chain      Run on-chain steps. Requires:
#                  1. wallet deploy-program output path in PROGRAM_BIN (see below)
#                  2. cargo build --release --features chain
#                  3. A running sequencer (local docker or testnet)
#
# Testnet:
#   SEQUENCER=https://testnet.lez.logos.co ./demo.sh --dev --chain

set -euo pipefail

DEV_MODE=0
CHAIN_MODE=0
SEQUENCER="${SEQUENCER:-http://127.0.0.1:3040}"
for arg in "$@"; do
  [ "$arg" = "--dev" ]   && DEV_MODE=1
  [ "$arg" = "--chain" ] && CHAIN_MODE=1
done

if [ "$DEV_MODE" = "1" ]; then
  export RISC0_DEV_MODE=1
  echo "[demo] RISC0_DEV_MODE=1 (mock proofs, no ZK)"
else
  echo "[demo] Real RISC0 proofs -- proof generation takes several minutes per vote"
fi

# Build target: add --features chain when running on-chain steps.
if [ "$CHAIN_MODE" = "1" ]; then
  BIN_FEATURES="--features chain"
else
  BIN_FEATURES=""
fi

BIN="./target/release/multisig"
# Program binary from wallet deploy-program (override with PROGRAM_BIN env var).
# Produced by: wallet deploy-program programs/multisig/target/.../private_multisig
PROGRAM_BIN="${PROGRAM_BIN:-}"
# Program ID from deploy step (set below; override if already deployed).
PROGRAM_ID="${PROGRAM_ID:-}"

echo ""
echo "=== LP-0002 Private M-of-N Multisig Demo (2-of-3) ==="
echo "Sequencer: $SEQUENCER"
echo ""

echo "[1/7] Building..."
# shellcheck disable=SC2086
cargo build --release --bin multisig $BIN_FEATURES 2>&1 | tail -3

# Multisig account: in chain mode the account is claimed by the program at
# initialize, which requires the tx to be signed with the account's key.
# Generate a fresh key and derive the multisig ID from it.
if [ "$CHAIN_MODE" = "1" ]; then
  KEYGEN_OUT=$("$BIN" chain keygen 2>/dev/null || true)
  if [ -z "$KEYGEN_OUT" ]; then
    # binary not built with chain yet; build then retry after [1/7]
    cargo build --release --bin multisig --features chain 2>&1 | tail -1
    KEYGEN_OUT=$("$BIN" chain keygen)
  fi
  SIGNING_KEY=$(echo "$KEYGEN_OUT" | grep '^signing_key:' | awk '{print $2}')
  MULTISIG_ID=$(echo "$KEYGEN_OUT" | grep '^multisig_id:' | awk '{print $2}')
  echo "Multisig account: $MULTISIG_ID (fresh key, demo only)"
else
  # Offline mode: deterministic ID (no on-chain claiming happens)
  MULTISIG_ID="cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
fi

# Three members with deterministic test nsks (never use outside demo)
NSK_1="1111111111111111111111111111111111111111111111111111111111111111"
NSK_2="2222222222222222222222222222222222222222222222222222222222222222"
NSK_3="3333333333333333333333333333333333333333333333333333333333333333"

echo ""
echo "[2/7] Deriving member commitments (nsk stays local, never sent anywhere)..."
COMMIT_1=$("$BIN" derive-commitment --nsk "$NSK_1" --multisig-id "$MULTISIG_ID")
COMMIT_2=$("$BIN" derive-commitment --nsk "$NSK_2" --multisig-id "$MULTISIG_ID")
COMMIT_3=$("$BIN" derive-commitment --nsk "$NSK_3" --multisig-id "$MULTISIG_ID")
echo "Member 0: $COMMIT_1"
echo "Member 1: $COMMIT_2"
echo "Member 2: $COMMIT_3"

PROPOSAL_ID="aaaa000000000000000000000000000000000000000000000000000000000001"
MEMBER_COMMITMENTS="$COMMIT_1,$COMMIT_2,$COMMIT_3"

echo ""
if [ "$CHAIN_MODE" = "1" ]; then
  echo "[3/7] Deploying program + initializing multisig on-chain..."
  if [ -z "$PROGRAM_ID" ] && [ -n "$PROGRAM_BIN" ]; then
    echo "  Deploying program binary: $PROGRAM_BIN"
    PROGRAM_ID=$(wallet deploy-program "$PROGRAM_BIN" | grep -oE '[0-9a-f]{64}' | head -1)
    echo "  Program ID: $PROGRAM_ID"
  elif [ -z "$PROGRAM_ID" ]; then
    echo "  Using testnet program IDs (deployed, see docs/TESTNET_EVIDENCE.md)"
    PROGRAM_ID="${PROGRAM_ID:-fb2d6afe695b3d03736f6a7f869d980884afc61f24d5199194f0891555a8a8e3}"
  fi
  VOTE_CIRCUIT_PROGRAM_ID="${VOTE_CIRCUIT_PROGRAM_ID:-7af8104a46999ed81962d5eb0dc4482db84a1352bacc95e86210fe1a46f87063}"
  TX=$("$BIN" chain initialize \
    --sequencer "$SEQUENCER" \
    --program-id "$PROGRAM_ID" \
    --vote-circuit-program-id "$VOTE_CIRCUIT_PROGRAM_ID" \
    --signing-key "$SIGNING_KEY" \
    --threshold 2 \
    --commitments "$MEMBER_COMMITMENTS")
  echo "  Initialize tx: $TX"
  echo "  Waiting for inclusion..."
  for i in $(seq 1 24); do
    STATE=$("$BIN" chain state --sequencer "$SEQUENCER" --multisig-id "$MULTISIG_ID" 2>/dev/null || true)
    echo "$STATE" | grep -q "program_owner: $PROGRAM_ID" && break
    sleep 5
  done
  "$BIN" chain state --sequencer "$SEQUENCER" --multisig-id "$MULTISIG_ID"

  echo ""
  echo "  Submitting proposal (unsigned: the account is now program-owned)..."
  TX=$("$BIN" chain submit-proposal \
    --sequencer "$SEQUENCER" \
    --program-id "$PROGRAM_ID" \
    --multisig-id "$MULTISIG_ID" \
    --proposal-id "$PROPOSAL_ID" \
    --action "7472616e73666572203130300a")
  echo "  Proposal tx: $TX"
  echo "  Waiting for inclusion..."
  for i in $(seq 1 24); do
    "$BIN" chain state --sequencer "$SEQUENCER" --multisig-id "$MULTISIG_ID" 2>/dev/null \
      | grep -q "${PROPOSAL_ID:0:8}" && break
    sleep 5
  done
else
  echo "[3/7] On-chain init skipped (pass --chain to run on-chain steps)"
  echo "      threshold=2, commitments=[$COMMIT_1, $COMMIT_2, ...]"
fi

echo ""
echo "[4/7] Member 0 votes (proof generated locally, nsk never leaves this machine)..."
"$BIN" vote \
  --nsk "$NSK_1" \
  --member-index 0 \
  --multisig-id "$MULTISIG_ID" \
  --proposal-id "$PROPOSAL_ID" \
  --member-commitments "$MEMBER_COMMITMENTS" \
  --out /tmp/vote0.bin

echo ""
echo "[5/7] Member 1 votes..."
"$BIN" vote \
  --nsk "$NSK_2" \
  --member-index 1 \
  --multisig-id "$MULTISIG_ID" \
  --proposal-id "$PROPOSAL_ID" \
  --member-commitments "$MEMBER_COMMITMENTS" \
  --out /tmp/vote1.bin

echo ""
echo "[6/7] Verifying both receipts offline..."
"$BIN" verify --receipt /tmp/vote0.bin --multisig-id "$MULTISIG_ID" --proposal-id "$PROPOSAL_ID"
"$BIN" verify --receipt /tmp/vote1.bin --multisig-id "$MULTISIG_ID" --proposal-id "$PROPOSAL_ID"

echo ""
if [ "$CHAIN_MODE" = "1" ]; then
  echo "[7/7] Casting votes on-chain (privacy-preserving transactions) and executing..."
  echo ""
  echo "  Each vote runs the vote-circuit program locally with the nsk as a"
  echo "  PRIVATE input, chains the call into the multisig program, and submits"
  echo "  one composite proof. The nsk never leaves this machine."
  echo ""
  TX=$("$BIN" chain vote \
    --sequencer "$SEQUENCER" \
    --program-id "$PROGRAM_ID" \
    --multisig-id "$MULTISIG_ID" \
    --proposal-id "$PROPOSAL_ID" \
    --nsk "$NSK_1" \
    --member-index 0 | grep '^tx:' | awk '{print $2}')
  echo "  Vote 0 tx: $TX"

  TX=$("$BIN" chain vote \
    --sequencer "$SEQUENCER" \
    --program-id "$PROGRAM_ID" \
    --multisig-id "$MULTISIG_ID" \
    --proposal-id "$PROPOSAL_ID" \
    --nsk "$NSK_2" \
    --member-index 1 | grep '^tx:' | awk '{print $2}')
  echo "  Vote 1 tx: $TX"

  echo "  Waiting for votes to land..."
  for i in $(seq 1 36); do
    DATA=$("$BIN" chain state --sequencer "$SEQUENCER" --multisig-id "$MULTISIG_ID" 2>/dev/null | grep '^data' || true)
    echo "$DATA" | grep -qE "\(255 bytes\)" && break
    sleep 5
  done

  TX=$("$BIN" chain execute \
    --sequencer "$SEQUENCER" \
    --program-id "$PROGRAM_ID" \
    --multisig-id "$MULTISIG_ID" \
    --proposal-id "$PROPOSAL_ID")
  echo "  Execute tx: $TX"
  sleep 12
  echo ""
  echo "  Final multisig state (vote_count=2, executed=true, 2 spent nullifiers):"
  "$BIN" chain state --sequencer "$SEQUENCER" --multisig-id "$MULTISIG_ID"
else
  echo "[7/7] On-chain submit skipped (pass --chain to run on-chain steps)"
fi

echo ""
echo "=== Demo complete ==="
echo "Vote receipts: /tmp/vote0.bin /tmp/vote1.bin"
echo ""
if [ "$CHAIN_MODE" = "0" ]; then
  echo "To run the full on-chain demo:"
  echo "  cargo build --release --features chain"
  echo "  SEQUENCER=https://testnet.lez.logos.co ./demo.sh --dev --chain"
fi
echo ""
echo "Privacy properties:"
echo "  - nsk never leaves the client"
echo "  - On-chain observers see that M proofs were accepted, not which members voted"
echo "  - member_set_root binds the proof to the registered set without revealing the voter"
echo "  - Per-proposal nullifiers prevent double-voting"
echo "  - LEZ nonce constraint avoided: voting commitments are separate from shielded accounts"

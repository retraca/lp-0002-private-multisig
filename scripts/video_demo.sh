#!/usr/bin/env bash
# LP-0002 narrated video demo driver.
#
# Runs the full private-multisig lifecycle against the hosted LEZ testnet with
# REAL proofs (RISC0_DEV_MODE unset = 0). Designed to be screen-recorded:
# clear section banners, explicit dev-mode display, explorer links per tx.
#
# Record with:
#   asciinema rec --cols 100 --rows 32 -c 'bash scripts/video_demo.sh' lp0002.cast
#
# Program IDs are the deployed v2 binaries (see docs/TESTNET_EVIDENCE.md).

set -euo pipefail

SEQUENCER="https://testnet.lez.logos.co"
EXPLORER="https://explorer.testnet.lez.logos.co/transaction"
BIN="./target/release/multisig"
MSPID="fb2d6afe695b3d03736f6a7f869d980884afc61f24d5199194f0891555a8a8e3"
VCPID="7af8104a46999ed81962d5eb0dc4482db84a1352bacc95e86210fe1a46f87063"

# Real proofs: dev mode OFF.
unset RISC0_DEV_MODE || true

NSK_1="1111111111111111111111111111111111111111111111111111111111111111"
NSK_2="2222222222222222222222222222222222222222222222222222222222222222"
NSK_3="3333333333333333333333333333333333333333333333333333333333333333"
PROPOSAL_ID="aaaa000000000000000000000000000000000000000000000000000000000001"

banner() {
  echo ""
  echo "=================================================================="
  echo "  $*"
  echo "=================================================================="
  echo ""
}

wait_grep() {  # $1 = multisig_id, $2 = grep pattern (for initialize/proposal)
  for _ in $(seq 1 40); do
    "$BIN" chain state --sequencer "$SEQUENCER" --multisig-id "$1" 2>/dev/null \
      | grep -q "$2" && return 0
    sleep 5
  done
  echo "timed out waiting for: $2" >&2; exit 1
}

# Decode the proposal's (vote_count executed nullifiers) from on-chain state.
state_triple() {  # $1 = multisig_id
  local data
  data=$("$BIN" chain state --sequencer "$SEQUENCER" --multisig-id "$1" 2>/dev/null \
    | grep '^data' | sed -E 's/.*: //') || true
  python3 "$(dirname "$0")/parse_state.py" "${data:-}"
}

wait_votes() {  # $1 = multisig_id, $2 = expected vote_count
  local mid="$1" want="$2" vc ex nn
  for _ in $(seq 1 60); do
    read -r vc ex nn < <(state_triple "$mid")
    [ "${vc:-0}" -ge "$want" ] && return 0
    sleep 5
  done
  echo "timed out waiting for vote_count >= $want" >&2; exit 1
}

wait_executed() {  # $1 = multisig_id
  local mid="$1" vc ex nn
  for _ in $(seq 1 48); do
    read -r vc ex nn < <(state_triple "$mid")
    [ "${ex:-0}" = "1" ] && return 0
    sleep 5
  done
  echo "timed out waiting for executed=1" >&2; exit 1
}

banner "LP-0002  Private M-of-N Multisig on the Logos Execution Zone"
echo "Sequencer : $SEQUENCER"
echo "Proving   : REAL (RISC0_DEV_MODE is '${RISC0_DEV_MODE:-unset}' = off)"
echo "Programs  : multisig      $MSPID"
echo "            vote-circuit  $VCPID"
echo ""
echo "A 2-of-3 multisig where members vote privately: the chain learns that a"
echo "threshold was met, never which members voted."

banner "STEP 1  Create the multisig account key"
echo "\$ multisig chain keygen"
KEYOUT=$("$BIN" chain keygen)
echo "$KEYOUT"
SK=$(echo "$KEYOUT" | grep '^signing_key:' | awk '{print $2}')
MID=$(echo "$KEYOUT" | grep '^multisig_id:' | awk '{print $2}')

banner "STEP 2  Each member derives a private commitment (nsk stays local)"
C1=$("$BIN" derive-commitment --nsk "$NSK_1" --multisig-id "$MID")
C2=$("$BIN" derive-commitment --nsk "$NSK_2" --multisig-id "$MID")
C3=$("$BIN" derive-commitment --nsk "$NSK_3" --multisig-id "$MID")
echo "member 0 commitment: $C1"
echo "member 1 commitment: $C2"
echo "member 2 commitment: $C3"

banner "STEP 3  Initialize the 2-of-3 multisig on-chain (signed)"
echo "\$ multisig chain initialize --threshold 2 --commitments c0,c1,c2 ..."
TX=$("$BIN" chain initialize --sequencer "$SEQUENCER" --program-id "$MSPID" \
  --vote-circuit-program-id "$VCPID" --signing-key "$SK" --threshold 2 \
  --commitments "$C1,$C2,$C3" | grep '^tx:' | awk '{print $2}')
echo "initialize tx: $TX"
echo "explorer     : $EXPLORER/$TX"
wait_grep "$MID" "program_owner: $MSPID"
echo "on-chain state after initialize:"
"$BIN" chain state --sequencer "$SEQUENCER" --multisig-id "$MID"

banner "STEP 4  Submit a proposal: \"transfer 100\""
TX=$("$BIN" chain submit-proposal --sequencer "$SEQUENCER" --program-id "$MSPID" \
  --multisig-id "$MID" --proposal-id "$PROPOSAL_ID" \
  --action "7472616e7366657220313030" | grep '^tx:' | awk '{print $2}')
echo "proposal tx: $TX"
echo "explorer   : $EXPLORER/$TX"
wait_grep "$MID" "aaaa00"

banner "STEP 5  Member 0 votes  (REAL proof generates now -- watch for it)"
echo "The vote-circuit program proves locally; the nsk never leaves this machine."
TX=$("$BIN" chain vote --sequencer "$SEQUENCER" --program-id "$MSPID" \
  --multisig-id "$MID" --proposal-id "$PROPOSAL_ID" \
  --nsk "$NSK_1" --member-index 0 | grep '^tx:' | awk '{print $2}')
echo "vote 0 tx: $TX"
echo "explorer : $EXPLORER/$TX"
wait_votes "$MID" 1
echo "vote_count is now 1 (one private vote recorded, voter unknown to the chain)"

banner "STEP 6  Member 1 votes  (REAL proof again)"
TX=$("$BIN" chain vote --sequencer "$SEQUENCER" --program-id "$MSPID" \
  --multisig-id "$MID" --proposal-id "$PROPOSAL_ID" \
  --nsk "$NSK_2" --member-index 1 | grep '^tx:' | awk '{print $2}')
echo "vote 1 tx: $TX"
echo "explorer : $EXPLORER/$TX"
wait_votes "$MID" 2
echo "vote_count is now 2 -- threshold met"

banner "STEP 7  Execute the proposal"
TX=$("$BIN" chain execute --sequencer "$SEQUENCER" --program-id "$MSPID" \
  --multisig-id "$MID" --proposal-id "$PROPOSAL_ID" | grep '^tx:' | awk '{print $2}')
echo "execute tx: $TX"
echo "explorer  : $EXPLORER/$TX"
wait_executed "$MID"
echo ""
echo "final on-chain state:"
"$BIN" chain state --sequencer "$SEQUENCER" --multisig-id "$MID"

banner "DONE  2-of-3 reached and executed, with real proofs, no voter revealed"
echo "multisig account: $MID"
echo "Every step is a real transaction on $SEQUENCER."

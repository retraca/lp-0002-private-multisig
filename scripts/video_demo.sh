#!/usr/bin/env bash
# LP-0002 demo driver — clean terminal session for screen recording.
#
# Runs the full private 2-of-3 multisig lifecycle against the hosted LEZ
# testnet with REAL proofs (RISC0_DEV_MODE off). Output is intentionally
# terse: commands and their real results, the way you'd actually run them.
# All explanation lives in the voiceover, not on screen.
#
# Record:  asciinema rec --overwrite -c 'bash scripts/video_demo.sh' lp0002.cast

set -euo pipefail

SEQ="${SEQUENCER:-https://testnet.lez.logos.co}"
BIN="./target/release/multisig"
MSPID="fb2d6afe695b3d03736f6a7f869d980884afc61f24d5199194f0891555a8a8e3"
VCPID="7af8104a46999ed81962d5eb0dc4482db84a1352bacc95e86210fe1a46f87063"
unset RISC0_DEV_MODE || true

NSK1="1111111111111111111111111111111111111111111111111111111111111111"
NSK2="2222222222222222222222222222222222222222222222222222222222222222"
NSK3="3333333333333333333333333333333333333333333333333333333333333333"
PROP="aaaa000000000000000000000000000000000000000000000000000000000001"

# --- display helpers -------------------------------------------------------
p()   { printf '$ %s\n' "$*"; }          # show a command line
beat() { echo; sleep 1.5; }              # small visual pause between steps

state_triple() {  # -> "vote_count executed nullifiers"
  local data
  data=$("$BIN" chain state --sequencer "$SEQ" --multisig-id "$MID" 2>/dev/null \
    | grep '^data' | sed -E 's/.*: //') || true
  python3 "$(dirname "$0")/parse_state.py" "${data:-}"
}

show_state() {  # clean one-line decode of on-chain state
  local vc ex nn e
  read -r vc ex nn < <(state_triple)
  e="false"; [ "${ex:-0}" = "1" ] && e="true"
  echo "  threshold=2  vote_count=${vc:-0}  executed=$e  spent_nullifiers=${nn:-0}"
}

wait_owned() { for _ in $(seq 1 40); do
  "$BIN" chain state --sequencer "$SEQ" --multisig-id "$MID" 2>/dev/null \
    | grep -q "program_owner: $MSPID" && return 0; sleep 5; done
  echo "timed out" >&2; exit 1; }
wait_prop() { for _ in $(seq 1 40); do
  "$BIN" chain state --sequencer "$SEQ" --multisig-id "$MID" 2>/dev/null \
    | grep -q "aaaa00" && return 0; sleep 5; done
  echo "timed out" >&2; exit 1; }
wait_votes() { local want="$1" vc ex nn; for _ in $(seq 1 60); do
  read -r vc ex nn < <(state_triple); [ "${vc:-0}" -ge "$want" ] && return 0
  sleep 5; done; echo "timed out" >&2; exit 1; }
wait_exec() { local vc ex nn; for _ in $(seq 1 48); do
  read -r vc ex nn < <(state_triple); [ "${ex:-0}" = "1" ] && return 0
  sleep 5; done; echo "timed out" >&2; exit 1; }

# --- session ---------------------------------------------------------------
echo "# LP-0002  private 2-of-3 multisig  —  Logos Execution Zone testnet"
echo "# real proofs (RISC0_DEV_MODE off)"
beat

echo "# create the multisig account"
p "multisig chain keygen"
KEYOUT=$("$BIN" chain keygen); echo "$KEYOUT"
SK=$(echo "$KEYOUT" | awk '/signing_key/{print $2}')
MID=$(echo "$KEYOUT" | awk '/^multisig_id/{print $2}')
beat

echo "# each member derives a commitment from their secret key (stays local)"
for nsk in "$NSK1" "$NSK2" "$NSK3"; do
  p "multisig derive-commitment --nsk \$NSK --multisig-id \$MID"
  "$BIN" derive-commitment --nsk "$nsk" --multisig-id "$MID"
done
C1=$("$BIN" derive-commitment --nsk "$NSK1" --multisig-id "$MID")
C2=$("$BIN" derive-commitment --nsk "$NSK2" --multisig-id "$MID")
C3=$("$BIN" derive-commitment --nsk "$NSK3" --multisig-id "$MID")
beat

echo "# create the 2-of-3 multisig on-chain"
p "multisig chain initialize --threshold 2 \\"
echo "      --program-id \$MSPID --vote-circuit-program-id \$VCPID \\"
echo "      --signing-key \$SK --commitments \$C1,\$C2,\$C3"
TX=$("$BIN" chain initialize --sequencer "$SEQ" --program-id "$MSPID" \
  --vote-circuit-program-id "$VCPID" --signing-key "$SK" --threshold 2 \
  --commitments "$C1,$C2,$C3" | awk '/^tx:/{print $2}')
echo "tx: $TX"
wait_owned
show_state
beat

echo "# anyone can propose; only the threshold gates execution"
p "multisig chain submit-proposal --proposal-id \$PROP --action 'transfer 100'"
TX=$("$BIN" chain submit-proposal --sequencer "$SEQ" --program-id "$MSPID" \
  --multisig-id "$MID" --proposal-id "$PROP" \
  --action "7472616e7366657220313030" | awk '/^tx:/{print $2}')
echo "tx: $TX"
wait_prop
show_state
beat

echo "# member 0 votes — real proof, generated locally (a few minutes)"
p "multisig chain vote --member-index 0 --nsk \$NSK1 --proposal-id \$PROP"
TX=$("$BIN" chain vote --sequencer "$SEQ" --program-id "$MSPID" \
  --multisig-id "$MID" --proposal-id "$PROP" \
  --nsk "$NSK1" --member-index 0 | awk '/^tx:/{print $2}')
echo "tx: $TX"
wait_votes 1
show_state
beat

echo "# member 1 votes — real proof again"
p "multisig chain vote --member-index 1 --nsk \$NSK2 --proposal-id \$PROP"
TX=$("$BIN" chain vote --sequencer "$SEQ" --program-id "$MSPID" \
  --multisig-id "$MID" --proposal-id "$PROP" \
  --nsk "$NSK2" --member-index 1 | awk '/^tx:/{print $2}')
echo "tx: $TX"
wait_votes 2
show_state
beat

echo "# threshold met — execute"
p "multisig chain execute --proposal-id \$PROP"
TX=$("$BIN" chain execute --sequencer "$SEQ" --program-id "$MSPID" \
  --multisig-id "$MID" --proposal-id "$PROP" | awk '/^tx:/{print $2}')
echo "tx: $TX"
wait_exec
show_state
beat

echo "# done — approved by 2 of 3, executed, voters never revealed"
echo "# multisig account: $MID"

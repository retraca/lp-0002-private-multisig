#!/usr/bin/env bash
# LP-0002 full 2-of-3 lifecycle against the HOSTED LEZ testnet (REAL proofs).
# This is the script that produced docs/TESTNET_EVIDENCE.md.
#
# Prereqs: built repo (cargo build --release -p private-multisig-cli; guest via
# cargo +risc0), a built LEZ v0.2.0 `wallet` binary (WALLET env), and a
# spendable public funder account (FUNDER_KEY env; the v0.2.0 genesis key in
# lez/testnet_initial_state works while the faucetless testnet allows it).
#
#   WALLET=/path/to/lez/target/release/wallet ./scripts/testnet-run.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MSIG="$ROOT/target/release/multisig"
ELF="$ROOT/programs/multisig/target/riscv32im-risc0-zkvm-elf/release/private_multisig"
WALLET="${WALLET:?path to the LEZ v0.2.0 wallet binary}"
SEQ="${SEQUENCER:-https://testnet.lez.logos.co}"
FUNDER_KEY="${FUNDER_KEY:-10a26a9aec7d34b82364eeae45c5294dbb0a764b000b94eeb9b58511dc487c4d}"
DUST="${DUST:-5}"
D="${TN_HOME:-$ROOT/.testnet-home}"

export RISC0_DEV_MODE=0
mkdir -p "$D"
cat > "$D/wallet_config.json" <<JSON
{"sequencer_addr":"$SEQ","seq_poll_timeout":"60s","seq_tx_poll_max_blocks":40,"seq_poll_max_retries":40,"seq_block_poll_max_amount":300}
JSON
export LEE_WALLET_HOME_DIR="$D"
w() { RUST_LOG=error "$WALLET" "$@"; }
die() { echo "FATAL: $*" >&2; exit 1; }
votes_now() { "$MSIG" chain state --multisig-id "$MULTISIG_ID" 2>/dev/null | grep -o 'votes=[0-9]*' | head -1 | cut -d= -f2; }
wait_for() { local n=$1; shift; for _ in $(seq 1 "$n"); do "$@" && return 0; sleep 15; done; return 1; }

printf 'testnet\n' | w account list >/dev/null 2>&1 || true
w account import public --private-key "$FUNDER_KEY" >/dev/null 2>&1 || true
FUNDER_ID=$(w account list 2>/dev/null | grep -o 'Public/[1-9A-HJ-NP-Za-km-z]*' | head -1 | cut -d/ -f2)
[ -n "$FUNDER_ID" ] || die "no funder"
echo "funder: Public/$FUNDER_ID"

echo "### members (fresh seeds)"
M0=$("$MSIG" member new); M1=$("$MSIG" member new); M2=$("$MSIG" member new)
echo "$M0"; echo "$M1"; echo "$M2"
NSK0=$(echo "$M0" | sed -n 's/^nsk: //p'); VID0=$(echo "$M0" | sed -n 's|^voting_account: Private/||p'); SEED0=$(echo "$M0" | sed -n 's/^seed: //p')
NSK1=$(echo "$M1" | sed -n 's/^nsk: //p'); VID1=$(echo "$M1" | sed -n 's|^voting_account: Private/||p'); SEED1=$(echo "$M1" | sed -n 's/^seed: //p')
NSK2=$(echo "$M2" | sed -n 's/^nsk: //p')
PROPOSAL_ID=$(python3 -c "import os;print(os.urandom(32).hex())")
ACTION="736574206665655f627073203d203235"

echo "### deploy"
"$MSIG" chain program-id --program-bin "$ELF"
"$MSIG" chain deploy --program-bin "$ELF" || die deploy
sleep 90

echo "### initialize"
KEYGEN=$("$MSIG" chain keygen); echo "$KEYGEN"
SIGNING_KEY=$(echo "$KEYGEN" | sed -n 's/^signing_key: //p')
MULTISIG_ID=$(echo "$KEYGEN" | sed -n 's/^multisig_id: //p')
C0=$("$MSIG" derive-commitment --nsk "$NSK0" --multisig-id "$MULTISIG_ID")
C1=$("$MSIG" derive-commitment --nsk "$NSK1" --multisig-id "$MULTISIG_ID")
C2=$("$MSIG" derive-commitment --nsk "$NSK2" --multisig-id "$MULTISIG_ID")
"$MSIG" chain initialize --program-bin "$ELF" --signing-key "$SIGNING_KEY" --threshold 2 --commitments "$C0,$C1,$C2" || die initialize
wait_for 30 sh -c "\"$MSIG\" chain state --multisig-id $MULTISIG_ID 2>/dev/null | grep -q 'threshold: 2'" || die "initialize did not land"

echo "### proposal $PROPOSAL_ID"
"$MSIG" chain submit-proposal --program-bin "$ELF" --multisig-id "$MULTISIG_ID" --proposal-id "$PROPOSAL_ID" --action "$ACTION" || die proposal
wait_for 30 sh -c "\"$MSIG\" chain state --multisig-id $MULTISIG_ID 2>/dev/null | grep -q 'proposal $PROPOSAL_ID'" || die "proposal did not land"

echo "### fund voting accounts (live riders)"
"$MSIG" member import --seed "$SEED0"
"$MSIG" member import --seed "$SEED1"
w auth-transfer send --from "Public/$FUNDER_ID" --to "Private/$VID0" --amount "$DUST" || die "fund m0"
w auth-transfer send --from "Public/$FUNDER_ID" --to "Private/$VID1" --amount "$DUST" || die "fund m1"
sleep 60

echo "### vote member 0 (real proof)"
"$MSIG" chain vote --program-bin "$ELF" --multisig-id "$MULTISIG_ID" --proposal-id "$PROPOSAL_ID" --nsk "$NSK0" --member-index 0 || die "vote 0"
wait_for 30 sh -c "[ \"\$(\"$MSIG\" chain state --multisig-id $MULTISIG_ID 2>/dev/null | grep -o 'votes=[0-9]*' | head -1 | cut -d= -f2)\" = 1 ]" || die "vote 0 did not land"

echo "### vote member 1 (real proof)"
"$MSIG" chain vote --program-bin "$ELF" --multisig-id "$MULTISIG_ID" --proposal-id "$PROPOSAL_ID" --nsk "$NSK1" --member-index 1 || die "vote 1"
wait_for 30 sh -c "[ \"\$(\"$MSIG\" chain state --multisig-id $MULTISIG_ID 2>/dev/null | grep -o 'votes=[0-9]*' | head -1 | cut -d= -f2)\" = 2 ]" || die "vote 1 did not land"

echo "### double vote must fail in-circuit"
if "$MSIG" chain vote --program-bin "$ELF" --multisig-id "$MULTISIG_ID" --proposal-id "$PROPOSAL_ID" --nsk "$NSK0" --member-index 0; then
  die "double vote accepted"
fi
echo "double vote rejected (ERR_6004)"

echo "### execute"
"$MSIG" chain execute --program-bin "$ELF" --multisig-id "$MULTISIG_ID" --proposal-id "$PROPOSAL_ID" || die execute
wait_for 30 sh -c "\"$MSIG\" chain state --multisig-id $MULTISIG_ID 2>/dev/null | grep -q 'executed=true'" || die "execute did not land"
"$MSIG" chain state --multisig-id "$MULTISIG_ID"
echo "=== TESTNET LIFECYCLE COMPLETE ==="

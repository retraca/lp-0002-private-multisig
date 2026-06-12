#!/usr/bin/env python3
"""Decode a MultisigState hex blob and print: vote_count executed nullifiers.

Borsh layout (see programs/multisig/src/lib.rs MultisigState):
  threshold: u8
  vote_circuit_program_id: [u32; 8]      (32 bytes)
  member_commitments: Vec<[u8; 32]>      (4-byte len + n*32)
  proposals: Vec<Proposal>               (4-byte len + entries)
    Proposal: id[32], action: Vec<u8>(4+N), vote_count u8, executed u8,
              spent_nullifiers: Vec<[u8;32]>(4 + m*32)

Prints "0 0 0" for an empty / unparseable state so callers can poll safely.
Only the first proposal is reported (the demo uses one).
"""
import sys


def main() -> None:
    hexstr = sys.argv[1].strip() if len(sys.argv) > 1 else ""
    try:
        b = bytes.fromhex(hexstr)
        o = 1 + 32                      # threshold + vote_circuit_program_id
        n_members = int.from_bytes(b[o:o + 4], "little"); o += 4
        o += n_members * 32
        n_props = int.from_bytes(b[o:o + 4], "little"); o += 4
        if n_props == 0:
            print("0 0 0"); return
        o += 32                          # proposal id
        action_len = int.from_bytes(b[o:o + 4], "little"); o += 4
        o += action_len
        vote_count = b[o]; o += 1
        executed = b[o]; o += 1
        n_null = int.from_bytes(b[o:o + 4], "little")
        print(f"{vote_count} {executed} {n_null}")
    except Exception:
        print("0 0 0")


if __name__ == "__main__":
    main()

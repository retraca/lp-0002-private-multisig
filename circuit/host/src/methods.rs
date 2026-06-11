// Pre-built guest binary (R0BF packaged, risc0-zkvm 3.0.5).
// To regenerate: cargo +risc0 build --release --target riscv32im-risc0-zkvm-elf
// in circuit/guest/, then python3 scripts/package_r0bf.py <guest_elf> <kernel_elf> circuit/guest/private-multisig-guest.bin
pub const PRIVATE_MULTISIG_GUEST_ELF: &[u8] =
    include_bytes!("../../guest/private-multisig-guest.bin");
pub const PRIVATE_MULTISIG_GUEST_ID: [u32; 8] = [
    0x5f6a1b22, 0xfd1c38bb, 0x5e9dc112, 0x097897f7,
    0x3edf606b, 0x1975db45, 0x1cccfd5e, 0xa74b8b23,
];

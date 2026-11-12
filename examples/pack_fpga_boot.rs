// Packs a REBEL-6 .tas program into the compact R6BT SPI NOR Flash boot image
// consumed by rebel6_soc_host.v at flash offset 0x100000:
//   [0] magic          = "R6BT" (0x54423652)
//   [1] instr_count    (u32)
//   [2] data_base      (u32)
//   [3] init_data_len  (u32, trimmed of trailing zero heap/BSS bytes)
//   [4] clear_words    (u32, 32-bit SDRAM words cleared to 0 at boot)
//   [5] stack_top      (u32)
//   [6] entry_pc       (u32)
//   [7] wad_flash_addr (u32, 0x200000)
//   [8] wad_size       (u32, byte length of vendor/DOOM1.WAD)
//   followed by:
//   - instr_count * 8 bytes (little-endian u64 packed 32-trit instructions)
//   - init_data_len bytes   (initialized .data/.rodata bytes)
use std::env;
use std::fs;
use std::path::Path;

use setun_doom::rebel6::asm::assemble;

fn main() {
    let args: Vec<String> = env::args().collect();
    let tas_path = args
        .get(1)
        .map(String::as_str)
        .unwrap_or("/home/robson/coding/setun-doom-rebel6/vendor/doomgeneric/riscv32/build/rebel6/doom_all.tas");
    let out_path = args
        .get(2)
        .map(String::as_str)
        .unwrap_or("fpga/build/doom_boot.bin");
    let wad_path = args
        .get(3)
        .map(String::as_str)
        .unwrap_or("vendor/DOOM1.WAD");

    let src = fs::read_to_string(tas_path).expect("cannot read .tas");
    let img = assemble(&src).expect("assemble failed");

    // Trim trailing zeros in .data (the 4 MiB .zero heap is cleared in hardware)
    let init_data_len = img
        .data
        .iter()
        .rposition(|&c| (c & 0xFF) != 0)
        .map_or(0, |idx| idx + 1);

    let wad_size = if Path::new(wad_path).exists() {
        fs::metadata(wad_path).expect("wad metadata").len() as u32
    } else {
        0
    };

    let stack_top: u32 = 6_500_000;
    let clear_bytes: u32 = 6_553_600; // covers data + 4 MiB heap + BSS + stack
    let clear_words: u32 = clear_bytes / 4;
    let wad_flash_addr: u32 = 0x0020_0000;

    let mut out = Vec::with_capacity(36 + img.instrs.len() * 8 + init_data_len);
    out.extend_from_slice(b"R6BT");
    out.extend_from_slice(&(img.instrs.len() as u32).to_le_bytes());
    out.extend_from_slice(&(img.data_base as u32).to_le_bytes());
    out.extend_from_slice(&(init_data_len as u32).to_le_bytes());
    out.extend_from_slice(&clear_words.to_le_bytes());
    out.extend_from_slice(&stack_top.to_le_bytes());
    out.extend_from_slice(&(img.entry as u32).to_le_bytes());
    out.extend_from_slice(&wad_flash_addr.to_le_bytes());
    out.extend_from_slice(&wad_size.to_le_bytes());

    for &w in &img.instrs {
        out.extend_from_slice(&w.to_le_bytes());
    }
    for &c in &img.data[..init_data_len] {
        out.push((c & 0xFF) as u8);
    }

    if let Some(parent) = Path::new(out_path).parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(out_path, &out).expect("cannot write R6BT boot image");
    println!(
        "R6BT packed -> {out_path}: total={} bytes ({:.1} KiB), instrs={}, data_base={}, init_data_len={} (saved {} zero bytes), entry={}, wad_size={}",
        out.len(),
        out.len() as f64 / 1024.0,
        img.instrs.len(),
        img.data_base,
        init_data_len,
        img.data.len() - init_data_len,
        img.entry,
        wad_size
    );
}

//! CLI tool to execute a native .setun binary directly on the Setun-Doom 21-trit CPU.

use std::env;
use std::fs;

use setun_doom::codegen::LinkedImage;
use setun_doom::cpu::Cpu;
use setun_doom::memory::Memory;
use setun_doom::word::Word21;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: setun-run <binary.setun> [arg0] [arg1] ... [arg6]");
        std::process::exit(1);
    }

    let bin_path = &args[1];
    let binary_bytes = fs::read(bin_path)
        .unwrap_or_else(|e| panic!("Failed to read Setun binary file '{}': {}", bin_path, e));

    let image = LinkedImage::from_bytes(&binary_bytes)
        .unwrap_or_else(|e| panic!("Failed to deserialize Setun binary: {}", e));

    println!(
        "[setun-run] Loaded binary '{}' ({} code words, entry PC = {})",
        bin_path,
        image.code_words.len(),
        image.entry_pc
    );

    let mut mem = Memory::new();
    let mut cpu = Cpu::new();

    image.load_into_memory(&mut mem);

    // Pass CLI arguments into index registers IX0..IX6
    for (i, arg_str) in args[2..].iter().enumerate().take(7) {
        if let Ok(val) = arg_str.parse::<i64>() {
            cpu.index_regs[i] = Word21::new(val);
            println!("  [setun-run] Argument IX{} = {}", i, val);
        }
    }

    cpu.pc = image.entry_pc as i64;

    println!("[setun-run] Starting CPU execution...");
    let steps = cpu.run_ticks(&mut mem, 10_000_000);

    let tel = cpu.telemetry;
    println!("================================================================");
    println!(" SETUN NATIVE EXECUTION COMPLETED");
    println!("================================================================");
    println!(
        " * Exit Status       : {}",
        if cpu.halted {
            "HALT (Clean Exit)"
        } else {
            "Active/Timeout"
        }
    );
    println!(" * Steps / Cycles    : {}", steps);
    println!(" * Return ACC (21t)  : {}", cpu.acc.as_i64());
    println!(
        " * Physical Flips    : {} Trit Flips (Half: {}, Full: {}) vs Binary32: {} ({:.2}% reduction)",
        tel.ternary_trit_flips,
        tel.ternary_half_swings,
        tel.ternary_full_swings,
        tel.binary32_bit_flips,
        tel.digit_flip_reduction_pct()
    );
    println!("================================================================");
}

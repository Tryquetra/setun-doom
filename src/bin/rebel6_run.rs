//! Runs a `.tas` image on the REBEL-6 core.
//!
//! Usage: `rebel6-run <file.tas> [max_steps]`

use std::env;
use std::fs;
use std::process::ExitCode;

use setun_doom::rebel6::asm::assemble;
use setun_doom::rebel6::cpu::{RunResult, Trap};
use setun_doom::rebel6::word::POW3;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let Some(path) = args.get(1) else {
        eprintln!("usage: rebel6-run <file.tas> [max_steps]");
        return ExitCode::from(2);
    };
    let max_steps = args
        .get(2)
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(2_000_000_000);
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(err) => {
            eprintln!("cannot read {path}: {err}");
            return ExitCode::from(1);
        }
    };
    let image = match assemble(&source) {
        Ok(image) => image,
        Err(err) => {
            eprintln!("assembly failed: {err}");
            return ExitCode::from(1);
        }
    };
    let mut cpu = image.machine();
    cpu.trace = std::env::var("REBEL6_TRACE").is_ok();
    cpu.write_reg(2, setun_doom::rebel6::word::Word24::new(1_594_324))
        .unwrap();
    cpu.write_reg(1, setun_doom::rebel6::word::Word24::new(-POW3[16]))
        .unwrap();
    println!(
        "image: {} instructions, {} data bytes, entry {}, data base {:#x}",
        image.instrs.len(),
        image.data.len(),
        image.entry,
        image.data_base
    );
    let result = cpu.run(max_steps);
    for i in 0..32 {
        println!(
            "x{i}\t0x{:x}\t{}",
            cpu.regs[i].to_i32(),
            cpu.regs[i].to_i32()
        );
    }
    if let Some(&dst) = image.symbols.get("dst") {
        if let Ok(bytes) = cpu.data.read_bytes(dst, 8) {
            println!("dst bytes: {:?}", String::from_utf8_lossy(&bytes));
        }
    }
    match result {
        Ok(RunResult::Ecall(num)) => {
            println!(
                "ecall a7={num} a0={} gp={}",
                cpu.regs[10].raw(),
                cpu.regs[3].raw()
            );
            println!(
                "cycles={} trit_flips={} energy={}",
                cpu.cycles, cpu.trit_flips, cpu.energy
            );
            ExitCode::SUCCESS
        }
        Ok(RunResult::Halt(code)) => {
            println!("halt code={code}");
            println!(
                "cycles={} trit_flips={} energy={}",
                cpu.cycles, cpu.trit_flips, cpu.energy
            );
            ExitCode::SUCCESS
        }
        Err(Trap::StepLimit) => {
            println!("step limit at pc={} cycles={}", cpu.pc, cpu.cycles);
            ExitCode::from(3)
        }
        Err(trap) => {
            println!("trap at pc={}: {trap:?}", cpu.pc);
            ExitCode::from(4)
        }
    }
}

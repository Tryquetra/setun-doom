//! CLI tool to execute a native .setun binary directly on the Setun-Doom 21-trit CPU.

use std::time::Duration;
use std::{env, fs};

use setun_doom::codegen::LinkedImage;
use setun_doom::cpu::Cpu;
use setun_doom::memory::Memory;
use setun_doom::word::Word21;
use setun_doom::Instruction;

const DEFAULT_MAX_STEPS: usize = 10_000_000;
const DEFAULT_TRACE_STEPS: usize = 10_000;
const DEFAULT_TRACE_DELAY_MS: u64 = 60;

const USAGE: &str =
    "Usage: setun-run <binary.setun> [arg0] ... [arg6] [--trace] [--trace-steps N] \
     [--trace-delay-ms M] [--max-steps N]";

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut bin_path: Option<String> = None;
    let mut ix_args: Vec<String> = Vec::new();
    let mut trace = false;
    let mut trace_steps = DEFAULT_TRACE_STEPS;
    let mut trace_delay_ms = DEFAULT_TRACE_DELAY_MS;
    let mut max_steps = DEFAULT_MAX_STEPS;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--trace" => trace = true,
            "--trace-steps" => {
                i += 1;
                let value = args.get(i).unwrap_or_else(|| {
                    eprintln!("[setun-run] --trace-steps requires a value\n{USAGE}");
                    std::process::exit(2);
                });
                trace_steps = match value.parse::<usize>() {
                    Ok(steps) if steps > 0 => steps,
                    _ => {
                        eprintln!("[setun-run] invalid --trace-steps value '{value}'");
                        std::process::exit(2);
                    }
                };
            }
            "--trace-delay-ms" => {
                i += 1;
                let value = args.get(i).unwrap_or_else(|| {
                    eprintln!("[setun-run] --trace-delay-ms requires a value\n{USAGE}");
                    std::process::exit(2);
                });
                trace_delay_ms = match value.parse::<u64>() {
                    Ok(delay) => delay,
                    Err(_) => {
                        eprintln!("[setun-run] invalid --trace-delay-ms value '{value}'");
                        std::process::exit(2);
                    }
                };
            }
            "--max-steps" => {
                i += 1;
                let value = args.get(i).unwrap_or_else(|| {
                    eprintln!("[setun-run] --max-steps requires a value\n{USAGE}");
                    std::process::exit(2);
                });
                max_steps = match value.parse::<usize>() {
                    Ok(steps) if steps > 0 => steps,
                    _ => {
                        eprintln!("[setun-run] invalid --max-steps value '{value}'");
                        std::process::exit(2);
                    }
                };
            }
            option if option.starts_with("--") => {
                eprintln!("[setun-run] unknown option '{option}'\n{USAGE}");
                std::process::exit(2);
            }
            value => {
                if bin_path.is_none() {
                    bin_path = Some(value.to_string());
                } else {
                    ix_args.push(value.to_string());
                }
            }
        }
        i += 1;
    }

    let bin_path = match bin_path {
        Some(path) => path,
        None => {
            eprintln!("{USAGE}");
            std::process::exit(1);
        }
    };

    let binary_bytes = fs::read(&bin_path)
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
    for (i, arg_str) in ix_args.iter().enumerate().take(7) {
        match arg_str.parse::<i64>() {
            Ok(val) => {
                cpu.index_regs[i] = Word21::new(val);
                println!("  [setun-run] Argument IX{} = {}", i, val);
            }
            Err(_) => eprintln!(
                "[setun-run] argument IX{} = '{}' is not an integer and was ignored",
                i, arg_str
            ),
        }
    }

    cpu.pc = image.entry_pc as i64;
    let mut executed_total = 0usize;

    if trace {
        println!(
            "[setun-run] Trace mode: chunk={} steps, delay={} ms, max_steps={}",
            trace_steps, trace_delay_ms, max_steps
        );
        while !cpu.halted && cpu.trap.is_none() && executed_total < max_steps {
            let chunk = trace_steps.min(max_steps - executed_total);
            let executed = cpu.run_ticks(&mut mem, chunk);
            if executed == 0 {
                break;
            }
            executed_total += executed;

            let (op, reg, operand) = match mem.read_word(cpu.pc) {
                Ok(word) => match Instruction::decode(word) {
                    Some(instr) => (format!("{:?}", instr.opcode), instr.reg_idx, instr.operand),
                    None => (String::from("INVALID"), 0, 0),
                },
                Err(error) => {
                    eprintln!("[setun-run] trace read fault at PC {}: {:?}", cpu.pc, error);
                    break;
                }
            };

            println!(
                "[trace {:>9}] pc={:<7} op={:<12} r={} operand={:<10} acc={:<12} \
                 ix0={:<12} flips={}",
                executed_total,
                cpu.pc,
                op,
                reg,
                operand,
                cpu.acc.as_i64(),
                cpu.index_regs[0].as_i64(),
                cpu.telemetry.ternary_trit_flips
            );

            if trace_delay_ms > 0 {
                std::thread::sleep(Duration::from_millis(trace_delay_ms));
            }
        }

        if !cpu.halted && cpu.trap.is_none() {
            println!(
                "[setun-run] Trace stopped at the step budget of {} (CPU still active)",
                max_steps
            );
        }
    } else {
        println!("[setun-run] Starting CPU execution...");
        executed_total = cpu.run_ticks(&mut mem, max_steps);
    }

    let tel = cpu.telemetry;
    println!("================================================================");
    println!(" SETUN NATIVE EXECUTION COMPLETED");
    println!("================================================================");
    println!(
        " * Exit Status       : {}",
        if cpu.halted {
            "HALT (Clean Exit)"
        } else if cpu.trap.is_some() {
            "TRAP"
        } else {
            "Active/Timeout"
        }
    );
    if let Some(trap) = &cpu.trap {
        println!(" * Trap              : {:?}", trap);
    }
    println!(" * Steps / Cycles    : {}", executed_total);
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

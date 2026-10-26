// Copyright (c) 2026 Robson Cassiano (https://eu.robsoncassiano.software)
// SPDX-License-Identifier: MIT

//! Integration test verifying Level 2 compilation:
//! C code -> Clang -emit-llvm -> LLVM IR (.ll) -> SetunCodeGenerator -> Native Setun Execution.

use std::fs;
use std::process::Command;

use setun_doom::codegen::{LlvmParser, SetunCodeGenerator};
use setun_doom::cpu::Cpu;
use setun_doom::memory::Memory;
use setun_doom::word::Word21;

#[test]
fn test_c_source_to_native_setun_execution() {
    // 1. Write a standalone C test routine computing an arithmetic expression
    let c_code = r#"
    int compute_sample(int a, int b) {
        int sum = a + b;
        int diff = a - b;
        int prod = sum * diff;
        return prod;
    }
    "#;

    let tmp_dir = std::env::temp_dir();
    let c_path = tmp_dir.join("test_compute_sample.c");
    let ll_path = tmp_dir.join("test_compute_sample.ll");

    fs::write(&c_path, c_code).expect("Failed to write temporary C file");

    // 2. Invoke host Clang compiler emitting clean LLVM IR
    let clang_status = Command::new("clang")
        .args([
            "-O1",
            "-S",
            "-emit-llvm",
            c_path.to_str().unwrap(),
            "-o",
            ll_path.to_str().unwrap(),
        ])
        .status()
        .expect("Failed to execute host clang command");
    assert!(clang_status.success(), "Clang execution failed");

    let ll_content = fs::read_to_string(&ll_path).expect("Failed to read emitted .ll file");

    // Clean up temporary files
    let _ = fs::remove_file(c_path);
    let _ = fs::remove_file(ll_path);

    // 3. Parse LLVM IR using Setun LlvmParser
    let module = LlvmParser::parse(&ll_content).expect("Failed to parse emitted LLVM IR");
    assert!(
        module.functions.contains_key("compute_sample"),
        "Function 'compute_sample' missing from parsed LLVM module"
    );

    let func = &module.functions["compute_sample"];

    // 4. Compile function into native Setun instructions
    let instructions = SetunCodeGenerator::compile_function(func)
        .expect("Failed to generate Setun instructions from LLVM function");

    println!("Generated Setun instructions:");
    for (i, instr) in instructions.iter().enumerate() {
        println!("  {:>2}: {:?}", i, instr);
    }

    assert!(
        !instructions.is_empty(),
        "Code generator produced empty instruction sequence"
    );

    // 5. Load and execute the native instruction sequence in Setun CPU
    // Test case: a = 10, b = 4 -> sum = 14, diff = 6 -> prod = 14 * 6 = 84
    let mut mem = Memory::new();
    let mut cpu = Cpu::new();

    // Set parameters in IX0 (a) and IX1 (b) according to calling convention
    cpu.index_regs[0] = Word21::new(10);
    cpu.index_regs[1] = Word21::new(4);

    cpu.load_program(&mut mem, 0, &instructions);
    let steps = cpu.run_ticks(&mut mem, 100);

    println!(
        "[Level 2 Test] Executed {} steps. Return ACC = {}",
        steps,
        cpu.acc.as_i64()
    );

    // Result of (10 + 4) * (10 - 4) = 14 * 6 = 84
    assert_eq!(
        cpu.acc.as_i64(),
        84,
        "Native Setun execution produced unexpected return value"
    );
}

#[test]
fn test_c_branch_and_load_store_execution() {
    // 2. Test branch and local memory load/store with unoptimized IR (-O0)
    let c_code = r#"
    int branch_and_mem(int x) {
        volatile int val = 50;
        if (x > 0) {
            val = val + x;
        } else {
            val = val - x;
        }
        return val;
    }
    "#;

    let tmp_dir = std::env::temp_dir();
    let c_path = tmp_dir.join("test_branch_and_mem.c");
    let ll_path = tmp_dir.join("test_branch_and_mem.ll");

    fs::write(&c_path, c_code).expect("Failed to write temporary C file");

    let clang_status = Command::new("clang")
        .args([
            "-O0",
            "-S",
            "-emit-llvm",
            c_path.to_str().unwrap(),
            "-o",
            ll_path.to_str().unwrap(),
        ])
        .status()
        .expect("Failed to execute host clang command");
    assert!(clang_status.success(), "Clang execution failed");

    let ll_content = fs::read_to_string(&ll_path).expect("Failed to read emitted .ll file");
    let _ = fs::remove_file(c_path);
    let _ = fs::remove_file(ll_path);

    let module = LlvmParser::parse(&ll_content).expect("Failed to parse LLVM IR");
    let func = &module.functions["branch_and_mem"];
    let instructions = SetunCodeGenerator::compile_function(func)
        .expect("Failed to compile branch_and_mem function");

    println!("Compiled branch_and_mem instructions:");
    for (i, instr) in instructions.iter().enumerate() {
        println!("  {:>2}: {:?}", i, instr);
    }

    // Case 1: x = 15 -> val = 50 + 15 = 65
    let mut mem = Memory::new();
    let mut cpu = Cpu::new();
    cpu.index_regs[0] = Word21::new(15);
    cpu.load_program(&mut mem, 0, &instructions);
    cpu.run_ticks(&mut mem, 200);
    assert_eq!(cpu.acc.as_i64(), 65);

    // Case 2: x = -10 -> val = 50 - (-10) = 60
    let mut mem2 = Memory::new();
    let mut cpu2 = Cpu::new();
    cpu2.index_regs[0] = Word21::new(-10);
    cpu2.load_program(&mut mem2, 0, &instructions);
    cpu2.run_ticks(&mut mem2, 200);
    assert_eq!(cpu2.acc.as_i64(), 60);
}

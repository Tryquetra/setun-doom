// Copyright (c) 2026 Robson Cassiano (https://eu.robsoncassiano.software)
// SPDX-License-Identifier: MIT

//! Integration test verifying in-memory static linker with multi-module resolution & dead code elimination.

use std::fs;
use std::process::Command;

use setun_doom::codegen::{LlvmParser, SetunLinker, CODE_SEGMENT_BASE};
use setun_doom::cpu::Cpu;
use setun_doom::memory::Memory;
use setun_doom::word::Word21;

#[test]
fn test_multimodule_linking_and_tree_shaking() {
    let tmp_dir = std::env::temp_dir();

    // Module A (math_ops.c): exports add_numbers and mul_numbers, plus an unused function
    let module_a_c = r#"
    int add_numbers(int a, int b) {
        return a + b;
    }

    int mul_numbers(int a, int b) {
        return a * b;
    }

    int unused_dead_code(int x) {
        return x * 100;
    }
    "#;

    // Module B (main_app.c): entry point computing expressions
    let module_b_c = r#"
    int main_entry(int x, int y) {
        int sum = x + y;
        int prod = x * y;
        int total = sum + prod;
        return total;
    }
    "#;

    let a_c_path = tmp_dir.join("mod_a.c");
    let a_ll_path = tmp_dir.join("mod_a.ll");
    let b_c_path = tmp_dir.join("mod_b.c");
    let b_ll_path = tmp_dir.join("mod_b.ll");

    fs::write(&a_c_path, module_a_c).expect("Failed to write mod_a.c");
    fs::write(&b_c_path, module_b_c).expect("Failed to write mod_b.c");

    // Compile both C modules with Clang to LLVM IR
    let status_a = Command::new("clang")
        .args(["-O1", "-S", "-emit-llvm", a_c_path.to_str().unwrap(), "-o", a_ll_path.to_str().unwrap()])
        .status()
        .expect("Failed to compile mod_a.c");
    assert!(status_a.success());

    let status_b = Command::new("clang")
        .args(["-O1", "-S", "-emit-llvm", b_c_path.to_str().unwrap(), "-o", b_ll_path.to_str().unwrap()])
        .status()
        .expect("Failed to compile mod_b.c");
    assert!(status_b.success());

    let ll_a = fs::read_to_string(&a_ll_path).unwrap();
    let ll_b = fs::read_to_string(&b_ll_path).unwrap();

    let _ = fs::remove_file(a_c_path);
    let _ = fs::remove_file(a_ll_path);
    let _ = fs::remove_file(b_c_path);
    let _ = fs::remove_file(b_ll_path);

    let parsed_a = LlvmParser::parse(&ll_a).expect("Failed to parse mod_a.ll");
    let parsed_b = LlvmParser::parse(&ll_b).expect("Failed to parse mod_b.ll");

    // 1. Initialize SetunLinker and add both compilation units
    let mut linker = SetunLinker::new();
    linker.add_module(parsed_a);
    linker.add_module(parsed_b);

    // 2. Link from entry point 'main_entry'
    let image = linker.link("main_entry").expect("Linker failed to produce image");

    // 3. Verify entry point layout
    assert_eq!(image.entry_pc, CODE_SEGMENT_BASE);
    assert!(image.symbol_table.contains_key("main_entry"));

    // 4. Verify tree-shaking: unused_dead_code was never called and should not be in the symbol table
    assert!(
        !image.symbol_table.contains_key("unused_dead_code"),
        "Tree-shaking failed: unused function was included in image"
    );

    // 5. Load linked image directly into Setun memory and execute on CPU
    let mut mem = Memory::new();
    let mut cpu = Cpu::new();

    image.load_into_memory(&mut mem);

    // Set parameters: x = 5, y = 6 -> sum = 11, prod = 30 -> total = 41
    cpu.index_regs[0] = Word21::new(5);
    cpu.index_regs[1] = Word21::new(6);
    cpu.pc = image.entry_pc as i64;

    let steps = cpu.run_ticks(&mut mem, 100);
    println!("[Linker Test] Executed {} steps. Return ACC = {}", steps, cpu.acc.as_i64());

    assert_eq!(
        cpu.acc.as_i64(),
        41,
        "Executable image produced incorrect mathematical result"
    );
}

#[test]
fn test_multimodule_with_function_calls() {
    let mod_math = r#"
    int square(int x) {
        return x * x;
    }
    "#;
    let mod_main = r#"
    int square(int x);
    int calc_hypot_sq(int a, int b) {
        int a2 = square(a);
        int b2 = square(b);
        return a2 + b2;
    }
    "#;

    let tmp_dir = std::env::temp_dir();
    let math_c = tmp_dir.join("test_math.c");
    let math_ll = tmp_dir.join("test_math.ll");
    let main_c = tmp_dir.join("test_main.c");
    let main_ll = tmp_dir.join("test_main.ll");

    fs::write(&math_c, mod_math).unwrap();
    fs::write(&main_c, mod_main).unwrap();

    let _ = Command::new("clang").args(["-O1", "-S", "-emit-llvm", math_c.to_str().unwrap(), "-o", math_ll.to_str().unwrap()]).status().unwrap();
    let _ = Command::new("clang").args(["-O1", "-S", "-emit-llvm", main_c.to_str().unwrap(), "-o", main_ll.to_str().unwrap()]).status().unwrap();

    let parsed_math = LlvmParser::parse(&fs::read_to_string(&math_ll).unwrap()).unwrap();
    let parsed_main = LlvmParser::parse(&fs::read_to_string(&main_ll).unwrap()).unwrap();

    let mut linker = SetunLinker::new();
    linker.add_module(parsed_math);
    linker.add_module(parsed_main);

    let image = linker.link("calc_hypot_sq").expect("Failed to link");
    println!("Image entry_pc: {}", image.entry_pc);
    for (addr, word) in &image.code_words {
        let instr = setun_doom::cpu::Instruction::decode(*word).unwrap();
        println!("  {}: {:?}", addr, instr);
    }

    let mut mem = Memory::new();
    let mut cpu = Cpu::new();
    image.load_into_memory(&mut mem);

    cpu.index_regs[0] = Word21::new(3);
    cpu.index_regs[1] = Word21::new(4);
    cpu.pc = image.entry_pc as i64;

    for step in 0..100 {
        if cpu.halted {
            break;
        }
        let pc = cpu.pc;
        let instr = setun_doom::cpu::Instruction::decode(mem.read_word_fast(pc)).unwrap();
        println!("Step {:2}: PC={} [{:?}] ACC={} IX0={} IX1={} IX2={} IX3={}",
            step, pc, instr, cpu.acc.as_i64(), cpu.index_regs[0].as_i64(), cpu.index_regs[1].as_i64(), cpu.index_regs[2].as_i64(), cpu.index_regs[3].as_i64());
        cpu.step(&mut mem);
    }

    assert_eq!(cpu.acc.as_i64(), 25);
}

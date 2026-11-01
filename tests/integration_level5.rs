// Copyright (c) 2026 Robson Cassiano (https://eu.robsoncassiano.software)
// SPDX-License-Identifier: MIT

//! Integration test for Level 5: C source compiled by the REBEL-6 backend and executed on the
//! 32-trit core, with no libc. The last test compiles the same C for RV32I, translates it with
//! R2R and checks that both paths reach the same result, which is the exit criterion of the
//! Level 5 foundation phase (`docs/level-5-6-plan.md`).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use setun_doom::codegen::{LlvmParser, Rebel6CodeGenerator};
use setun_doom::rebel6::asm::assemble;
use setun_doom::rebel6::cpu::RunResult;
use setun_doom::rebel6::word::Word24;

/// Stack top used by the loader, matching `src/rebel6/asm.rs`.
const STACK_TOP: i64 = 0x40_0000;

/// Expected result of `tests/level5/arith.c`: the sum of every term the program verifies.
const EXPECTED: i32 = 144222;

fn source_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/level5/arith.c")
}

/// Emits LLVM IR for the level 5 program at the given optimisation level.
fn emit_ir(level: &str) -> String {
    let output = Command::new("clang")
        .args([
            &format!("-O{level}"),
            "-S",
            "-emit-llvm",
            source_path().to_str().expect("source path"),
            "-o",
            "-",
        ])
        .output()
        .expect("clang must be installed");
    assert!(output.status.success(), "clang failed: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).expect("clang emits UTF-8")
}

/// Compiles the program and runs it on the REBEL-6 core, returning the value left in `a0`.
fn run_program(level: &str) -> i32 {
    let ir = emit_ir(level);
    let module = LlvmParser::parse(&ir).expect("the IR parses");
    let assembly = Rebel6CodeGenerator::compile_module(&module, "main").expect("the module compiles");
    let image = assemble(&assembly).expect("the assembly assembles");
    let mut cpu = image.machine();
    cpu.write_reg(2, Word24::new(STACK_TOP)).expect("sp is writable");
    match cpu.run(200_000) {
        Ok(RunResult::Ecall(0)) => cpu.regs[10].to_i32(),
        other => panic!("program did not exit through ecall 0: {other:?}"),
    }
}

#[test]
fn arithmetic_c_program_runs_on_the_rebel6_core() {
    assert_eq!(run_program("1"), EXPECTED);
}

#[test]
fn unoptimised_ir_with_memory_slots_runs_too() {
    assert_eq!(run_program("0"), EXPECTED);
}

/// Builds the RV32I translation of `tests/level5/arith.c` and returns the `.tas` it wrote.
fn build_translated(level: &str) -> PathBuf {
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/build_level5_parity.py");
    let output = Command::new("python3")
        .arg(&script)
        .args(["-O", level])
        .output()
        .expect("python3 must be installed");
    assert!(
        output.status.success(),
        "the RV32I build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let printed = String::from_utf8(output.stdout).expect("the build script prints UTF-8");
    let path = PathBuf::from(printed.trim());
    assert!(
        path.is_file(),
        "the build script did not write {}",
        path.display()
    );
    path
}

/// Runs a `.tas` image from `_start` and returns the value the exit syscall leaves in `a0`.
fn run_image(path: &Path) -> i32 {
    let source = fs::read_to_string(path).expect("the translated image is readable");
    let image = assemble(&source).expect("the translated image assembles");
    let mut cpu = image.machine();
    cpu.write_reg(2, Word24::new(STACK_TOP))
        .expect("sp is writable");
    match cpu.run(2_000_000) {
        Ok(RunResult::Ecall(0)) => cpu.regs[10].to_i32(),
        other => panic!("{} did not exit through ecall 0: {other:?}", path.display()),
    }
}

#[test]
fn rv32i_translation_and_native_backend_agree() {
    let translated = run_image(&build_translated("0"));
    assert_eq!(
        translated, EXPECTED,
        "the translated program returns the expected score"
    );
    assert_eq!(
        translated,
        run_program("0"),
        "both paths must return the same score"
    );
}

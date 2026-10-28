// Copyright (c) 2026 Robson Cassiano (https://eu.robsoncassiano.software)
// SPDX-License-Identifier: MIT

//! Integration test verifying minilibc runtime execution on native Setun 21-trit processor.

use std::fs;
use std::process::Command;

use setun_doom::codegen::{LlvmParser, SetunLinker};
use setun_doom::cpu::Cpu;
use setun_doom::memory::Memory;
use setun_doom::word::Word21;

#[test]
fn test_minilibc_memory_and_string_operations() {
    let test_c_code = r#"
    #include "minilibc.h"

    int run_test(int seed) {
        char buf[32];
        memset(buf, 0, 32);
        strcpy(buf, "Doom-Setun");

        int len = (int)strlen(buf);
        int cmp = strcmp(buf, "Doom-Setun");

        char *heap_str = (char *)malloc(16);
        memcpy(heap_str, buf, 4);
        heap_str[4] = '\0';

        int sub_len = (int)strlen(heap_str);

        // len is 10, cmp is 0, sub_len is 4. Result: 10 + 0 + 4 + seed = 14 + seed
        return len + cmp + sub_len + seed;
    }
    "#;

    let tmp_dir = std::env::temp_dir();
    let test_c = tmp_dir.join("test_runtime.c");
    let test_ll = tmp_dir.join("test_runtime.ll");
    let libc_ll = tmp_dir.join("minilibc_test.ll");

    fs::write(&test_c, test_c_code).unwrap();

    let status = Command::new("clang")
        .args([
            "-O1",
            "-S",
            "-emit-llvm",
            "-fno-builtin",
            "-Isrc/runtime",
            test_c.to_str().unwrap(),
            "-o",
            test_ll.to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(
        status.success(),
        "Failed to compile test_runtime.c with clang"
    );

    let status_libc = Command::new("clang")
        .args([
            "-O1",
            "-S",
            "-emit-llvm",
            "-fno-builtin",
            "-Isrc/runtime",
            "src/runtime/minilibc.c",
            "-o",
            libc_ll.to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(
        status_libc.success(),
        "Failed to compile minilibc.c with clang"
    );

    let parsed_test = LlvmParser::parse(&fs::read_to_string(&test_ll).unwrap()).unwrap();
    let parsed_libc = LlvmParser::parse(&fs::read_to_string(&libc_ll).unwrap()).unwrap();

    let mut linker = SetunLinker::new();
    linker.add_module(parsed_test);
    linker.add_module(parsed_libc);

    let image = linker
        .link("run_test")
        .expect("Linking minilibc test failed");

    let mut mem = Memory::new();
    let mut cpu = Cpu::new();
    image.load_into_memory(&mut mem);

    // Call run_test(seed = 100) -> 14 + 100 = 114
    cpu.index_regs[0] = Word21::new(100);
    cpu.pc = image.entry_pc as i64;

    let steps = cpu.run_ticks(&mut mem, 3000);

    assert!(
        cpu.halted,
        "CPU should have halted cleanly within {} steps",
        steps
    );
    assert_eq!(cpu.acc.as_i64(), 114, "run_test produced unexpected result");
}

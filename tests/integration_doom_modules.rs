// Copyright (c) 2026 Robson Cassiano (https://eu.robsoncassiano.software)
// SPDX-License-Identifier: MIT

//! Integration test compiling and running authentic Doom engine modules on native Setun 21-trit processor.

use std::fs;
use std::process::Command;

use setun_doom::codegen::{LlvmParser, SetunLinker};
use setun_doom::cpu::Cpu;
use setun_doom::memory::Memory;
use setun_doom::word::Word21;

#[test]
fn test_doom_fixed_point_and_bounding_box() {
    let test_c_code = r#"
    #include "minilibc.h"
    #include "doomtype.h"
    #include "m_fixed.h"
    #include "m_bbox.h"
    #include "m_random.h"

    int test_doom_core(int seed) {
        fixed_t one = 1 << 16;
        fixed_t mul_res = FixedMul(2 * one, 3 * one);
        fixed_t div_res = FixedDiv(10 * one, 2 * one);

        fixed_t box[4];
        M_ClearBox(box);
        M_AddToBox(box, 10 * one, 20 * one);
        M_AddToBox(box, -5 * one, 30 * one);
        M_AddToBox(box, 50 * one, -15 * one);

        fixed_t top = box[BOXTOP] >> 16;
        fixed_t bottom = box[BOXBOTTOM] >> 16;
        fixed_t left = box[BOXLEFT] >> 16;
        fixed_t right = box[BOXRIGHT] >> 16;

        int bbox_sum = (int)(top + bottom + left + right);
        int mul_int = (int)(mul_res >> 16);
        int div_int = (int)(div_res >> 16);

        M_ClearRandom();
        int r1 = M_Random();  // 8
        int r2 = M_Random();  // 109
        int pr1 = P_Random(); // 8
        int r_sum = r1 + r2 + pr1; // 125

        return mul_int + div_int + bbox_sum + r_sum + seed;
    }
    "#;

    let tmp_dir = std::env::temp_dir();
    let test_c = tmp_dir.join("test_doom_entry.c");
    let test_ll = tmp_dir.join("test_doom_entry.ll");
    let fixed_ll = tmp_dir.join("doom_m_fixed.ll");
    let bbox_ll = tmp_dir.join("doom_m_bbox.ll");
    let random_ll = tmp_dir.join("doom_m_random.ll");
    let libc_ll = tmp_dir.join("doom_minilibc.ll");

    fs::write(&test_c, test_c_code).unwrap();

    let compile_file = |src: &str, dst: &std::path::Path| {
        let status = Command::new("clang")
            .args([
                "-O1",
                "-S",
                "-emit-llvm",
                "-fno-builtin",
                "-Isrc/runtime",
                "-Ivendor/doomgeneric/doomgeneric",
                src,
                "-o",
                dst.to_str().unwrap(),
            ])
            .status()
            .unwrap_or_else(|e| panic!("Failed to execute clang on {}: {}", src, e));
        assert!(status.success(), "Clang compilation failed for {}", src);
    };

    compile_file(test_c.to_str().unwrap(), &test_ll);
    compile_file("vendor/doomgeneric/doomgeneric/m_fixed.c", &fixed_ll);
    compile_file("vendor/doomgeneric/doomgeneric/m_bbox.c", &bbox_ll);
    compile_file("vendor/doomgeneric/doomgeneric/m_random.c", &random_ll);
    compile_file("src/runtime/minilibc.c", &libc_ll);

    let parsed_test = LlvmParser::parse(&fs::read_to_string(&test_ll).unwrap()).unwrap();
    let parsed_fixed = LlvmParser::parse(&fs::read_to_string(&fixed_ll).unwrap()).unwrap();
    let parsed_bbox = LlvmParser::parse(&fs::read_to_string(&bbox_ll).unwrap()).unwrap();
    let parsed_random = LlvmParser::parse(&fs::read_to_string(&random_ll).unwrap()).unwrap();
    let parsed_libc = LlvmParser::parse(&fs::read_to_string(&libc_ll).unwrap()).unwrap();

    let mut linker = SetunLinker::new();
    linker.add_module(parsed_test);
    linker.add_module(parsed_fixed);
    linker.add_module(parsed_bbox);
    linker.add_module(parsed_random);
    linker.add_module(parsed_libc);

    let image = linker
        .link("test_doom_core")
        .expect("Failed to link Doom core modules");

    let mut mem = Memory::new();
    let mut cpu = Cpu::new();
    image.load_into_memory(&mut mem);

    // Call test_doom_core(seed = 100) -> 6 + 5 + 60 + 125 + 100 = 296
    cpu.index_regs[0] = Word21::new(100);
    cpu.pc = image.entry_pc as i64;

    let steps = cpu.run_ticks(&mut mem, 4000);

    assert!(
        cpu.halted,
        "CPU should have halted cleanly within {} steps",
        steps
    );
    assert_eq!(
        cpu.acc.as_i64(),
        296,
        "Doom core test produced unexpected result"
    );
}

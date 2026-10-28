// Copyright (c) 2026 Robson Cassiano (https://eu.robsoncassiano.software)
// SPDX-License-Identifier: MIT

//! Equivalence checks between the gate-accurate engine and the opt-in fast engine.
//!
//! The fast mode exists only to make interactive play viable. It must produce the same
//! architectural results as the veracity mode, and it must not fabricate switching telemetry.

use setun_doom::dbt::SetunDbtEngine;

/// Doom RV32IM benchmark: covers RV32I plus RV32M with a nontrivial control flow.
const DOOM_BENCHMARK_ELF: &[u8] =
    include_bytes!("../vendor/riscv_tests/rv32m/doom_rv32_benchmark.elf");

const STEP_LIMIT: u64 = 5_000_000;

#[test]
fn test_fast_mode_matches_veracity_mode() {
    let mut veracity = SetunDbtEngine::<true>::from_elf_bytes(DOOM_BENCHMARK_ELF)
        .expect("load benchmark in veracity mode");
    let veracity_report = veracity.run(STEP_LIMIT).expect("veracity run");

    let mut fast =
        SetunDbtEngine::<false>::from_elf_bytes_fast(DOOM_BENCHMARK_ELF).expect("load benchmark in fast mode");
    let fast_report = fast.run(STEP_LIMIT).expect("fast run");

    assert_eq!(veracity_report.exit, fast_report.exit);
    assert_eq!(
        veracity_report.rv32i_instructions_executed,
        fast_report.rv32i_instructions_executed
    );
    assert_eq!(
        veracity_report.setun_instructions_executed,
        fast_report.setun_instructions_executed
    );
    assert_eq!(veracity.stdout_buffer, fast.stdout_buffer);
    assert_eq!(veracity.pc, fast.pc);
    assert_eq!(veracity.read_reg(10).as_i64(), fast.read_reg(10).as_i64());
    assert_eq!(veracity.read_reg(2).as_i64(), fast.read_reg(2).as_i64());
}

#[test]
fn test_fast_mode_records_no_switching_telemetry() {
    let mut fast =
        SetunDbtEngine::<false>::from_elf_bytes_fast(DOOM_BENCHMARK_ELF).expect("load benchmark in fast mode");
    let report = fast.run(STEP_LIMIT).expect("fast run");

    assert_eq!(report.telemetry.ternary_trit_flips, 0);
    assert_eq!(report.telemetry.ternary_half_swings, 0);
    assert_eq!(report.telemetry.ternary_full_swings, 0);
    assert_eq!(report.telemetry.binary32_bit_flips, 0);
    assert_eq!(report.telemetry.adder_carry_propagations, 0);
    assert!(report.rv32i_instructions_executed > 800_000);
}

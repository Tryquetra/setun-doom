//! External black-box integration tests for the Setun-Doom library.

use setun_doom::{
    dbt::{run_all_riscv_isa_tests, SetunDbtEngine},
    Cpu, Instruction, Memory, Opcode, TernaryFixed, Trit, Word21,
};

#[test]
fn test_integration_public_api_ternary_math() {
    let a = TernaryFixed::from_f64(1.75);
    let b = TernaryFixed::from_f64(-16.0);
    let prod = a * b;
    assert!((prod.to_f64() - (-28.0)).abs() < 1e-4);

    let w1 = Word21::new(12345);
    let w2 = Word21::new(-6789);
    let (sum, _cout, carries) = w1.gate_ripple_carry_add(w2, Trit::Zero);
    assert_eq!(sum.as_i64(), 12345 - 6789);
    assert!(carries > 0);
}

#[test]
fn test_integration_public_api_cpu_pipeline() {
    let mut mem = Memory::new();
    let mut cpu = Cpu::new();

    // Loads a small test program via direct instruction encoding
    let instr1 = Instruction::new(Opcode::LoadImm, 0, 777);
    let instr2 = Instruction::new(Opcode::Halt, 0, 0);

    mem.write_word(0, instr1.encode()).expect("write instr1");
    mem.write_word(1, instr2.encode()).expect("write instr2");

    let steps = cpu.run_ticks(&mut mem, 10);
    assert_eq!(steps, 1); // 1 step executed before HALT interrupts the loop
    assert!(cpu.halted);
    assert_eq!(cpu.acc.as_i64(), 777);
}

#[test]
fn test_integration_public_api_dbt_rv32im() {
    let summary = run_all_riscv_isa_tests().expect("Failed to execute official RV32I test suite");
    assert_eq!(summary.tests.len(), 37);
    for (name, report) in &summary.tests {
        assert!(report.is_riscv_test_pass(), "Test {} failed", name);
    }

    let doom_elf = include_bytes!("../vendor/riscv_tests/rv32m/doom_rv32_benchmark.elf");
    let mut engine = SetunDbtEngine::from_elf_bytes(doom_elf).expect("ELF load");
    let report = engine.run(5_000_000).expect("Doom benchmark run");
    assert!(report.is_riscv_test_pass());
    assert!(!report.stdout_buffer.is_empty());
}

// Copyright (c) 2026 Robson Cassiano (https://eu.robsoncassiano.software)
// SPDX-License-Identifier: MIT

//! Tradutor Binário Dinâmico (`DBT / JIT`) de `RV32I` (32 bits) para `Setun-Doom` (`21 trits`).
//!
//! Implementa o **Nível 1 de Emulação Ternária**:
//! 1. **Decodificador completo da ISA base `RV32I`** (todas as 37 instruções de usuário + sistema).
//! 2. **Banco de 32 Registradores `x0..x31` em [`Word21`]** mapeado na memória linear da Setun
//!    (`DBT_REGISTER_FILE_BASE .. +32`), onde `x0` é fisicamente aterrado em `Word21::ZERO`.
//! 3. **Compilador de Blocos Básicos com Fusão REBEL-6 / Setun**:
//!    - Fusão de constantes de 32 bits (`LUI + ADDI -> LI.T` e `AUIPC + ADDI -> AIPC.T`) em uma
//!      única operação de 21 trits (pois $[-2^{31}, 2^{31}-1] \subset [-5{,}23 \times 10^9, +5{,}23 \times 10^9]$).
//!    - Fusão de comparações e desvios (`SLT/SLTI/SLTU/SLTIU + BEQ/BNE x0 -> BCEG`) usando o
//!      comparador tricótomo de 21 portas [`Word21::gate_cmp`].
//! 4. **Execução Estrita em Nível de Portas Ternárias (Zero Falsos Positivos)**:
//!    - Todas as somas, subtrações, cálculos de endereço efetivo e reduções de faixa de 32 bits
//!      operam exclusivamente pelo somador Ripple-Carry de 21 estágios ([`Word21::gate_ripple_carry_add`])
//!      e pelo inversor físico ([`Word21::gate_invert`]).
//!    - Todas as comparações assinadas e não-assinadas operam pelo comparador ternário
//!      ([`Word21::gate_cmp`]).
//!    - Operações lógicas bit-a-bit de legado binário (`AND`, `OR`, `XOR`, `SLL`, `SRL`, `SRA`)
//!      atravessam o conversor de base em hardware de J. Ramil Alvarez (1974) na [`Cpu`],
//!      contabilizando todas as propagações físicas de carry.

use std::collections::{HashMap, HashSet};

use crate::cpu::{Cpu, Instruction, Opcode, PowerTelemetry};
use crate::elf::{
    read_elf_u16_le, read_elf_u32_le, read_elf_u8, write_elf_u8, Elf32Image, ElfError,
    LoadedElfLayout, DEFAULT_ELF_MEMORY_BASE, DEFAULT_RV32I_STACK_TOP,
};
use crate::memory::{Memory, MemoryError};
use crate::trit::Trit;
use crate::word::Word21;

/// Endereço base na memória Setun para o banco de 32 registradores `x0..x31` do `RV32I`.
pub const DBT_REGISTER_FILE_BASE: usize = 500;

/// Endereço base na memória Setun para o cache de instruções traduzidas `Word21`.
pub const DBT_CODE_CACHE_BASE: usize = 1_000;

/// Capacidade máxima do cache de código traduzido na memória Setun.
pub const DBT_CODE_CACHE_LIMIT: usize = 90_000;

/// Limite superior de `i32` (`+2.147.483.647`) representado em [`Word21`].
const MAX_I32_WORD21: Word21 = Word21::new(i32::MAX as i64);

/// Limite inferior de `i32` (`-2.147.483.648`) representado em [`Word21`].
const MIN_I32_WORD21: Word21 = Word21::new(i32::MIN as i64);

/// Módulo $2^{32} = +4.294.967.296$ representado em [`Word21`] (cabe folgadamente em $\pm 5{,}23 \times 10^9$).
const POS_MOD32_WORD21: Word21 = Word21::new(4_294_967_296);

/// Módulo negativo $-2^{32} = -4.294.967.296$ representado em [`Word21`].
const NEG_MOD32_WORD21: Word21 = Word21::new(-4_294_967_296);

/// Nomes padronizados ABI dos 32 registradores inteiros do `RV32I`.
pub const RV32I_REG_ABI_NAMES: [&str; 32] = [
    "zero", "ra", "sp", "gp", "tp", "t0", "t1", "t2", "s0", "s1", "a0", "a1", "a2", "a3", "a4",
    "a5", "a6", "a7", "s2", "s3", "s4", "s5", "s6", "s7", "s8", "s9", "s10", "s11", "t3", "t4",
    "t5", "t6",
];

/// Instrução individual da ISA `RV32I` decodificada a partir de uma palavra de 32 bits.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Rv32iInstr {
    Lui { rd: u8, imm: i32 },
    Auipc { rd: u8, imm: i32 },
    Jal { rd: u8, offset: i32 },
    Jalr { rd: u8, rs1: u8, imm: i32 },
    Beq { rs1: u8, rs2: u8, offset: i32 },
    Bne { rs1: u8, rs2: u8, offset: i32 },
    Blt { rs1: u8, rs2: u8, offset: i32 },
    Bge { rs1: u8, rs2: u8, offset: i32 },
    Bltu { rs1: u8, rs2: u8, offset: i32 },
    Bgeu { rs1: u8, rs2: u8, offset: i32 },
    Lb { rd: u8, rs1: u8, imm: i32 },
    Lh { rd: u8, rs1: u8, imm: i32 },
    Lw { rd: u8, rs1: u8, imm: i32 },
    Lbu { rd: u8, rs1: u8, imm: i32 },
    Lhu { rd: u8, rs1: u8, imm: i32 },
    Sb { rs1: u8, rs2: u8, imm: i32 },
    Sh { rs1: u8, rs2: u8, imm: i32 },
    Sw { rs1: u8, rs2: u8, imm: i32 },
    Addi { rd: u8, rs1: u8, imm: i32 },
    Slti { rd: u8, rs1: u8, imm: i32 },
    Sltiu { rd: u8, rs1: u8, imm: i32 },
    Xori { rd: u8, rs1: u8, imm: i32 },
    Ori { rd: u8, rs1: u8, imm: i32 },
    Andi { rd: u8, rs1: u8, imm: i32 },
    Slli { rd: u8, rs1: u8, shamt: u8 },
    Srli { rd: u8, rs1: u8, shamt: u8 },
    Srai { rd: u8, rs1: u8, shamt: u8 },
    Add { rd: u8, rs1: u8, rs2: u8 },
    Sub { rd: u8, rs1: u8, rs2: u8 },
    Sll { rd: u8, rs1: u8, rs2: u8 },
    Slt { rd: u8, rs1: u8, rs2: u8 },
    Sltu { rd: u8, rs1: u8, rs2: u8 },
    Xor { rd: u8, rs1: u8, rs2: u8 },
    Srl { rd: u8, rs1: u8, rs2: u8 },
    Sra { rd: u8, rs1: u8, rs2: u8 },
    Or { rd: u8, rs1: u8, rs2: u8 },
    And { rd: u8, rs1: u8, rs2: u8 },
    Fence,
    Ecall,
    Ebreak,
}

impl Rv32iInstr {
    /// Decodifica uma palavra binária de 32 bits Little-Endian em uma instrução [`Rv32iInstr`].
    pub fn decode(raw: u32) -> Result<Self, DbtError> {
        let opcode = raw & 0x7F;
        let rd = ((raw >> 7) & 0x1F) as u8;
        let funct3 = (raw >> 12) & 0x07;
        let rs1 = ((raw >> 15) & 0x1F) as u8;
        let rs2 = ((raw >> 20) & 0x1F) as u8;
        let funct7 = (raw >> 25) & 0x7F;

        match opcode {
            0x37 => {
                let imm = (raw & 0xFFFF_F000) as i32;
                Ok(Self::Lui { rd, imm })
            }
            0x17 => {
                let imm = (raw & 0xFFFF_F000) as i32;
                Ok(Self::Auipc { rd, imm })
            }
            0x6F => {
                let offset = decode_j_imm(raw);
                Ok(Self::Jal { rd, offset })
            }
            0x67 => {
                if funct3 != 0 {
                    return Err(DbtError::IllegalInstruction { raw, pc: 0 });
                }
                let imm = (raw as i32) >> 20;
                Ok(Self::Jalr { rd, rs1, imm })
            }
            0x63 => {
                let offset = decode_b_imm(raw);
                match funct3 {
                    0 => Ok(Self::Beq { rs1, rs2, offset }),
                    1 => Ok(Self::Bne { rs1, rs2, offset }),
                    4 => Ok(Self::Blt { rs1, rs2, offset }),
                    5 => Ok(Self::Bge { rs1, rs2, offset }),
                    6 => Ok(Self::Bltu { rs1, rs2, offset }),
                    7 => Ok(Self::Bgeu { rs1, rs2, offset }),
                    _ => Err(DbtError::IllegalInstruction { raw, pc: 0 }),
                }
            }
            0x03 => {
                let imm = (raw as i32) >> 20;
                match funct3 {
                    0 => Ok(Self::Lb { rd, rs1, imm }),
                    1 => Ok(Self::Lh { rd, rs1, imm }),
                    2 => Ok(Self::Lw { rd, rs1, imm }),
                    4 => Ok(Self::Lbu { rd, rs1, imm }),
                    5 => Ok(Self::Lhu { rd, rs1, imm }),
                    _ => Err(DbtError::IllegalInstruction { raw, pc: 0 }),
                }
            }
            0x23 => {
                let imm = decode_s_imm(raw);
                match funct3 {
                    0 => Ok(Self::Sb { rs1, rs2, imm }),
                    1 => Ok(Self::Sh { rs1, rs2, imm }),
                    2 => Ok(Self::Sw { rs1, rs2, imm }),
                    _ => Err(DbtError::IllegalInstruction { raw, pc: 0 }),
                }
            }
            0x13 => {
                let imm = (raw as i32) >> 20;
                let shamt = ((raw >> 20) & 0x1F) as u8;
                match funct3 {
                    0 => Ok(Self::Addi { rd, rs1, imm }),
                    2 => Ok(Self::Slti { rd, rs1, imm }),
                    3 => Ok(Self::Sltiu { rd, rs1, imm }),
                    4 => Ok(Self::Xori { rd, rs1, imm }),
                    6 => Ok(Self::Ori { rd, rs1, imm }),
                    7 => Ok(Self::Andi { rd, rs1, imm }),
                    1 => {
                        if funct7 != 0 {
                            return Err(DbtError::IllegalInstruction { raw, pc: 0 });
                        }
                        Ok(Self::Slli { rd, rs1, shamt })
                    }
                    5 => match funct7 {
                        0x00 => Ok(Self::Srli { rd, rs1, shamt }),
                        0x20 => Ok(Self::Srai { rd, rs1, shamt }),
                        _ => Err(DbtError::IllegalInstruction { raw, pc: 0 }),
                    },
                    _ => Err(DbtError::IllegalInstruction { raw, pc: 0 }),
                }
            }
            0x33 => match (funct3, funct7) {
                (0, 0x00) => Ok(Self::Add { rd, rs1, rs2 }),
                (0, 0x20) => Ok(Self::Sub { rd, rs1, rs2 }),
                (1, 0x00) => Ok(Self::Sll { rd, rs1, rs2 }),
                (2, 0x00) => Ok(Self::Slt { rd, rs1, rs2 }),
                (3, 0x00) => Ok(Self::Sltu { rd, rs1, rs2 }),
                (4, 0x00) => Ok(Self::Xor { rd, rs1, rs2 }),
                (5, 0x00) => Ok(Self::Srl { rd, rs1, rs2 }),
                (5, 0x20) => Ok(Self::Sra { rd, rs1, rs2 }),
                (6, 0x00) => Ok(Self::Or { rd, rs1, rs2 }),
                (7, 0x00) => Ok(Self::And { rd, rs1, rs2 }),
                _ => Err(DbtError::IllegalInstruction { raw, pc: 0 }),
            },
            0x0F => Ok(Self::Fence),
            0x73 => {
                if funct3 == 0 {
                    match raw >> 20 {
                        0 => Ok(Self::Ecall),
                        1 => Ok(Self::Ebreak),
                        _ => Ok(Self::Fence),
                    }
                } else {
                    // Instruções CSR (caso ocorram em prólogos bare-metal) tratadas como NOP
                    Ok(Self::Fence)
                }
            }
            _ => Err(DbtError::IllegalInstruction { raw, pc: 0 }),
        }
    }
}

/// Modo de comparação na instrução fundida tricótoma `BCEG` (`SLT* + BEQ/BNE`).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FusedCmpKind {
    /// Compara dois registradores com sinal (`SLT`).
    RegSigned { rs1: u8, rs2: u8 },
    /// Compara dois registradores sem sinal (`SLTU`).
    RegUnsigned { rs1: u8, rs2: u8 },
    /// Compara registrador e imediato com sinal (`SLTI`).
    ImmSigned { rs1: u8, imm: i32 },
    /// Compara registrador e imediato sem sinal (`SLTIU`).
    ImmUnsigned { rs1: u8, imm: i32 },
}

/// Operação traduzida para a máquina ternária `Setun-Doom` (`REBEL-6 / Setun ISA`).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SetunTranslatedOp {
    /// Carga direta de constante de 32 bits em 21 trits (`LI.T`), fundindo `LUI + ADDI`.
    LiT {
        rd: u8,
        imm32: i32,
        intermediate_hi20: Option<i32>,
        rv32i_instrs: u8,
    },
    /// Carga de endereço relativo ao PC em 21 trits (`AIPC.T`), fundindo `AUIPC + ADDI`.
    AipcT {
        rd: u8,
        target_addr: i32,
        intermediate_hi20: Option<i32>,
        rv32i_instrs: u8,
    },
    /// Salto condicional tricótomo fundido (`SLT/SLTI/SLTU/SLTIU [+ LI 0|1] + BEQ/BNE -> BCEG`).
    BcegFused {
        cmp_rd: u8,
        aux_li: Option<(u8, i32)>,
        kind: FusedCmpKind,
        branch_if_less: bool,
        target_pc: u32,
        fallthrough_pc: u32,
        rv32i_instrs: u8,
    },
    /// Instrução base mapeada 1-para-1 para a ALU ternária de 21 trits.
    Single { instr: Rv32iInstr, pc: u32 },
}

/// Bloco básico traduzido e armazenado no cache de tradução dinâmica.
#[derive(Clone, Debug)]
pub struct TranslatedBasicBlock {
    /// Endereço inicial do bloco no espaço virtual `RV32I`.
    pub start_pc: u32,
    /// Operações ternárias Setun geradas para este bloco.
    pub ops: Vec<SetunTranslatedOp>,
    /// Endereço base na memória Setun (`DBT_CODE_CACHE_BASE`) onde as `Instruction`s foram gravadas.
    pub setun_cache_addr: usize,
}

/// Erros de execução ou tradução do motor DBT.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DbtError {
    /// Erro ao carregar a imagem ELF32.
    Elf(ElfError),
    /// Erro de acesso à memória linear da Setun.
    Memory(MemoryError),
    /// Instrução binária desconhecida ou corrompida.
    IllegalInstruction { raw: u32, pc: u32 },
    /// Parada por `EBREAK`.
    Breakpoint { pc: u32 },
    /// Limite máximo de ciclos/instruções atingido (prevenção de loop infinito em testes).
    StepLimitExceeded(u64),
}

impl From<ElfError> for DbtError {
    fn from(e: ElfError) -> Self {
        Self::Elf(e)
    }
}

impl From<MemoryError> for DbtError {
    fn from(e: MemoryError) -> Self {
        Self::Memory(e)
    }
}

/// Motivo de término da execução de um binário `RV32I` traduzido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DbtExitReason {
    /// Encerrado via `ecall` (convenção `riscv-tests` ou syscall `exit`).
    Ecall {
        /// Código de saída em `a0` (`x10`, onde `0` = aprovação em `riscv-tests`).
        a0_exit_code: i32,
        /// Identificador do teste atual em `gp` (`x3`).
        gp_test_id: i32,
        /// Número da syscall em `a7` (`x17`).
        a7_syscall: i32,
    },
    /// Retorno da função `main` ao endereço sentinela `ra`.
    ReturnedFromMain { a0_return_value: i32 },
}

/// Relatório completo de execução e telemetria de um programa `RV32I` sobre a máquina `Setun-Doom`.
#[derive(Clone, Debug)]
pub struct DbtExecutionReport {
    /// Motivo de parada e códigos de retorno.
    pub exit: DbtExitReason,
    /// Total de instruções binárias `RV32I` originais equivalentes executadas.
    pub rv32i_instructions_executed: u64,
    /// Total de instruções ternárias `Setun-Doom` efetivamente executadas (após fusões `LI.T`, `AIPC.T`, `BCEG`).
    pub setun_instructions_executed: u64,
    /// Quantidade de fusões de imediatos (`LUI/AUIPC + ADDI`) executadas.
    pub immediate_fusions_executed: u64,
    /// Quantidade de fusões de saltos tricótomos (`SLT* + BEQ/BNE -> BCEG`) executadas.
    pub branch_fusions_executed: u64,
    /// Telemetria física de comutação de estados (*Trit Flips*, *Half/Full-Swings*, *Bit Flips* e *Gate Carries*).
    pub telemetry: PowerTelemetry,
    /// Saída capturada de `stdout` (syscall `write`).
    pub stdout_buffer: Vec<u8>,
}

impl DbtExecutionReport {
    /// Redução percentual de instruções executadas (`Setun` vs `RV32I`).
    pub fn instruction_reduction_pct(&self) -> f64 {
        if self.rv32i_instructions_executed == 0 {
            return 0.0;
        }
        let r = self.rv32i_instructions_executed as f64;
        let s = self.setun_instructions_executed as f64;
        ((s - r) / r) * 100.0
    }

    /// Verifica se o teste da suíte oficial do RISC-V passou (`a0 == 0` e `gp > 0`).
    pub fn is_riscv_test_pass(&self) -> bool {
        match self.exit {
            DbtExitReason::Ecall {
                a0_exit_code,
                gp_test_id,
                ..
            } => a0_exit_code == 0 && gp_test_id > 0,
            DbtExitReason::ReturnedFromMain { a0_return_value } => a0_return_value == 0,
        }
    }
}

/// Valor sentinela colocado em `ra` (`x1`) no início de `main` para detectar retorno limpo
/// (mesma técnica do simulador `REBEL-6`, onde apenas um único trit é ativado: $-3^{16} = -43.046.721$).
pub const SENTINEL_RETURN_PC: u32 = (-43_046_721i32) as u32;

/// Motor de Tradução Binária Dinâmica (`DBT`) que executa binários `ELF32` `RV32I` sobre a CPU e Memória Setun.
pub struct SetunDbtEngine {
    /// CPU ternária balanceada Setun-Doom (acumulador, registrador R, conversor Alvarez e telemetria).
    pub cpu: Cpu,
    /// Memória linear ternária de $3^{14} = 4.782.969$ palavras.
    pub memory: Memory,
    /// Banco de 32 registradores arquiteturais `x0..x31` representados em [`Word21`].
    pub regs: [Word21; 32],
    /// Contador de programa (`PC`) no espaço de endereçamento virtual `RV32I`.
    pub pc: u32,
    /// Layout da imagem ELF32 carregada na memória Setun.
    pub layout: LoadedElfLayout,
    /// Conjunto de endereços que são destinos estáticos de saltos/branches (para segurança de fusão).
    branch_targets: HashSet<u32>,
    /// Cache de blocos básicos já traduzidos (`start_pc -> TranslatedBasicBlock`).
    block_cache: HashMap<u32, TranslatedBasicBlock>,
    /// Próximo endereço livre na região `DBT_CODE_CACHE_BASE` da memória Setun.
    next_code_cache_addr: usize,
    /// Contadores de instruções e fusões.
    pub rv32i_instructions_executed: u64,
    pub setun_instructions_executed: u64,
    pub immediate_fusions_executed: u64,
    pub branch_fusions_executed: u64,
    /// Buffer de console para syscall `write`.
    pub stdout_buffer: Vec<u8>,
}

impl SetunDbtEngine {
    /// Cria uma nova instância do motor `SetunDbtEngine` e carrega um binário `ELF32` na memória ternária.
    pub fn from_elf_bytes(elf_bytes: &[u8]) -> Result<Self, DbtError> {
        let image = Elf32Image::parse(elf_bytes)?;
        let mut memory = Memory::new();
        let layout = image.load_into_setun_memory(&mut memory, DEFAULT_ELF_MEMORY_BASE)?;

        let branch_targets = scan_static_branch_targets(&memory, &layout, &image);

        let mut engine = Self {
            cpu: Cpu::new(),
            memory,
            regs: [Word21::ZERO; 32],
            pc: layout.entry_point,
            layout,
            branch_targets,
            block_cache: HashMap::new(),
            next_code_cache_addr: DBT_CODE_CACHE_BASE,
            rv32i_instructions_executed: 0,
            setun_instructions_executed: 0,
            immediate_fusions_executed: 0,
            branch_fusions_executed: 0,
            stdout_buffer: Vec::new(),
        };

        // Inicializa o Stack Pointer (sp = x2) e o Return Address sentinela (ra = x1)
        // sincronizados na memória Setun sem poluir a telemetria do programa de usuário.
        let sp_word = Word21::new(DEFAULT_RV32I_STACK_TOP as i64);
        let ra_word = Word21::new(SENTINEL_RETURN_PC as i32 as i64);
        engine.regs[2] = sp_word;
        engine.regs[1] = ra_word;
        for r in 0..32 {
            engine
                .memory
                .write_word((DBT_REGISTER_FILE_BASE + r) as i64, engine.regs[r])?;
        }

        Ok(engine)
    }

    /// Lê o valor de um registrador `x0..x31` como [`Word21`] (`x0` retorna sempre `Word21::ZERO`).
    #[inline(always)]
    pub fn read_reg(&self, reg: u8) -> Word21 {
        let idx = (reg & 0x1F) as usize;
        if idx == 0 {
            Word21::ZERO
        } else {
            self.regs[idx]
        }
    }

    /// Escreve uma [`Word21`] no registrador `rd` (`x1..x31`), atualiza a célula correspondente
    /// na memória Setun (`DBT_REGISTER_FILE_BASE + rd`) e contabiliza os *Trit Flips* vs *Bit Flips*.
    #[inline]
    pub fn write_reg(&mut self, rd: u8, new_val: Word21) -> Result<(), MemoryError> {
        let idx = (rd & 0x1F) as usize;
        if idx == 0 {
            return Ok(());
        }
        let old_val = self.regs[idx];
        self.regs[idx] = new_val;
        self.memory
            .write_word((DBT_REGISTER_FILE_BASE + idx) as i64, new_val)?;
        self.cpu.telemetry.record_transition(old_val, new_val);
        Ok(())
    }

    /// Converte um inteiro binário `i32` para [`Word21`] através do conversor em hardware
    /// de J. Ramil Alvarez (1974) (`Word21::alvarez_from_i32_gates`), acumulando todas as
    /// propagações de carry das portas `Trit::full_add`.
    #[inline]
    pub fn gate_alvarez_bin_to_ter(&mut self, val: i32) -> Word21 {
        let (trits, carries) = Word21::alvarez_from_i32_gates(val);
        self.cpu.telemetry.adder_carry_propagations += carries as u64;
        Word21::from_trits(&trits)
    }

    /// Extrai o valor de 32 bits em complemento de dois a partir de uma [`Word21`].
    #[inline(always)]
    pub fn gate_alvarez_ter_to_bin(&self, w: Word21) -> i32 {
        w.to_i32_saturating()
    }

    /// Soma duas palavras `Word21` usando o somador físico Ripple-Carry de 21 portas (`Trit::full_add`)
    /// e aplica o envelopamento de 32 bits (`[-2^31, 2^31 - 1]`) inteiramente via portas ternárias.
    #[inline]
    pub fn gate_add_i32(&mut self, a: Word21, b: Word21) -> Word21 {
        let (sum, _cout, carries) = a.gate_ripple_carry_add(b, Trit::Zero);
        self.cpu.telemetry.adder_carry_propagations += carries as u64;
        self.cpu.cycles += 1;

        if sum.gate_cmp_t(MAX_I32_WORD21) == Trit::Positive {
            let (wrapped, _c2, carries2) = sum.gate_ripple_carry_add(NEG_MOD32_WORD21, Trit::Zero);
            self.cpu.telemetry.adder_carry_propagations += carries2 as u64;
            wrapped
        } else if sum.gate_cmp_t(MIN_I32_WORD21) == Trit::Negative {
            let (wrapped, _c2, carries2) = sum.gate_ripple_carry_add(POS_MOD32_WORD21, Trit::Zero);
            self.cpu.telemetry.adder_carry_propagations += carries2 as u64;
            wrapped
        } else {
            sum
        }
    }

    /// Subtrai `b` de `a` invertendo os 21 fios de `b` (`Word21::gate_invert_trits`) e somando pelo
    /// somador Ripple-Carry de 21 estágios (`gate_add_i32`).
    #[inline]
    pub fn gate_sub_i32(&mut self, a: Word21, b: Word21) -> Word21 {
        let neg_b = b.gate_invert_trits();
        self.gate_add_i32(a, neg_b)
    }

    /// Converte uma `Word21` assinada (`[-2^31, 2^31 - 1]`) para sua magnitude não-assinada
    /// de 32 bits (`[0, 2^32 - 1]`) dentro de uma `Word21` usando o somador Ripple-Carry ternário.
    #[inline]
    pub fn gate_to_unsigned32_word21(&mut self, w: Word21) -> Word21 {
        if w.sign_trit() == Trit::Negative {
            let (pos, _c, carries) = w.gate_ripple_carry_add(POS_MOD32_WORD21, Trit::Zero);
            self.cpu.telemetry.adder_carry_propagations += carries as u64;
            pos
        } else {
            w
        }
    }

    /// Compara duas `Word21` como inteiros de 32 bits sem sinal usando o comparador de 21 portas `gate_cmp_t`.
    #[inline]
    pub fn gate_cmp_u32(&mut self, a: Word21, b: Word21) -> Trit {
        let ua = self.gate_to_unsigned32_word21(a);
        let ub = self.gate_to_unsigned32_word21(b);
        ua.gate_cmp_t(ub)
    }

    /// Traduz (ou recupera do cache) o bloco básico iniciando em `start_pc`.
    pub fn get_or_translate_block(
        &mut self,
        start_pc: u32,
    ) -> Result<TranslatedBasicBlock, DbtError> {
        if let Some(block) = self.block_cache.get(&start_pc) {
            return Ok(block.clone());
        }

        let mut ops = Vec::with_capacity(16);
        let mut cur_pc = start_pc;
        let cache_addr = self.next_code_cache_addr;

        for _ in 0..64 {
            let raw0 = read_elf_u32_le(&self.memory, self.layout.setun_base_offset, cur_pc)?;
            let instr0 = Rv32iInstr::decode(raw0).map_err(|_| DbtError::IllegalInstruction {
                raw: raw0,
                pc: cur_pc,
            })?;

            let next_pc = cur_pc.wrapping_add(4);
            let can_fuse_next = !self.branch_targets.contains(&next_pc);

            // 1. Tentativa de Fusão LUI + ADDI -> LI.T (Constante completa de 32 bits em 21 trits)
            if let Rv32iInstr::Lui { rd, imm: hi20 } = instr0 {
                if rd != 0 && can_fuse_next {
                    if let Ok(raw1) =
                        read_elf_u32_le(&self.memory, self.layout.setun_base_offset, next_pc)
                    {
                        if let Ok(Rv32iInstr::Addi {
                            rd: rd2,
                            rs1,
                            imm: lo12,
                        }) = Rv32iInstr::decode(raw1)
                        {
                            if rd2 == rd && rs1 == rd {
                                let full_imm = hi20.wrapping_add(lo12);
                                ops.push(SetunTranslatedOp::LiT {
                                    rd,
                                    imm32: full_imm,
                                    intermediate_hi20: Some(hi20),
                                    rv32i_instrs: 2,
                                });
                                self.emit_setun_code_word(Opcode::Load, rd as i64);
                                cur_pc = cur_pc.wrapping_add(8);
                                if self.branch_targets.contains(&cur_pc) {
                                    break;
                                }
                                continue;
                            }
                        }
                    }
                }
            }

            // 2. Tentativa de Fusão AUIPC + ADDI -> AIPC.T (Endereço relativo completo em 21 trits)
            if let Rv32iInstr::Auipc { rd, imm: hi20 } = instr0 {
                if rd != 0 && can_fuse_next {
                    if let Ok(raw1) =
                        read_elf_u32_le(&self.memory, self.layout.setun_base_offset, next_pc)
                    {
                        if let Ok(Rv32iInstr::Addi {
                            rd: rd2,
                            rs1,
                            imm: lo12,
                        }) = Rv32iInstr::decode(raw1)
                        {
                            if rd2 == rd && rs1 == rd {
                                let hi_addr = (cur_pc as i32).wrapping_add(hi20);
                                let full_addr = hi_addr.wrapping_add(lo12);
                                ops.push(SetunTranslatedOp::AipcT {
                                    rd,
                                    target_addr: full_addr,
                                    intermediate_hi20: Some(hi_addr),
                                    rv32i_instrs: 2,
                                });
                                self.emit_setun_code_word(Opcode::Load, rd as i64);
                                cur_pc = cur_pc.wrapping_add(8);
                                if self.branch_targets.contains(&cur_pc) {
                                    break;
                                }
                                continue;
                            }
                        }
                    }
                }
            }

            // 3. Tentativa de Fusão Tricótoma SLT/SLTI/SLTU/SLTIU + [LI 0|1] + BEQ/BNE -> BCEG
            let slt_candidate = match instr0 {
                Rv32iInstr::Slt { rd, rs1, rs2 } if rd != 0 => {
                    Some((rd, FusedCmpKind::RegSigned { rs1, rs2 }))
                }
                Rv32iInstr::Sltu { rd, rs1, rs2 } if rd != 0 => {
                    Some((rd, FusedCmpKind::RegUnsigned { rs1, rs2 }))
                }
                Rv32iInstr::Slti { rd, rs1, imm } if rd != 0 => {
                    Some((rd, FusedCmpKind::ImmSigned { rs1, imm }))
                }
                Rv32iInstr::Sltiu { rd, rs1, imm } if rd != 0 => {
                    Some((rd, FusedCmpKind::ImmUnsigned { rs1, imm }))
                }
                _ => None,
            };

            if let Some((cmp_rd, kind)) = slt_candidate {
                if can_fuse_next {
                    if let Ok(raw1) =
                        read_elf_u32_le(&self.memory, self.layout.setun_base_offset, next_pc)
                    {
                        if let Ok(instr1) = Rv32iInstr::decode(raw1) {
                            // Caso 3a: Janela de 2 instruções (SLT* + BEQ/BNE cmp_rd, x0, offset)
                            let branch_match_2 = match instr1 {
                                Rv32iInstr::Bne { rs1, rs2, offset }
                                    if (rs1 == cmp_rd && rs2 == 0)
                                        || (rs1 == 0 && rs2 == cmp_rd) =>
                                {
                                    Some((true, offset))
                                }
                                Rv32iInstr::Beq { rs1, rs2, offset }
                                    if (rs1 == cmp_rd && rs2 == 0)
                                        || (rs1 == 0 && rs2 == cmp_rd) =>
                                {
                                    Some((false, offset))
                                }
                                _ => None,
                            };

                            if let Some((branch_if_less, offset)) = branch_match_2 {
                                let target_pc = (next_pc as i32).wrapping_add(offset) as u32;
                                let fallthrough_pc = cur_pc.wrapping_add(8);
                                ops.push(SetunTranslatedOp::BcegFused {
                                    cmp_rd,
                                    kind,
                                    aux_li: None,
                                    branch_if_less,
                                    target_pc,
                                    fallthrough_pc,
                                    rv32i_instrs: 2,
                                });
                                self.emit_setun_code_word(Opcode::Bceg, offset as i64);
                                break;
                            }

                            // Caso 3b: Janela de 3 instruções (SLT* + LI r_exp, 0|1 + BEQ/BNE cmp_rd, r_exp, offset)
                            if let Rv32iInstr::Addi {
                                rd: exp_rd,
                                rs1: 0,
                                imm: exp_imm @ (0 | 1),
                            } = instr1
                            {
                                let third_pc = cur_pc.wrapping_add(8);
                                if exp_rd != 0
                                    && exp_rd != cmp_rd
                                    && !self.branch_targets.contains(&third_pc)
                                {
                                    if let Ok(raw2) = read_elf_u32_le(
                                        &self.memory,
                                        self.layout.setun_base_offset,
                                        third_pc,
                                    ) {
                                        if let Ok(instr2) = Rv32iInstr::decode(raw2) {
                                            let branch_match_3 = match instr2 {
                                                Rv32iInstr::Bne { rs1, rs2, offset }
                                                    if (rs1 == cmp_rd && rs2 == exp_rd)
                                                        || (rs1 == exp_rd && rs2 == cmp_rd) =>
                                                {
                                                    // BNE salta quando cmp_rd != exp_imm:
                                                    // se exp_imm == 0, salta quando cmp_rd == 1 (is_less == true)
                                                    // se exp_imm == 1, salta quando cmp_rd == 0 (is_less == false)
                                                    Some((exp_imm == 0, offset))
                                                }
                                                Rv32iInstr::Beq { rs1, rs2, offset }
                                                    if (rs1 == cmp_rd && rs2 == exp_rd)
                                                        || (rs1 == exp_rd && rs2 == cmp_rd) =>
                                                {
                                                    // BEQ salta quando cmp_rd == exp_imm:
                                                    // se exp_imm == 1, salta quando cmp_rd == 1 (is_less == true)
                                                    // se exp_imm == 0, salta quando cmp_rd == 0 (is_less == false)
                                                    Some((exp_imm == 1, offset))
                                                }
                                                _ => None,
                                            };

                                            if let Some((branch_if_less, offset)) = branch_match_3 {
                                                let target_pc =
                                                    (third_pc as i32).wrapping_add(offset) as u32;
                                                let fallthrough_pc = cur_pc.wrapping_add(12);
                                                ops.push(SetunTranslatedOp::BcegFused {
                                                    cmp_rd,
                                                    kind,
                                                    aux_li: Some((exp_rd, exp_imm)),
                                                    branch_if_less,
                                                    target_pc,
                                                    fallthrough_pc,
                                                    rv32i_instrs: 3,
                                                });
                                                self.emit_setun_code_word(
                                                    Opcode::Bceg,
                                                    offset as i64,
                                                );
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // 4. Tradução direta 1-para-1 para operação ternária da Setun
            ops.push(SetunTranslatedOp::Single {
                instr: instr0,
                pc: cur_pc,
            });
            self.emit_setun_code_word(map_rv32i_to_setun_opcode(&instr0), cur_pc as i64);

            let is_terminator = matches!(
                instr0,
                Rv32iInstr::Jal { .. }
                    | Rv32iInstr::Jalr { .. }
                    | Rv32iInstr::Beq { .. }
                    | Rv32iInstr::Bne { .. }
                    | Rv32iInstr::Blt { .. }
                    | Rv32iInstr::Bge { .. }
                    | Rv32iInstr::Bltu { .. }
                    | Rv32iInstr::Bgeu { .. }
                    | Rv32iInstr::Ecall
                    | Rv32iInstr::Ebreak
            );

            cur_pc = next_pc;
            if is_terminator || self.branch_targets.contains(&cur_pc) {
                break;
            }
        }

        let block = TranslatedBasicBlock {
            start_pc,
            ops,
            setun_cache_addr: cache_addr,
        };
        self.block_cache.insert(start_pc, block.clone());
        Ok(block)
    }

    /// Emite uma instrução codificada de 21 trits na região `DBT_CODE_CACHE_BASE` da memória Setun.
    fn emit_setun_code_word(&mut self, opcode: Opcode, operand: i64) {
        if self.next_code_cache_addr < DBT_CODE_CACHE_LIMIT {
            let clamped = operand.clamp(-7_000_000, 7_000_000);
            let instr = Instruction::new(opcode, 0, clamped);
            let _ = self
                .memory
                .write_word(self.next_code_cache_addr as i64, instr.encode());
            self.next_code_cache_addr += 1;
        }
    }

    /// Executa o programa `RV32I` traduzido até encontrar uma chamada de saída (`ecall` / retorno de `main`)
    /// ou atingir `max_steps`.
    pub fn run(&mut self, max_steps: u64) -> Result<DbtExecutionReport, DbtError> {
        while self.setun_instructions_executed < max_steps {
            if self.pc == SENTINEL_RETURN_PC {
                let a0_val = self.read_reg(10).to_i32_saturating();
                return Ok(self.build_report(DbtExitReason::ReturnedFromMain {
                    a0_return_value: a0_val,
                }));
            }

            let block = self.get_or_translate_block(self.pc)?;

            for op in &block.ops {
                match *op {
                    SetunTranslatedOp::LiT {
                        rd,
                        imm32,
                        intermediate_hi20,
                        rv32i_instrs,
                    } => {
                        // Na referência binária RV32I sem fusão, LUI grava hi20 e ADDI grava imm32.
                        // Na Setun (LI.T), gravamos diretamente a Word21 de 21 trits em um único ciclo!
                        if let Some(hi20) = intermediate_hi20 {
                            if rd != 0 {
                                let old_bin = self.read_reg(rd).to_i32_saturating();
                                let extra_flips =
                                    ((old_bin ^ hi20).count_ones() + (hi20 ^ imm32).count_ones())
                                        as u64;
                                let direct_flips = (old_bin ^ imm32).count_ones() as u64;
                                self.cpu.telemetry.binary32_bit_flips +=
                                    extra_flips.saturating_sub(direct_flips);
                            }
                        }
                        self.write_reg(rd, Word21::from_i32(imm32))?;
                        self.rv32i_instructions_executed += rv32i_instrs as u64;
                        self.setun_instructions_executed += 1;
                        self.immediate_fusions_executed += 1;
                        self.cpu.cycles += 1;
                        self.pc = self.pc.wrapping_add((rv32i_instrs as u32) * 4);
                    }

                    SetunTranslatedOp::AipcT {
                        rd,
                        target_addr,
                        intermediate_hi20,
                        rv32i_instrs,
                    } => {
                        if let Some(hi20) = intermediate_hi20 {
                            if rd != 0 {
                                let old_bin = self.read_reg(rd).to_i32_saturating();
                                let extra_flips = ((old_bin ^ hi20).count_ones()
                                    + (hi20 ^ target_addr).count_ones())
                                    as u64;
                                let direct_flips = (old_bin ^ target_addr).count_ones() as u64;
                                self.cpu.telemetry.binary32_bit_flips +=
                                    extra_flips.saturating_sub(direct_flips);
                            }
                        }
                        self.write_reg(rd, Word21::from_i32(target_addr))?;
                        self.rv32i_instructions_executed += rv32i_instrs as u64;
                        self.setun_instructions_executed += 1;
                        self.immediate_fusions_executed += 1;
                        self.cpu.cycles += 1;
                        self.pc = self.pc.wrapping_add((rv32i_instrs as u32) * 4);
                    }

                    SetunTranslatedOp::BcegFused {
                        cmp_rd,
                        kind,
                        aux_li,
                        branch_if_less,
                        target_pc,
                        fallthrough_pc,
                        rv32i_instrs,
                    } => {
                        let cmp_trit = match kind {
                            FusedCmpKind::RegSigned { rs1, rs2 } => {
                                let a = self.read_reg(rs1);
                                let b = self.read_reg(rs2);
                                a.gate_cmp_t(b)
                            }
                            FusedCmpKind::RegUnsigned { rs1, rs2 } => {
                                let a = self.read_reg(rs1);
                                let b = self.read_reg(rs2);
                                self.gate_cmp_u32(a, b)
                            }
                            FusedCmpKind::ImmSigned { rs1, imm } => {
                                let a = self.read_reg(rs1);
                                let b = Word21::from_i32(imm);
                                a.gate_cmp_t(b)
                            }
                            FusedCmpKind::ImmUnsigned { rs1, imm } => {
                                let a = self.read_reg(rs1);
                                let b = Word21::from_i32(imm);
                                self.gate_cmp_u32(a, b)
                            }
                        };

                        let is_less = cmp_trit == Trit::Negative;
                        let slt_val = if is_less {
                            Word21::new(1)
                        } else {
                            Word21::ZERO
                        };
                        self.write_reg(cmp_rd, slt_val)?;
                        if let Some((exp_rd, exp_imm)) = aux_li {
                            self.write_reg(exp_rd, Word21::from_i32(exp_imm))?;
                        }

                        self.rv32i_instructions_executed += rv32i_instrs as u64;
                        self.setun_instructions_executed += 1;
                        self.branch_fusions_executed += 1;
                        self.cpu.cycles += 1;

                        let take_branch = if branch_if_less { is_less } else { !is_less };
                        self.pc = if take_branch {
                            target_pc
                        } else {
                            fallthrough_pc
                        };
                    }

                    SetunTranslatedOp::Single { instr, pc } => {
                        self.pc = pc;
                        if let Some(exit_reason) = self.execute_single_instr(instr, pc)? {
                            return Ok(self.build_report(exit_reason));
                        }
                    }
                }
            }
        }

        Err(DbtError::StepLimitExceeded(max_steps))
    }

    /// Executa uma única instrução traduzida usando os circuitos de portas ternárias da Setun.
    fn execute_single_instr(
        &mut self,
        instr: Rv32iInstr,
        pc: u32,
    ) -> Result<Option<DbtExitReason>, DbtError> {
        self.rv32i_instructions_executed += 1;
        self.setun_instructions_executed += 1;
        self.cpu.cycles += 1;

        let mut next_pc = pc.wrapping_add(4);

        match instr {
            Rv32iInstr::Lui { rd, imm } => {
                self.write_reg(rd, Word21::from_i32(imm))?;
            }
            Rv32iInstr::Auipc { rd, imm } => {
                let res = self.gate_add_i32(Word21::from_i32(pc as i32), Word21::from_i32(imm));
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Jal { rd, offset } => {
                let link = self.gate_add_i32(Word21::from_i32(pc as i32), Word21::new(4));
                let target =
                    self.gate_add_i32(Word21::from_i32(pc as i32), Word21::from_i32(offset));
                self.write_reg(rd, link)?;
                next_pc = target.to_i32_saturating() as u32;
            }
            Rv32iInstr::Jalr { rd, rs1, imm } => {
                // Calcula o endereço alvo a partir de rs1 ANTES de sobrescrever rd (caso rd == rs1)
                let base = self.read_reg(rs1);
                let target_sum = self.gate_add_i32(base, Word21::from_i32(imm));
                let target_u32 = (target_sum.to_i32_saturating() as u32) & !1u32;
                let link = self.gate_add_i32(Word21::from_i32(pc as i32), Word21::new(4));
                self.write_reg(rd, link)?;
                next_pc = target_u32;
            }
            Rv32iInstr::Beq { rs1, rs2, offset } => {
                let a = self.read_reg(rs1);
                let b = self.read_reg(rs2);
                if a.gate_cmp_t(b) == Trit::Zero {
                    let target =
                        self.gate_add_i32(Word21::from_i32(pc as i32), Word21::from_i32(offset));
                    next_pc = target.to_i32_saturating() as u32;
                }
            }
            Rv32iInstr::Bne { rs1, rs2, offset } => {
                let a = self.read_reg(rs1);
                let b = self.read_reg(rs2);
                if a.gate_cmp_t(b) != Trit::Zero {
                    let target =
                        self.gate_add_i32(Word21::from_i32(pc as i32), Word21::from_i32(offset));
                    next_pc = target.to_i32_saturating() as u32;
                }
            }
            Rv32iInstr::Blt { rs1, rs2, offset } => {
                let a = self.read_reg(rs1);
                let b = self.read_reg(rs2);
                if a.gate_cmp_t(b) == Trit::Negative {
                    let target =
                        self.gate_add_i32(Word21::from_i32(pc as i32), Word21::from_i32(offset));
                    next_pc = target.to_i32_saturating() as u32;
                }
            }
            Rv32iInstr::Bge { rs1, rs2, offset } => {
                let a = self.read_reg(rs1);
                let b = self.read_reg(rs2);
                if a.gate_cmp_t(b) != Trit::Negative {
                    let target =
                        self.gate_add_i32(Word21::from_i32(pc as i32), Word21::from_i32(offset));
                    next_pc = target.to_i32_saturating() as u32;
                }
            }
            Rv32iInstr::Bltu { rs1, rs2, offset } => {
                let a = self.read_reg(rs1);
                let b = self.read_reg(rs2);
                if self.gate_cmp_u32(a, b) == Trit::Negative {
                    let target =
                        self.gate_add_i32(Word21::from_i32(pc as i32), Word21::from_i32(offset));
                    next_pc = target.to_i32_saturating() as u32;
                }
            }
            Rv32iInstr::Bgeu { rs1, rs2, offset } => {
                let a = self.read_reg(rs1);
                let b = self.read_reg(rs2);
                if self.gate_cmp_u32(a, b) != Trit::Negative {
                    let target =
                        self.gate_add_i32(Word21::from_i32(pc as i32), Word21::from_i32(offset));
                    next_pc = target.to_i32_saturating() as u32;
                }
            }
            Rv32iInstr::Lb { rd, rs1, imm } => {
                let addr_w = self.gate_add_i32(self.read_reg(rs1), Word21::from_i32(imm));
                let vaddr = addr_w.to_i32_saturating() as u32;
                let b = read_elf_u8(&self.memory, self.layout.setun_base_offset, vaddr)? as i8;
                let w = self.gate_alvarez_bin_to_ter(b as i32);
                self.write_reg(rd, w)?;
            }
            Rv32iInstr::Lbu { rd, rs1, imm } => {
                let addr_w = self.gate_add_i32(self.read_reg(rs1), Word21::from_i32(imm));
                let vaddr = addr_w.to_i32_saturating() as u32;
                let b = read_elf_u8(&self.memory, self.layout.setun_base_offset, vaddr)?;
                let w = self.gate_alvarez_bin_to_ter(b as i32);
                self.write_reg(rd, w)?;
            }
            Rv32iInstr::Lh { rd, rs1, imm } => {
                let addr_w = self.gate_add_i32(self.read_reg(rs1), Word21::from_i32(imm));
                let vaddr = addr_w.to_i32_saturating() as u32;
                let h = read_elf_u16_le(&self.memory, self.layout.setun_base_offset, vaddr)? as i16;
                let w = self.gate_alvarez_bin_to_ter(h as i32);
                self.write_reg(rd, w)?;
            }
            Rv32iInstr::Lhu { rd, rs1, imm } => {
                let addr_w = self.gate_add_i32(self.read_reg(rs1), Word21::from_i32(imm));
                let vaddr = addr_w.to_i32_saturating() as u32;
                let h = read_elf_u16_le(&self.memory, self.layout.setun_base_offset, vaddr)?;
                let w = self.gate_alvarez_bin_to_ter(h as i32);
                self.write_reg(rd, w)?;
            }
            Rv32iInstr::Lw { rd, rs1, imm } => {
                let addr_w = self.gate_add_i32(self.read_reg(rs1), Word21::from_i32(imm));
                let vaddr = addr_w.to_i32_saturating() as u32;
                let val =
                    read_elf_u32_le(&self.memory, self.layout.setun_base_offset, vaddr)? as i32;
                let w = self.gate_alvarez_bin_to_ter(val);
                self.write_reg(rd, w)?;
            }
            Rv32iInstr::Sb { rs1, rs2, imm } => {
                let addr_w = self.gate_add_i32(self.read_reg(rs1), Word21::from_i32(imm));
                let vaddr = addr_w.to_i32_saturating() as u32;
                let val_i32 = self.gate_alvarez_ter_to_bin(self.read_reg(rs2));
                write_elf_u8(
                    &mut self.memory,
                    self.layout.setun_base_offset,
                    vaddr,
                    val_i32 as u8,
                )?;
            }
            Rv32iInstr::Sh { rs1, rs2, imm } => {
                let addr_w = self.gate_add_i32(self.read_reg(rs1), Word21::from_i32(imm));
                let vaddr = addr_w.to_i32_saturating() as u32;
                let val_i32 = self.gate_alvarez_ter_to_bin(self.read_reg(rs2));
                let bytes = (val_i32 as u16).to_le_bytes();
                write_elf_u8(
                    &mut self.memory,
                    self.layout.setun_base_offset,
                    vaddr,
                    bytes[0],
                )?;
                write_elf_u8(
                    &mut self.memory,
                    self.layout.setun_base_offset,
                    vaddr.wrapping_add(1),
                    bytes[1],
                )?;
            }
            Rv32iInstr::Sw { rs1, rs2, imm } => {
                let addr_w = self.gate_add_i32(self.read_reg(rs1), Word21::from_i32(imm));
                let vaddr = addr_w.to_i32_saturating() as u32;
                let val_i32 = self.gate_alvarez_ter_to_bin(self.read_reg(rs2));
                let bytes = (val_i32 as u32).to_le_bytes();
                for i in 0..4 {
                    write_elf_u8(
                        &mut self.memory,
                        self.layout.setun_base_offset,
                        vaddr.wrapping_add(i),
                        bytes[i as usize],
                    )?;
                }
            }
            Rv32iInstr::Addi { rd, rs1, imm } => {
                let res = self.gate_add_i32(self.read_reg(rs1), Word21::from_i32(imm));
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Slti { rd, rs1, imm } => {
                let a = self.read_reg(rs1);
                let b = Word21::from_i32(imm);
                let res = if a.gate_cmp_t(b) == Trit::Negative {
                    Word21::new(1)
                } else {
                    Word21::ZERO
                };
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Sltiu { rd, rs1, imm } => {
                let a = self.read_reg(rs1);
                let b = Word21::from_i32(imm);
                let res = if self.gate_cmp_u32(a, b) == Trit::Negative {
                    Word21::new(1)
                } else {
                    Word21::ZERO
                };
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Xori { rd, rs1, imm } => {
                let a_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs1));
                let res = self.gate_alvarez_bin_to_ter(a_bin ^ imm);
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Ori { rd, rs1, imm } => {
                let a_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs1));
                let res = self.gate_alvarez_bin_to_ter(a_bin | imm);
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Andi { rd, rs1, imm } => {
                let a_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs1));
                let res = self.gate_alvarez_bin_to_ter(a_bin & imm);
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Slli { rd, rs1, shamt } => {
                let a_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs1));
                let res =
                    self.gate_alvarez_bin_to_ter(((a_bin as u32) << (shamt & 0x1F)) as i32);
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Srli { rd, rs1, shamt } => {
                let a_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs1));
                let res =
                    self.gate_alvarez_bin_to_ter(((a_bin as u32) >> (shamt & 0x1F)) as i32);
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Srai { rd, rs1, shamt } => {
                let a_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs1));
                let res = self.gate_alvarez_bin_to_ter(a_bin >> (shamt & 0x1F));
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Add { rd, rs1, rs2 } => {
                let res = self.gate_add_i32(self.read_reg(rs1), self.read_reg(rs2));
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Sub { rd, rs1, rs2 } => {
                let res = self.gate_sub_i32(self.read_reg(rs1), self.read_reg(rs2));
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Sll { rd, rs1, rs2 } => {
                let a_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs1));
                let b_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs2)) as u32 & 0x1F;
                let res = self.gate_alvarez_bin_to_ter(((a_bin as u32) << b_bin) as i32);
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Slt { rd, rs1, rs2 } => {
                let a = self.read_reg(rs1);
                let b = self.read_reg(rs2);
                let res = if a.gate_cmp_t(b) == Trit::Negative {
                    Word21::new(1)
                } else {
                    Word21::ZERO
                };
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Sltu { rd, rs1, rs2 } => {
                let a = self.read_reg(rs1);
                let b = self.read_reg(rs2);
                let res = if self.gate_cmp_u32(a, b) == Trit::Negative {
                    Word21::new(1)
                } else {
                    Word21::ZERO
                };
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Xor { rd, rs1, rs2 } => {
                let a_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs1));
                let b_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs2));
                let res = self.gate_alvarez_bin_to_ter(a_bin ^ b_bin);
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Srl { rd, rs1, rs2 } => {
                let a_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs1));
                let b_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs2)) as u32 & 0x1F;
                let res = self.gate_alvarez_bin_to_ter(((a_bin as u32) >> b_bin) as i32);
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Sra { rd, rs1, rs2 } => {
                let a_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs1));
                let b_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs2)) as u32 & 0x1F;
                let res = self.gate_alvarez_bin_to_ter(a_bin >> b_bin);
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Or { rd, rs1, rs2 } => {
                let a_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs1));
                let b_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs2));
                let res = self.gate_alvarez_bin_to_ter(a_bin | b_bin);
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::And { rd, rs1, rs2 } => {
                let a_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs1));
                let b_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs2));
                let res = self.gate_alvarez_bin_to_ter(a_bin & b_bin);
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Fence => {}
            Rv32iInstr::Ecall => {
                let a0 = self.read_reg(10).to_i32_saturating();
                let a1 = self.read_reg(11).to_i32_saturating() as u32;
                let a2 = self.read_reg(12).to_i32_saturating() as u32;
                let a7 = self.read_reg(17).to_i32_saturating();
                let gp = self.read_reg(3).to_i32_saturating();

                // Syscall Linux/Newlib write(fd=a0, buf=a1, count=a2) quando a7 == 64
                if a7 == 64 {
                    for i in 0..a2 {
                        if let Ok(b) = read_elf_u8(
                            &self.memory,
                            self.layout.setun_base_offset,
                            a1.wrapping_add(i),
                        ) {
                            self.stdout_buffer.push(b);
                        }
                    }
                    self.write_reg(10, Word21::from_i32(a2 as i32))?;
                } else {
                    // Convenção riscv-tests ou exit(a0)
                    self.pc = next_pc;
                    return Ok(Some(DbtExitReason::Ecall {
                        a0_exit_code: a0,
                        gp_test_id: gp,
                        a7_syscall: a7,
                    }));
                }
            }
            Rv32iInstr::Ebreak => {
                return Err(DbtError::Breakpoint { pc });
            }
        }

        self.pc = next_pc;
        Ok(None)
    }

    fn build_report(&self, exit: DbtExitReason) -> DbtExecutionReport {
        DbtExecutionReport {
            exit,
            rv32i_instructions_executed: self.rv32i_instructions_executed,
            setun_instructions_executed: self.setun_instructions_executed,
            immediate_fusions_executed: self.immediate_fusions_executed,
            branch_fusions_executed: self.branch_fusions_executed,
            telemetry: self.cpu.telemetry,
            stdout_buffer: self.stdout_buffer.clone(),
        }
    }
}

/// Varre as faixas executáveis da imagem ELF para descobrir todos os destinos estáticos de desvios e saltos.
fn scan_static_branch_targets(
    memory: &Memory,
    layout: &LoadedElfLayout,
    image: &Elf32Image,
) -> HashSet<u32> {
    let mut targets = HashSet::new();
    targets.insert(layout.entry_point);
    for &sym_addr in image.symbols.values() {
        targets.insert(sym_addr);
    }

    for &(start, end) in &layout.executable_ranges {
        let mut pc = start;
        while pc + 4 <= end {
            if let Ok(raw) = read_elf_u32_le(memory, layout.setun_base_offset, pc) {
                if let Ok(instr) = Rv32iInstr::decode(raw) {
                    match instr {
                        Rv32iInstr::Jal { offset, .. }
                        | Rv32iInstr::Beq { offset, .. }
                        | Rv32iInstr::Bne { offset, .. }
                        | Rv32iInstr::Blt { offset, .. }
                        | Rv32iInstr::Bge { offset, .. }
                        | Rv32iInstr::Bltu { offset, .. }
                        | Rv32iInstr::Bgeu { offset, .. } => {
                            let target = (pc as i32).wrapping_add(offset) as u32;
                            targets.insert(target);
                        }
                        _ => {}
                    }
                }
            }
            pc = pc.wrapping_add(4);
        }
    }
    targets
}

/// Mapeia uma instrução `RV32I` para o [`Opcode`] correspondente na ISA de 21 trits da Setun.
fn map_rv32i_to_setun_opcode(instr: &Rv32iInstr) -> Opcode {
    match instr {
        Rv32iInstr::Lui { .. } | Rv32iInstr::Auipc { .. } => Opcode::Load,
        Rv32iInstr::Jal { .. } | Rv32iInstr::Jalr { .. } => Opcode::Call,
        Rv32iInstr::Beq { .. } | Rv32iInstr::Bne { .. } => Opcode::JmpZ,
        Rv32iInstr::Blt { .. } | Rv32iInstr::Bltu { .. } => Opcode::JmpN,
        Rv32iInstr::Bge { .. } | Rv32iInstr::Bgeu { .. } => Opcode::JmpP,
        Rv32iInstr::Lb { .. }
        | Rv32iInstr::Lbu { .. }
        | Rv32iInstr::Lh { .. }
        | Rv32iInstr::Lhu { .. }
        | Rv32iInstr::Lw { .. } => Opcode::Load,
        Rv32iInstr::Sb { .. } | Rv32iInstr::Sh { .. } | Rv32iInstr::Sw { .. } => Opcode::Store,
        Rv32iInstr::Addi { .. } | Rv32iInstr::Add { .. } => Opcode::Add,
        Rv32iInstr::Sub { .. } => Opcode::Sub,
        Rv32iInstr::Slti { .. }
        | Rv32iInstr::Sltiu { .. }
        | Rv32iInstr::Slt { .. }
        | Rv32iInstr::Sltu { .. } => Opcode::CmpT,
        Rv32iInstr::Slli { .. }
        | Rv32iInstr::Sll { .. }
        | Rv32iInstr::Srli { .. }
        | Rv32iInstr::Srai { .. }
        | Rv32iInstr::Srl { .. }
        | Rv32iInstr::Sra { .. } => Opcode::Shift,
        Rv32iInstr::Xori { .. } | Rv32iInstr::Xor { .. } => Opcode::Smt,
        Rv32iInstr::Ori { .. } | Rv32iInstr::Or { .. } => Opcode::Sat,
        Rv32iInstr::Andi { .. } | Rv32iInstr::And { .. } => Opcode::Sit,
        Rv32iInstr::Fence => Opcode::Nop,
        Rv32iInstr::Ecall | Rv32iInstr::Ebreak => Opcode::Halt,
    }
}

#[inline]
fn decode_s_imm(raw: u32) -> i32 {
    let imm4_0 = (raw >> 7) & 0x1F;
    let imm11_5 = (raw >> 25) & 0x7F;
    let imm12 = (imm11_5 << 5) | imm4_0;
    ((imm12 << 20) as i32) >> 20
}

#[inline]
fn decode_b_imm(raw: u32) -> i32 {
    let bit11 = (raw >> 7) & 0x01;
    let bits4_1 = (raw >> 8) & 0x0F;
    let bits10_5 = (raw >> 25) & 0x3F;
    let bit12 = (raw >> 31) & 0x01;
    let imm13 = (bit12 << 12) | (bit11 << 11) | (bits10_5 << 5) | (bits4_1 << 1);
    ((imm13 << 19) as i32) >> 19
}

#[inline]
fn decode_j_imm(raw: u32) -> i32 {
    let bits19_12 = (raw >> 12) & 0xFF;
    let bit11 = (raw >> 20) & 0x01;
    let bits10_1 = (raw >> 21) & 0x3FF;
    let bit20 = (raw >> 31) & 0x01;
    let imm21 = (bit20 << 20) | (bits19_12 << 12) | (bit11 << 11) | (bits10_1 << 1);
    ((imm21 << 11) as i32) >> 11
}

/// Resumo consolidado da execução de toda a suíte oficial de 37 microtestes `RV32I` sobre a Setun-Doom.
#[derive(Clone, Debug)]
pub struct RiscvIsaSuiteSummary {
    /// Resultados individuais dos 37 testes (`(nome_do_teste, DbtExecutionReport)`).
    pub tests: Vec<(&'static str, DbtExecutionReport)>,
    /// Total de instruções binárias `RV32I` equivalentes na suíte.
    pub total_rv32i_instrs: u64,
    /// Total de instruções ternárias `Setun-Doom` executadas na suíte.
    pub total_setun_instrs: u64,
    /// Total de fusões `LI.T` e `AIPC.T` executadas.
    pub total_immediate_fusions: u64,
    /// Total de fusões tricótomas `BCEG` executadas.
    pub total_branch_fusions: u64,
    /// Telemetria acumulada de todos os 37 testes.
    pub total_telemetry: PowerTelemetry,
}

impl RiscvIsaSuiteSummary {
    /// Diferença percentual de instruções executadas na suíte inteira (`Setun` vs `RV32I`).
    pub fn instruction_diff_pct(&self) -> f64 {
        if self.total_rv32i_instrs == 0 {
            return 0.0;
        }
        let r = self.total_rv32i_instrs as f64;
        let s = self.total_setun_instrs as f64;
        ((s - r) / r) * 100.0
    }
}

/// Lista estática dos 37 microtestes oficiais da especificação `RV32I` compilados em `ELF32`.
pub const RISCV_ISA_ELF_TESTS: [(&str, &[u8]); 37] = [
    ("add", include_bytes!("../vendor/riscv_tests/elf/add.elf")),
    ("addi", include_bytes!("../vendor/riscv_tests/elf/addi.elf")),
    ("and", include_bytes!("../vendor/riscv_tests/elf/and.elf")),
    ("andi", include_bytes!("../vendor/riscv_tests/elf/andi.elf")),
    ("auipc", include_bytes!("../vendor/riscv_tests/elf/auipc.elf")),
    ("beq", include_bytes!("../vendor/riscv_tests/elf/beq.elf")),
    ("bge", include_bytes!("../vendor/riscv_tests/elf/bge.elf")),
    ("bgeu", include_bytes!("../vendor/riscv_tests/elf/bgeu.elf")),
    ("blt", include_bytes!("../vendor/riscv_tests/elf/blt.elf")),
    ("bltu", include_bytes!("../vendor/riscv_tests/elf/bltu.elf")),
    ("bne", include_bytes!("../vendor/riscv_tests/elf/bne.elf")),
    ("jal", include_bytes!("../vendor/riscv_tests/elf/jal.elf")),
    ("jalr", include_bytes!("../vendor/riscv_tests/elf/jalr.elf")),
    ("lb", include_bytes!("../vendor/riscv_tests/elf/lb.elf")),
    ("lbu", include_bytes!("../vendor/riscv_tests/elf/lbu.elf")),
    ("lh", include_bytes!("../vendor/riscv_tests/elf/lh.elf")),
    ("lhu", include_bytes!("../vendor/riscv_tests/elf/lhu.elf")),
    ("lui", include_bytes!("../vendor/riscv_tests/elf/lui.elf")),
    ("lw", include_bytes!("../vendor/riscv_tests/elf/lw.elf")),
    ("or", include_bytes!("../vendor/riscv_tests/elf/or.elf")),
    ("ori", include_bytes!("../vendor/riscv_tests/elf/ori.elf")),
    ("sb", include_bytes!("../vendor/riscv_tests/elf/sb.elf")),
    ("sh", include_bytes!("../vendor/riscv_tests/elf/sh.elf")),
    ("sll", include_bytes!("../vendor/riscv_tests/elf/sll.elf")),
    ("slli", include_bytes!("../vendor/riscv_tests/elf/slli.elf")),
    ("slt", include_bytes!("../vendor/riscv_tests/elf/slt.elf")),
    ("slti", include_bytes!("../vendor/riscv_tests/elf/slti.elf")),
    ("sltiu", include_bytes!("../vendor/riscv_tests/elf/sltiu.elf")),
    ("sltu", include_bytes!("../vendor/riscv_tests/elf/sltu.elf")),
    ("sra", include_bytes!("../vendor/riscv_tests/elf/sra.elf")),
    ("srai", include_bytes!("../vendor/riscv_tests/elf/srai.elf")),
    ("srl", include_bytes!("../vendor/riscv_tests/elf/srl.elf")),
    ("srli", include_bytes!("../vendor/riscv_tests/elf/srli.elf")),
    ("sub", include_bytes!("../vendor/riscv_tests/elf/sub.elf")),
    ("sw", include_bytes!("../vendor/riscv_tests/elf/sw.elf")),
    ("xor", include_bytes!("../vendor/riscv_tests/elf/xor.elf")),
    ("xori", include_bytes!("../vendor/riscv_tests/elf/xori.elf")),
];

/// Executa todos os 37 testes oficiais da ISA `RV32I` sobre o motor `SetunDbtEngine`.
pub fn run_all_riscv_isa_tests() -> Result<RiscvIsaSuiteSummary, DbtError> {
    let mut results = Vec::with_capacity(RISCV_ISA_ELF_TESTS.len());
    let mut total_rv32i_instrs = 0u64;
    let mut total_setun_instrs = 0u64;
    let mut total_immediate_fusions = 0u64;
    let mut total_branch_fusions = 0u64;
    let mut total_telemetry = PowerTelemetry::default();

    for &(name, elf_bytes) in &RISCV_ISA_ELF_TESTS {
        let mut engine = SetunDbtEngine::from_elf_bytes(elf_bytes)?;
        let report = engine.run(100_000)?;
        total_rv32i_instrs += report.rv32i_instructions_executed;
        total_setun_instrs += report.setun_instructions_executed;
        total_immediate_fusions += report.immediate_fusions_executed;
        total_branch_fusions += report.branch_fusions_executed;

        total_telemetry.ternary_trit_flips += report.telemetry.ternary_trit_flips;
        total_telemetry.ternary_half_swings += report.telemetry.ternary_half_swings;
        total_telemetry.ternary_full_swings += report.telemetry.ternary_full_swings;
        total_telemetry.binary32_bit_flips += report.telemetry.binary32_bit_flips;
        total_telemetry.adder_carry_propagations += report.telemetry.adder_carry_propagations;

        results.push((name, report));
    }

    Ok(RiscvIsaSuiteSummary {
        tests: results,
        total_rv32i_instrs,
        total_setun_instrs,
        total_immediate_fusions,
        total_branch_fusions,
        total_telemetry,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_37_riscv_rv32i_official_isa_tests_on_setun_dbt() {
        let summary =
            run_all_riscv_isa_tests().expect("Falha ao executar a suite de 37 testes RV32I");

        assert_eq!(summary.tests.len(), 37);

        for (name, report) in &summary.tests {
            assert!(
                report.is_riscv_test_pass(),
                "Teste oficial RV32I '{}' falhou: {:?}",
                name,
                report.exit
            );
            assert!(
                report.telemetry.adder_carry_propagations > 0,
                "Teste '{}' nao registrou propagacoes de carry nas portas ternarias",
                name
            );
        }

        // Verifica que as fusões de imediatos (LI.T / AIPC.T) e saltos tricótomos (BCEG)
        // reduziram efetivamente a contagem de instruções executadas frente ao RV32I binário.
        assert!(summary.total_immediate_fusions > 0);
        assert!(summary.total_branch_fusions > 0);
        assert!(summary.total_setun_instrs < summary.total_rv32i_instrs);
        assert!(
            summary.total_telemetry.ternary_trit_flips < summary.total_telemetry.binary32_bit_flips
        );
    }
}

//! Unidade Central de Processamento (`Setun-Doom CPU`), Conjunto de Instruções (ISA)
//! e Ciclo Fetch-Decode-Execute.
//!
//! Layout de Instrução de 21 trits (`Word21`):
//! - **Trits `[0..13]` (14 trits)**: Operando ou Endereço imediato ($3^{14} = 4.782.969$ posições).
//! - **Trits `[14..15]` (2 trits)**: Registrador de Índice Alvo ($3^2 = 9$ registradores, codificados
//!   em `[-4..+4]` ou mapeados em `0..8`).
//! - **Trits `[16..20]` (5 trits)**: Opcode ($3^5 = 243$ instruções possíveis, em `[-121..+121]`).
//!
//! Desvios Condicionais Tricótomos:
//! O Setun dispensa registradores de flags binários (Zero/Sign/Carry/Overflow). Os saltos
//! inspecionam diretamente o estado intrínseco do Acumulador (`ACC.sign_trit()`):
//! - `JMPN`: desvia se `ACC < 0` (`Trit::Negative`).
//! - `JMPZ`: desvia se `ACC == 0` (`Trit::Zero`).
//! - `JMPP`: desvia se `ACC > 0` (`Trit::Positive`).
//! - `JMP`: desvio incondicional.

use crate::fixed::TernaryFixed;
use crate::memory::{Memory, INITIAL_STACK_POINTER, MEMORY_SIZE};
use crate::trit::Trit;
use crate::word::{extract_trit_field, Word21, POW3};

/// Quantidade de registradores de índice na arquitetura Setun-Doom ($3^2 = 9$).
pub const INDEX_REGISTER_COUNT: usize = 9;

/// Opcodes de 5 trits da arquitetura Setun-Doom (`[-121, +121]`).
#[repr(i8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Opcode {
    /// Sem operação (`0`).
    Nop = 0,
    /// `ACC = ACC + MEM[operand]` (`+1`).
    Add = 1,
    /// `ACC = ACC - MEM[operand]` (`-1`).
    Sub = -1,
    /// `ACC = FixedMul(ACC, MEM[operand])` em ponto fixo ternário `11.10`.
    /// Por padrão (`reg_idx == 0`, estilo Setun-58 `++0`), copia o multiplicando `ACC` anterior para `R`;
    /// se `reg_idx == 1`, deposita o resto fracionário de 10 trits em `R` (`+2`).
    Mul = 2,
    /// `ACC = FixedDiv(ACC, MEM[operand])` em ponto fixo ternário `11.10` (`-2`).
    Div = -2,
    /// `ACC = -ACC` (inversão instantânea de todos os 21 trits) (`+3`).
    Neg = 3,

    /// `ACC = MEM[operand]` (`+4`).
    Load = 4,
    /// `MEM[operand] = ACC` (`-4`).
    Store = -4,
    /// `ACC = MEM[operand + IX[reg]]` (`+5`).
    LoadIndex = 5,
    /// `MEM[operand + IX[reg]] = ACC` (`-5`).
    StoreIndex = -5,

    /// Salto incondicional: `PC = operand` (`+6`).
    Jmp = 6,
    /// Salto tricótomo negativo (`reg_idx == 0`: salta se `ACC < 0`; `reg_idx == 1`: salta se `ACC >= 0` / `JMP_GE`) (`-7`).
    JmpN = -7,
    /// Salto tricótomo zero (`reg_idx == 0`: salta se `ACC == 0`; `reg_idx == 1`: salta se `ACC != 0` / `JMP_NE`) (`+7`).
    JmpZ = 7,
    /// Salto tricótomo positivo (`reg_idx == 0`: salta se `ACC > 0`; `reg_idx == 1`: salta se `ACC <= 0` / `JMP_LE`) (`+8`).
    JmpP = 8,
    /// Branch Compare Equal or Greater (`bceg` do REBEL-2):
    /// Compara `ACC` com `IX[reg]` (ou usa `ACC` direto) e despacha em 3 vias em 1 único ciclo
    /// usando dois offsets relativos de 7 trits empacotados nos 14 trits do operando:
    /// - Se `ACC == IX[reg]`: `PC = PC_atual + offset_eq` (trits `0..6`, `-1093..+1093`)
    /// - Se `ACC > IX[reg]`:  `PC = PC_atual + offset_gt` (trits `7..13`, `-1093..+1093`)
    /// - Se `ACC < IX[reg]`:  segue para `PC + 1` (`+19`).
    Bceg = 19,

    /// Empilha `ACC`: `MEM[SP] = ACC; SP -= 1` (`+9`).
    Push = 9,
    /// Desempilha em `ACC`: `SP += 1; ACC = MEM[SP]` (`-9`).
    Pop = -9,
    /// Chamada de sub-rotina: empilha `PC` de retorno e salta para `operand` (`+10`).
    Call = 10,
    /// Retorno de sub-rotina: desempilha endereço e restaura `PC` (`-10`).
    Ret = -10,

    /// Carrega constante imediata de 14 trits no `ACC`: `ACC = operand` (`+11`).
    LoadImm = 11,
    /// Soma constante imediata de 14 trits ao `ACC`: `ACC = ACC + operand` (`-11`).
    AddImm = -11,
    /// Carrega constante imediata de 14 trits no registrador de índice `IX[reg]` (`+12`).
    LoadIxImm = 12,
    /// Soma constante imediata ao registrador de índice `IX[reg]` (`-12`).
    AddIxImm = -12,
    /// Copia `ACC` para `IX[reg]` (se `reg == 8`, atualiza também o registrador multiplicador `R`) (`+13`).
    MovAccToIx = 13,
    /// Copia `IX[reg]` para `ACC` (se `reg == 8`, lê o registrador multiplicador `R`) (`-13`).
    MovIxToAcc = -13,
    /// Deslocamento ternário de trits por `operand` posições:
    /// se `reg_idx == 0`, desloca `ACC`; se `reg_idx > 0`, desloca `IX[reg_idx]` e grava em `ACC` (`+14`).
    Shift = 14,
    /// Multiplicação inteira pura `ACC = ACC * MEM[operand]` (`-14`).
    MulInt = -14,
    /// Divisão inteira pura `ACC = ACC / MEM[operand]` (`+15`).
    DivInt = 15,
    /// Comparação Tricótoma (`cmp.t` do artigo REBEL-6): `ACC = ACC <=> MEM[operand]` em `{-1, 0, +1}` (`+16`).
    CmpT = 16,
    /// Votação Majoritária Ternária (`majv.t` do artigo REBEL-6):
    /// `ACC = MajorityVote(ACC, IX[reg], MEM[operand])` trit-a-trit (`+17`).
    MajvT = 17,
    /// Votação Minoritária Ternária (`minv.t` do artigo REBEL-6):
    /// `ACC = MinorityVote(ACC, IX[reg], MEM[operand])` trit-a-trit (`-17`).
    MinvT = -17,
    /// Soma palavra da memória diretamente ao registrador `IX[reg]`:
    /// `IX[reg] = IX[reg] + MEM[operand]` sem destruir `ACC` (`+20`).
    AddIx = 20,
    /// Decrementa `IX[reg]` em 1 e salta para `operand` se `IX[reg] >= 0` (`BIX` / laço de índice
    /// estilo Setun sem sobrescrever `ACC`) (`-20`).
    DecIxJmpGe = -20,
    /// `R = MEM[operand]` (`PARCEL_TO_R` `+-+` da Setun-58 / `YRT` da Setun-70) (`+22`).
    LoadR = 22,
    /// `ACC = ACC + FixedMul(MEM[operand], R)` (`MULTIPLICATION_POS` `+++` da Setun-58 / `LBT` `B25` da Setun-70) (`+24`).
    MacPos = 24,
    /// `ACC = ACC - FixedMul(MEM[operand], R)` (`MULTIPLICATION_NEG` `++-` da Setun-58) (`-24`).
    MacNeg = -24,
    /// Multiplicação trit-a-trit `ACC = ACC (*) MEM[operand]` (`SMT` `+-0` da Setun-58 / `B19` da Setun-70) (`+25`).
    Smt = 25,
    /// Adição limiar trit-a-trit `ACC = ThresholdAdd(ACC, MEM[operand])` (`SAT` `B21` `000+-+` da Setun-70) (`-25`).
    Sat = -25,
    /// Inibição/Coalescência trit-a-trit `ACC = ACC + MEM[operand] * (1 - |ACC|)` (`SIT` `B20` `000+-0` da Setun-70) (`+26`).
    Sit = 26,
    /// Parada controlada da CPU (`HALT`) (`+40`).
    Halt = 40,
}

impl Opcode {
    /// Decodifica um campo de 5 trits (`-121..=+121`) em um `Opcode`.
    #[inline(always)]
    pub const fn from_trit_field(val: i64) -> Option<Self> {
        match val {
            0 => Some(Self::Nop),
            1 => Some(Self::Add),
            -1 => Some(Self::Sub),
            2 => Some(Self::Mul),
            -2 => Some(Self::Div),
            3 => Some(Self::Neg),
            4 => Some(Self::Load),
            -4 => Some(Self::Store),
            5 => Some(Self::LoadIndex),
            -5 => Some(Self::StoreIndex),
            6 => Some(Self::Jmp),
            -7 => Some(Self::JmpN),
            7 => Some(Self::JmpZ),
            8 => Some(Self::JmpP),
            9 => Some(Self::Push),
            -9 => Some(Self::Pop),
            10 => Some(Self::Call),
            -10 => Some(Self::Ret),
            11 => Some(Self::LoadImm),
            -11 => Some(Self::AddImm),
            12 => Some(Self::LoadIxImm),
            -12 => Some(Self::AddIxImm),
            13 => Some(Self::MovAccToIx),
            -13 => Some(Self::MovIxToAcc),
            14 => Some(Self::Shift),
            -14 => Some(Self::MulInt),
            15 => Some(Self::DivInt),
            16 => Some(Self::CmpT),
            17 => Some(Self::MajvT),
            -17 => Some(Self::MinvT),
            19 => Some(Self::Bceg),
            20 => Some(Self::AddIx),
            -20 => Some(Self::DecIxJmpGe),
            22 => Some(Self::LoadR),
            24 => Some(Self::MacPos),
            -24 => Some(Self::MacNeg),
            25 => Some(Self::Smt),
            -25 => Some(Self::Sat),
            26 => Some(Self::Sit),
            40 => Some(Self::Halt),
            _ => None,
        }
    }
}

/// Instrução decodificada da arquitetura Setun-Doom.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Instruction {
    /// Opcode de 5 trits.
    pub opcode: Opcode,
    /// Índice do registrador alvo (`0..=8`, mapeado dos 2 trits `[-4..+4]`).
    pub reg_idx: usize,
    /// Operando ou endereço imediato de 14 trits (`[-2_391_484, +2_391_484]`).
    pub operand: i64,
}

impl Instruction {
    /// Cria uma nova instrução verificando o índice de registrador (`0..9`).
    #[inline(always)]
    pub const fn new(opcode: Opcode, reg_idx: usize, operand: i64) -> Self {
        assert!(reg_idx < INDEX_REGISTER_COUNT);
        Self {
            opcode,
            reg_idx,
            operand,
        }
    }

    /// Codifica a instrução em uma palavra ternária `Word21`:
    /// `word = operand + (reg_trits * 3^14) + (opcode * 3^16)`.
    #[inline(always)]
    pub const fn encode(self) -> Word21 {
        let reg_balanced = (self.reg_idx as i64) - 4; // 0..8 -> -4..+4 (2 trits)
        let op_balanced = self.opcode as i64; // 5 trits
        // Garante que o operando esteja na faixa balanceada de 14 trits [-2_391_484, +2_391_484]
        let operand_balanced = crate::word::wrap_balanced::<14>(self.operand);
        let raw = operand_balanced + reg_balanced * POW3[14] + op_balanced * POW3[16];
        Word21::new(raw)
    }

    /// Decodifica uma `Word21` em `Instruction` extraindo os campos de 14, 2 e 5 trits.
    #[inline(always)]
    pub const fn decode(word: Word21) -> Option<Self> {
        let raw = word.as_i64();
        let operand = extract_trit_field(raw, 0, 14);
        let reg_balanced = extract_trit_field(raw, 14, 2); // -4..+4
        let op_balanced = extract_trit_field(raw, 16, 5); // -121..+121

        let opcode = match Opcode::from_trit_field(op_balanced) {
            Some(op) => op,
            None => return None,
        };
        let reg_idx = (reg_balanced + 4) as usize;
        Some(Self {
            opcode,
            reg_idx,
            operand,
        })
    }

    /// Converte o operando balanceado de 14 trits para um endereço linear `0..4_782_969`
    /// quando usado para endereçamento de memória alta (como Framebuffer).
    #[inline(always)]
    pub const fn operand_as_address(self) -> i64 {
        if self.operand < 0 {
            self.operand + (MEMORY_SIZE as i64)
        } else {
            self.operand
        }
    }

    /// Empacota dois offsets relativos de 7 trits (`offset_eq` e `offset_gt`, cada um em `[-1093..+1093]`)
    /// dentro de um operando de 14 trits para a instrução `BCEG`.
    #[inline(always)]
    pub const fn pack_bceg_offsets(offset_eq: i64, offset_gt: i64) -> i64 {
        let eq7 = crate::word::wrap_balanced::<7>(offset_eq);
        let gt7 = crate::word::wrap_balanced::<7>(offset_gt);
        eq7 + gt7 * POW3[7]
    }

    /// Desempacota os dois offsets relativos de 7 trits (`(offset_eq, offset_gt)`) do operando `BCEG`.
    #[inline(always)]
    pub const fn unpack_bceg_offsets(operand: i64) -> (i64, i64) {
        let eq7 = extract_trit_field(operand, 0, 7);
        let gt7 = extract_trit_field(operand, 7, 7);
        (eq7, gt7)
    }

    /// Constrói diretamente uma instrução `BCEG` comparando `ACC` com `IX[reg_idx]`.
    #[inline(always)]
    pub const fn new_bceg(reg_idx: usize, offset_eq: i64, offset_gt: i64) -> Self {
        Self::new(
            Opcode::Bceg,
            reg_idx,
            Self::pack_bceg_offsets(offset_eq, offset_gt),
        )
    }
}

/// Exceções / Armadilhas de Hardware da CPU Setun-Doom (`CpuTrap`).
/// Impede falhas silenciosas (como decodificação inválida tratada como `HALT`,
/// divisão por zero ignorada ou acesso fora da faixa de 14 trits de memória).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CpuTrap {
    /// Palavra de instrução contém código de operação ou registrador inválido.
    InvalidInstruction { pc: i64, raw_word: Word21 },
    /// Tentativa de divisão por zero em `Opcode::Div` ou `Opcode::DivInt`.
    DivisionByZero { pc: i64, opcode: Opcode },
    /// Violação de limites no barramento de memória de 14 trits.
    MemoryFault {
        pc: i64,
        error: crate::memory::MemoryError,
    },
}

/// Telemetria de eficiência energética e chaveamento de estado (*Digit Flips*) baseada na
/// metodologia da Seção IV.B do artigo **REBEL-6** (IEEE ISMVL 2025), livre de viés de escala.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct PowerTelemetry {
    /// Total de trits que mudaram de estado ao atualizar registradores (`Word21`).
    pub ternary_trit_flips: u64,
    /// Quantidade de transições ternárias de meia-excursão (`-1 <-> 0` ou `0 <-> +1`).
    pub ternary_half_swings: u64,
    /// Quantidade de transições ternárias de excursão completa (`-1 <-> +1`).
    pub ternary_full_swings: u64,
    /// Total de bits que mudariam em registradores binários de 32 bits (`RV32I` complemento de dois)
    /// executando as operações equivalentes na representação nativa de cada base.
    pub binary32_bit_flips: u64,
    /// Propagações de carry ativas medidas nos estágios `Trit::full_add` da ALU ternária.
    pub adder_carry_propagations: u64,
}

impl PowerTelemetry {
    /// Contabiliza uma escrita de registrador inteiro de `prev` para `next`.
    #[inline(always)]
    pub fn record_transition(&mut self, prev: Word21, next: Word21) {
        if prev == next {
            return;
        }
        let (t_flips, half, full) = prev.count_trit_flips(next);
        let b_flips = prev.count_binary32_flips(next);
        self.ternary_trit_flips += t_flips as u64;
        self.ternary_half_swings += half as u64;
        self.ternary_full_swings += full as u64;
        self.binary32_bit_flips += b_flips as u64;
    }

    /// Contabiliza uma transição pareada 1-para-1 de ponto fixo comparando a transição
    /// `prev_ternary -> next_ternary` em `Word21` (`11.10`, escala $3^{10} = 59.049$)
    /// contra a transição exata correspondente `prev_binary16 -> next_binary16` em `RV32I` (`16.16`, escala $2^{16} = 65.536$).
    #[inline(always)]
    pub fn record_fixed16_transition(
        &mut self,
        prev_ternary: Word21,
        next_ternary: Word21,
        prev_binary16: i32,
        next_binary16: i32,
    ) {
        if prev_ternary != next_ternary {
            let (t_flips, half, full) = prev_ternary.count_trit_flips(next_ternary);
            self.ternary_trit_flips += t_flips as u64;
            self.ternary_half_swings += half as u64;
            self.ternary_full_swings += full as u64;
        }
        if prev_binary16 != next_binary16 {
            let b_flips = (prev_binary16 ^ next_binary16).count_ones();
            self.binary32_bit_flips += b_flips as u64;
        }
    }

    /// Calcula a redução percentual de *Digit Flips* da arquitetura ternária em relação ao binário 32-bit
    /// (ex: `33.24` significa `-33.24%` de chaveamento dinâmico como reportado no artigo REBEL-6).
    pub fn digit_flip_reduction_pct(&self) -> f64 {
        if self.binary32_bit_flips == 0 {
            return 0.0;
        }
        let b = self.binary32_bit_flips as f64;
        let t = self.ternary_trit_flips as f64;
        ((b - t) / b) * 100.0
    }

    /// Calcula a redução percentual de energia capacitiva dinâmica ($E \propto \Delta V^2$),
    /// ponderando cada *Half-Swing* (`-1 <-> 0`, `0 <-> +1`) com $0{,}25\times$ a energia de um
    /// *Full-Swing* (`-1 <-> +1` ou transição binária `0 <-> 1`).
    pub fn weighted_energy_reduction_pct(&self) -> f64 {
        if self.binary32_bit_flips == 0 {
            return 0.0;
        }
        let b_energy = self.binary32_bit_flips as f64;
        let t_energy =
            (self.ternary_half_swings as f64) * 0.25 + (self.ternary_full_swings as f64) * 1.0;
        ((b_energy - t_energy) / b_energy) * 100.0
    }
}

/// Estado arquitetural completo da CPU Setun-Doom.
pub struct Cpu {
    /// Acumulador principal de 21 trits (`ACC`).
    pub acc: Word21,
    /// Registrador Multiplicador/Quociente (`R`) de 21 trits.
    pub r: Word21,
    /// Contador de Programa (`PC`) apontando para o endereço da próxima instrução.
    pub pc: i64,
    /// Ponteiro de Pilha (`SP`).
    pub sp: i64,
    /// Banco de 9 registradores de índice (`IX[0..9]`), endereçados por 2 trits (`-4..+4`).
    pub index_regs: [Word21; INDEX_REGISTER_COUNT],
    /// Indica se a CPU encontrou uma instrução `HALT` legítima.
    pub halted: bool,
    /// Armadilha de hardware disparada caso ocorra erro de execução (nunca falha silenciosamente).
    pub trap: Option<CpuTrap>,
    /// Contador total de ciclos/instruções executadas.
    pub cycles: u64,
    /// Telemetria comparativa de chaveamento de estados (*Trit Flips* vs *Bit Flips* binários).
    pub telemetry: PowerTelemetry,
}

impl Cpu {
    /// Inicializa a CPU com registradores zerados e `SP` no topo da RAM.
    pub fn new() -> Self {
        Self {
            acc: Word21::ZERO,
            r: Word21::ZERO,
            pc: 0,
            sp: INITIAL_STACK_POINTER,
            index_regs: [Word21::ZERO; INDEX_REGISTER_COUNT],
            halted: false,
            trap: None,
            cycles: 0,
            telemetry: PowerTelemetry::default(),
        }
    }

    /// Reinicia o estado da CPU preservando ou definindo o ponto de entrada `entry_pc`.
    pub fn reset(&mut self, entry_pc: i64) {
        self.acc = Word21::ZERO;
        self.r = Word21::ZERO;
        self.pc = entry_pc;
        self.sp = INITIAL_STACK_POINTER;
        self.index_regs = [Word21::ZERO; INDEX_REGISTER_COUNT];
        self.halted = false;
        self.trap = None;
        self.cycles = 0;
        self.telemetry = PowerTelemetry::default();
    }

    #[inline(always)]
    fn set_acc(&mut self, next: Word21) {
        self.telemetry.record_transition(self.acc, next);
        self.acc = next;
    }

    #[inline(always)]
    fn set_r(&mut self, next: Word21) {
        self.telemetry.record_transition(self.r, next);
        self.r = next;
    }

    #[inline(always)]
    fn set_ix(&mut self, idx: usize, next: Word21) {
        let prev = self.index_regs[idx];
        self.telemetry.record_transition(prev, next);
        self.index_regs[idx] = next;
    }

    /// Carrega uma fatia de instruções codificadas na memória a partir de `base_addr`
    /// e posiciona `PC` em `base_addr`.
    pub fn load_program(&mut self, mem: &mut Memory, base_addr: i64, instructions: &[Instruction]) {
        for (offset, instr) in instructions.iter().enumerate() {
            mem.write_word(base_addr + (offset as i64), instr.encode())
                .expect("Falha ao carregar programa na memoria ternaria");
        }
        self.pc = base_addr;
        self.halted = false;
        self.trap = None;
    }

    /// Executa um único ciclo Fetch-Decode-Execute com verificação estrita de erros (`CpuTrap`).
    /// Retorna `Ok(true)` se a instrução executou e a CPU segue ativa, `Ok(false)` se atingiu `HALT`,
    /// ou `Err(CpuTrap)` se ocorrer instrução inválida, divisão por zero ou violação de memória.
    #[inline]
    pub fn step_checked(&mut self, mem: &mut Memory) -> Result<bool, CpuTrap> {
        if let Some(trap) = self.trap {
            return Err(trap);
        }
        if self.halted {
            return Ok(false);
        }

        let instr_pc = self.pc;

        // 1. FETCH (verificado em 14 trits)
        let instr_word = mem.read_word(instr_pc).map_err(|error| {
            let t = CpuTrap::MemoryFault {
                pc: instr_pc,
                error,
            };
            self.trap = Some(t);
            t
        })?;
        self.pc += 1;

        // 2. DECODE (proibido tratar opcode inválido como HALT silencioso)
        let Some(instr) = Instruction::decode(instr_word) else {
            let t = CpuTrap::InvalidInstruction {
                pc: instr_pc,
                raw_word: instr_word,
            };
            self.trap = Some(t);
            return Err(t);
        };

        // Helper para leitura/escrita verificada de operandos
        let read_mem = |mem: &Memory, addr: i64| -> Result<Word21, CpuTrap> {
            mem.read_word(addr).map_err(|error| CpuTrap::MemoryFault {
                pc: instr_pc,
                error,
            })
        };
        let write_mem = |mem: &mut Memory, addr: i64, val: Word21| -> Result<(), CpuTrap> {
            mem.write_word(addr, val)
                .map_err(|error| CpuTrap::MemoryFault {
                    pc: instr_pc,
                    error,
                })
        };

        // 3. EXECUTE
        let exec_res: Result<bool, CpuTrap> = (|| {
            match instr.opcode {
                Opcode::Nop => {}

                Opcode::Add => {
                    let addr = instr.operand_as_address();
                    let val = read_mem(mem, addr)?;
                    let (sum, _cout, carries) =
                        self.acc.gate_ripple_carry_add(val, Trit::Zero);
                    self.telemetry.adder_carry_propagations += carries as u64;
                    self.set_acc(sum);
                }

                Opcode::Sub => {
                    let addr = instr.operand_as_address();
                    let val = read_mem(mem, addr)?;
                    let neg_val = val.gate_invert_trits();
                    let (diff, _cout, carries) =
                        self.acc.gate_ripple_carry_add(neg_val, Trit::Zero);
                    self.telemetry.adder_carry_propagations += carries as u64;
                    self.set_acc(diff);
                }

                Opcode::Mul => {
                    // Unifica MUL e MUL_ZERO (++0 da Setun-58 / L*T B26 da Setun-70):
                    // Por padrão (`reg_idx == 0`), copia o multiplicando `ACC` anterior para `R`;
                    // se `reg_idx == 1`, deposita o resto fracionário de 10 trits em `R`.
                    let addr = instr.operand_as_address();
                    let val = read_mem(mem, addr)?;
                    let prev_acc = self.acc;
                    let a = TernaryFixed::from_raw(prev_acc);
                    let b = TernaryFixed::from_raw(val);
                    let (_prod42, carries) = prev_acc.gate_shift_and_add_mul42(val);
                    self.telemetry.adder_carry_propagations += carries as u64;
                    let (prod, rem) = a.fixed_mul_with_rem(b);

                    // Comparação pareada 1-para-1 sem viés de base (prev_16 -> next_16):
                    let a16 = a.to_doom_fixed16();
                    let b16 = b.to_doom_fixed16();
                    let bin_prod16 = (((a16 as i64) * (b16 as i64)) >> 16)
                        .clamp(i32::MIN as i64, i32::MAX as i64) as i32;
                    self.telemetry.record_fixed16_transition(
                        prev_acc,
                        prod.raw_word(),
                        a16,
                        bin_prod16,
                    );
                    self.acc = prod.raw_word();
                    if instr.reg_idx == 1 {
                        self.set_r(rem);
                    } else {
                        self.set_r(prev_acc);
                    }
                }

                Opcode::Div => {
                    let addr = instr.operand_as_address();
                    let val = read_mem(mem, addr)?;
                    let a = TernaryFixed::from_raw(self.acc);
                    let b = TernaryFixed::from_raw(val);
                    let Some(quot) = a.checked_fixed_div(b) else {
                        return Err(CpuTrap::DivisionByZero {
                            pc: instr_pc,
                            opcode: Opcode::Div,
                        });
                    };
                    let a16 = a.to_doom_fixed16();
                    let b16 = b.to_doom_fixed16();
                    let bin_quot16 = if b16 != 0 {
                        (((a16 as i64) << 16) / (b16 as i64))
                            .clamp(i32::MIN as i64, i32::MAX as i64) as i32
                    } else {
                        quot.to_doom_fixed16()
                    };
                    self.telemetry.record_fixed16_transition(
                        self.acc,
                        quot.raw_word(),
                        a16,
                        bin_quot16,
                    );
                    self.acc = quot.raw_word();
                }

                Opcode::Neg => {
                    // Inversão física de 21 portas `Trit::invert` (`sti.t` do artigo REBEL-6)
                    self.set_acc(self.acc.gate_invert_trits());
                }

                Opcode::Load => {
                    let addr = instr.operand_as_address();
                    let val = read_mem(mem, addr)?;
                    self.set_acc(val);
                }

                Opcode::Store => {
                    let addr = instr.operand_as_address();
                    write_mem(mem, addr, self.acc)?;
                }

                Opcode::LoadIndex => {
                    let base = instr.operand_as_address();
                    let offset = self.index_regs[instr.reg_idx].as_i64();
                    let val = read_mem(mem, base + offset)?;
                    self.set_acc(val);
                }

                Opcode::StoreIndex => {
                    let base = instr.operand_as_address();
                    let offset = self.index_regs[instr.reg_idx].as_i64();
                    write_mem(mem, base + offset, self.acc)?;
                }

                Opcode::Jmp => {
                    self.pc = instr.operand_as_address();
                }

                Opcode::JmpN => {
                    // reg_idx == 0: salta se ACC < 0 (JMPN); reg_idx == 1: salta se ACC >= 0 (JMP_GE)
                    let is_neg = self.acc.sign_trit() == Trit::Negative;
                    if is_neg ^ (instr.reg_idx == 1) {
                        self.pc = instr.operand_as_address();
                    }
                }

                Opcode::JmpZ => {
                    // reg_idx == 0: salta se ACC == 0 (JMPZ); reg_idx == 1: salta se ACC != 0 (JMP_NE)
                    let is_zero = self.acc.sign_trit() == Trit::Zero;
                    if is_zero ^ (instr.reg_idx == 1) {
                        self.pc = instr.operand_as_address();
                    }
                }

                Opcode::JmpP => {
                    // reg_idx == 0: salta se ACC > 0 (JMPP); reg_idx == 1: salta se ACC <= 0 (JMP_LE)
                    let is_pos = self.acc.sign_trit() == Trit::Positive;
                    if is_pos ^ (instr.reg_idx == 1) {
                        self.pc = instr.operand_as_address();
                    }
                }

                Opcode::Bceg => {
                    // Instrução BCEG operando via comparador físico de 21 trits (`gate_cmp_t`):
                    let rhs = self.index_regs[instr.reg_idx];
                    let cmp = self.acc.gate_cmp_t(rhs);
                    let (offset_eq, offset_gt) = Instruction::unpack_bceg_offsets(instr.operand);
                    match cmp {
                        Trit::Zero => {
                            if offset_eq != 0 {
                                self.pc = instr_pc + offset_eq;
                            }
                        }
                        Trit::Positive => {
                            if offset_gt != 0 {
                                self.pc = instr_pc + offset_gt;
                            }
                        }
                        Trit::Negative => {}
                    }
                }

                Opcode::Push => {
                    write_mem(mem, self.sp, self.acc)?;
                    self.sp -= 1;
                }

                Opcode::Pop => {
                    self.sp += 1;
                    let val = read_mem(mem, self.sp)?;
                    self.set_acc(val);
                }

                Opcode::Call => {
                    write_mem(mem, self.sp, Word21::new(self.pc))?;
                    self.sp -= 1;
                    self.pc = instr.operand_as_address();
                }

                Opcode::Ret => {
                    self.sp += 1;
                    let ret_addr = read_mem(mem, self.sp)?;
                    self.pc = ret_addr.as_i64();
                }

                Opcode::LoadImm => {
                    self.set_acc(Word21::new(instr.operand));
                }

                Opcode::AddImm => {
                    let (sum, _cout, carries) = self
                        .acc
                        .gate_ripple_carry_add(Word21::new(instr.operand), Trit::Zero);
                    self.telemetry.adder_carry_propagations += carries as u64;
                    self.set_acc(sum);
                }

                Opcode::LoadIxImm => {
                    self.set_ix(instr.reg_idx, Word21::new(instr.operand));
                }

                Opcode::AddIxImm => {
                    let (next, _cout, carries) = self.index_regs[instr.reg_idx]
                        .gate_ripple_carry_add(Word21::new(instr.operand), Trit::Zero);
                    self.telemetry.adder_carry_propagations += carries as u64;
                    self.set_ix(instr.reg_idx, next);
                }

                Opcode::MovAccToIx => {
                    self.set_ix(instr.reg_idx, self.acc);
                    if instr.reg_idx == 8 {
                        self.set_r(self.acc);
                    }
                }

                Opcode::MovIxToAcc => {
                    if instr.reg_idx == 8 {
                        self.set_acc(self.r);
                    } else {
                        self.set_acc(self.index_regs[instr.reg_idx]);
                    }
                }

                Opcode::Shift => {
                    // reg_idx == 0: desloca ACC; reg_idx > 0: desloca IX[reg_idx] para ACC sem destruir IX
                    let src = if instr.reg_idx == 0 {
                        self.acc
                    } else {
                        self.index_regs[instr.reg_idx]
                    };
                    self.set_acc(src.shift_trits(instr.operand as i32));
                }

                Opcode::MulInt => {
                    let addr = instr.operand_as_address();
                    let val = read_mem(mem, addr)?;
                    let (_prod42, carries) = self.acc.gate_shift_and_add_mul42(val);
                    self.telemetry.adder_carry_propagations += carries as u64;
                    self.set_acc(self.acc * val);
                }

                Opcode::DivInt => {
                    let addr = instr.operand_as_address();
                    let val = read_mem(mem, addr)?;
                    let Some((quot, rem)) = self.acc.checked_div_rem(val) else {
                        return Err(CpuTrap::DivisionByZero {
                            pc: instr_pc,
                            opcode: Opcode::DivInt,
                        });
                    };
                    self.set_r(rem);
                    self.set_acc(quot);
                }

                Opcode::CmpT => {
                    let addr = instr.operand_as_address();
                    let rhs = read_mem(mem, addr)?;
                    self.set_acc(self.acc.cmp_t(rhs));
                }

                Opcode::MajvT => {
                    let addr = instr.operand_as_address();
                    let third = read_mem(mem, addr)?;
                    let second = self.index_regs[instr.reg_idx];
                    self.set_acc(self.acc.tritwise_majority(second, third));
                }

                Opcode::MinvT => {
                    let addr = instr.operand_as_address();
                    let third = read_mem(mem, addr)?;
                    let second = self.index_regs[instr.reg_idx];
                    self.set_acc(self.acc.tritwise_minority(second, third));
                }

                Opcode::AddIx => {
                    let addr = instr.operand_as_address();
                    let val = read_mem(mem, addr)?;
                    let (next, _cout, carries) =
                        self.index_regs[instr.reg_idx].gate_ripple_carry_add(val, Trit::Zero);
                    self.telemetry.adder_carry_propagations += carries as u64;
                    self.set_ix(instr.reg_idx, next);
                }

                Opcode::DecIxJmpGe => {
                    let (next, _cout, carries) = self.index_regs[instr.reg_idx]
                        .gate_ripple_carry_add(Word21::NEG_ONE, Trit::Zero);
                    self.telemetry.adder_carry_propagations += carries as u64;
                    self.set_ix(instr.reg_idx, next);
                    if next.sign_trit() != Trit::Negative {
                        self.pc = instr.operand_as_address();
                    }
                }

                Opcode::LoadR => {
                    let addr = instr.operand_as_address();
                    let val = read_mem(mem, addr)?;
                    self.set_r(val);
                }

                Opcode::MacPos => {
                    // Setun-58 (+++) / Setun-70 LBT (B25): ACC = ACC + FixedMul(MEM[op], R)
                    let addr = instr.operand_as_address();
                    let val = read_mem(mem, addr)?;
                    let m = TernaryFixed::from_raw(val);
                    let r = TernaryFixed::from_raw(self.r);
                    let (_prod42, mul_carries) = val.gate_shift_and_add_mul42(self.r);
                    let (prod, _rem) = m.fixed_mul_with_rem(r);
                    let (sum, _cout, add_carries) =
                        self.acc.gate_ripple_carry_add(prod.raw_word(), Trit::Zero);
                    self.telemetry.adder_carry_propagations +=
                        (mul_carries + add_carries) as u64;

                    let acc16 = TernaryFixed::from_raw(self.acc).to_doom_fixed16();
                    let m16 = m.to_doom_fixed16();
                    let r16 = r.to_doom_fixed16();
                    let bin_prod16 = (((m16 as i64) * (r16 as i64)) >> 16)
                        .clamp(i32::MIN as i64, i32::MAX as i64) as i32;
                    let bin_mac16 = acc16.wrapping_add(bin_prod16);
                    self.telemetry
                        .record_fixed16_transition(self.acc, sum, acc16, bin_mac16);
                    self.acc = sum;
                }

                Opcode::MacNeg => {
                    // Setun-58 (++-): ACC = ACC - FixedMul(MEM[op], R)
                    let addr = instr.operand_as_address();
                    let val = read_mem(mem, addr)?;
                    let m = TernaryFixed::from_raw(val);
                    let r = TernaryFixed::from_raw(self.r);
                    let (_prod42, mul_carries) = val.gate_shift_and_add_mul42(self.r);
                    let (prod, _rem) = m.fixed_mul_with_rem(r);
                    let neg_prod = prod.raw_word().gate_invert_trits();
                    let (diff, _cout, add_carries) =
                        self.acc.gate_ripple_carry_add(neg_prod, Trit::Zero);
                    self.telemetry.adder_carry_propagations +=
                        (mul_carries + add_carries) as u64;

                    let acc16 = TernaryFixed::from_raw(self.acc).to_doom_fixed16();
                    let m16 = m.to_doom_fixed16();
                    let r16 = r.to_doom_fixed16();
                    let bin_prod16 = (((m16 as i64) * (r16 as i64)) >> 16)
                        .clamp(i32::MIN as i64, i32::MAX as i64) as i32;
                    let bin_mac16 = acc16.wrapping_sub(bin_prod16);
                    self.telemetry
                        .record_fixed16_transition(self.acc, diff, acc16, bin_mac16);
                    self.acc = diff;
                }

                Opcode::Smt => {
                    // Setun-58 (+-0) / Setun-70 B19 (SMT): multiplicação trit-a-trit
                    let addr = instr.operand_as_address();
                    let val = read_mem(mem, addr)?;
                    self.set_acc(self.acc.tritwise_mul(val));
                }

                Opcode::Sat => {
                    // Setun-70 B21 (SAT 000+-+): adição limiar trit-a-trit
                    let addr = instr.operand_as_address();
                    let val = read_mem(mem, addr)?;
                    self.set_acc(self.acc.tritwise_sat(val));
                }

                Opcode::Sit => {
                    // Setun-70 B20 (SIT 000+-0): S + T * (1 - |S|) trit-a-trit
                    let addr = instr.operand_as_address();
                    let val = read_mem(mem, addr)?;
                    self.set_acc(self.acc.tritwise_sit(val));
                }

                Opcode::Halt => {
                    self.halted = true;
                    self.cycles += 1;
                    return Ok(false);
                }
            }
            Ok(true)
        })();

        match exec_res {
            Ok(active) => {
                if active {
                    self.cycles += 1;
                }
                Ok(active)
            }
            Err(trap) => {
                self.trap = Some(trap);
                Err(trap)
            }
        }
    }

    /// Executa um único ciclo Fetch-Decode-Execute (`step()`).
    /// Aborta ruidosamente (panic) se qualquer `CpuTrap` ocorrer (proibindo falha silenciosa).
    #[inline(always)]
    pub fn step(&mut self, mem: &mut Memory) -> bool {
        self.step_checked(mem)
            .unwrap_or_else(|trap| panic!("ARMADILHA DE HARDWARE SETUN-DOOM: {:?}", trap))
    }

    /// Executa até `count` ciclos da CPU ou até encontrar uma instrução `HALT` legítima.
    /// Aborta ruidosamente caso ocorra qualquer `CpuTrap`.
    pub fn run_ticks(&mut self, mem: &mut Memory, count: usize) -> usize {
        let mut executed = 0;
        while executed < count {
            if !self.step(mem) {
                break;
            }
            executed += 1;
        }
        executed
    }
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::FRAMEBUFFER_BASE;
    use crate::word::Tryte6;

    #[test]
    fn test_instruction_encode_decode_roundtrip() {
        let cases = [
            Instruction::new(Opcode::Add, 0, 123_456),
            Instruction::new(Opcode::Sub, 4, -2_000_000),
            Instruction::new(Opcode::LoadIndex, 8, 4_600_000),
            Instruction::new(Opcode::JmpN, 2, 1024),
            Instruction::new(Opcode::JmpZ, 3, 2048),
            Instruction::new(Opcode::JmpP, 7, 4096),
            Instruction::new(Opcode::Call, 1, 512),
            Instruction::new(Opcode::Ret, 0, 0),
        ];

        for &orig in &cases {
            let word = orig.encode();
            let decoded = Instruction::decode(word).expect("Falha ao decodificar instrucao");
            assert_eq!(decoded.opcode, orig.opcode);
            assert_eq!(decoded.reg_idx, orig.reg_idx);
            assert_eq!(decoded.operand_as_address(), orig.operand_as_address());
        }
    }

    #[test]
    fn test_trichotomous_branches_without_flags() {
        let mut mem = Memory::new();
        let mut cpu = Cpu::new();

        // Programa que testa JMPN, JMPZ e JMPP inspecionando diretamente o sinal de ACC:
        // Endereço 100: valor -50
        // Endereço 101: valor 0
        // Endereço 102: valor +50
        // Endereço 200: resultado de sucesso
        mem.write_word(100, Word21::new(-50)).unwrap();
        mem.write_word(101, Word21::new(0)).unwrap();
        mem.write_word(102, Word21::new(50)).unwrap();

        let prog = [
            // 0: Carrega -50 e testa JMPN -> deve saltar para 3
            Instruction::new(Opcode::Load, 0, 100),
            Instruction::new(Opcode::JmpN, 0, 3),
            Instruction::new(Opcode::Halt, 0, 0), // 2: Falha se não saltar
            // 3: Neg (-(-50) = +50) e testa JMPP -> deve saltar para 6
            Instruction::new(Opcode::Neg, 0, 0),
            Instruction::new(Opcode::JmpP, 0, 6),
            Instruction::new(Opcode::Halt, 0, 0), // 5: Falha se não saltar
            // 6: Subtrai MEM[102] (+50 - 50 = 0) e testa JMPZ -> deve saltar para 9
            Instruction::new(Opcode::Sub, 0, 102),
            Instruction::new(Opcode::JmpZ, 0, 9),
            Instruction::new(Opcode::Halt, 0, 0), // 8: Falha se não saltar
            // 9: Sucesso! Grava 777 no endereço 200 e para
            Instruction::new(Opcode::LoadImm, 0, 777),
            Instruction::new(Opcode::Store, 0, 200),
            Instruction::new(Opcode::Halt, 0, 0),
        ];

        cpu.load_program(&mut mem, 0, &prog);
        cpu.run_ticks(&mut mem, 100);

        assert!(cpu.halted);
        assert_eq!(mem.read_word(200).unwrap().as_i64(), 777);
    }

    #[test]
    fn test_stack_call_ret_and_indexed_framebuffer_write() {
        let mut mem = Memory::new();
        let mut cpu = Cpu::new();

        // Sub-rotina em 50: escreve ACC no Framebuffer[IX[1]] e retorna
        let sub = [
            Instruction::new(Opcode::StoreIndex, 1, FRAMEBUFFER_BASE as i64),
            Instruction::new(Opcode::Ret, 0, 0),
        ];
        cpu.load_program(&mut mem, 50, &sub);

        let color = Tryte6::from_palette_index(180);
        let main_prog = [
            Instruction::new(Opcode::LoadIxImm, 1, 325), // Pixel offset 325
            Instruction::new(Opcode::LoadImm, 0, color.as_i64()),
            Instruction::new(Opcode::Call, 0, 50),
            Instruction::new(Opcode::Halt, 0, 0),
        ];
        cpu.load_program(&mut mem, 0, &main_prog);
        cpu.run_ticks(&mut mem, 50);

        assert!(cpu.halted);
        assert_eq!(cpu.sp, INITIAL_STACK_POINTER);
        let fb_val = mem
            .read_word((FRAMEBUFFER_BASE + 325) as i64)
            .unwrap()
            .as_i64();
        assert_eq!(fb_val, color.as_i64());
    }

    #[test]
    fn test_rebel6_cmpt_majvt_minvt_and_digit_flips_telemetry() {
        let mut mem = Memory::new();
        let mut cpu = Cpu::new();

        // Testa CMP.T (<=>), MAJV.T (voto majoritário) e telemetria de Digit Flips
        // quando números oscilam entre positivos e negativos (cenário destacado na Seção V.B do paper REBEL-6)
        mem.write_word(400, Word21::new(100)).unwrap();
        mem.write_word(401, Word21::new(-100)).unwrap();

        let prog = [
            // Oscila ACC entre -1, 0, +1, -2, +2 para medir economia de digit flips vs complemento de dois
            Instruction::new(Opcode::LoadImm, 0, -1),
            Instruction::new(Opcode::LoadImm, 0, 1),
            Instruction::new(Opcode::LoadImm, 0, -1),
            Instruction::new(Opcode::LoadImm, 0, 0),
            Instruction::new(Opcode::LoadImm, 0, -1),
            // Testa CMP.T: ACC (-1) <=> MEM[400] (100) -> -1
            Instruction::new(Opcode::CmpT, 0, 400),
            Instruction::new(Opcode::Store, 0, 410),
            // Testa CMP.T: ACC (100) <=> MEM[401] (-100) -> +1
            Instruction::new(Opcode::Load, 0, 400),
            Instruction::new(Opcode::CmpT, 0, 401),
            Instruction::new(Opcode::Store, 0, 411),
            // Testa MAJV.T e MINV.T:
            // ACC = +1, IX[0] = +1, MEM[401_sign] = -1 -> Majority = +1, Minority = -1
            Instruction::new(Opcode::LoadImm, 0, 1),
            Instruction::new(Opcode::LoadIxImm, 0, 1),
            Instruction::new(Opcode::MajvT, 0, 410), // MEM[410] vale -1 -> Maj(+1, +1, -1) = +1
            Instruction::new(Opcode::Store, 0, 412),
            Instruction::new(Opcode::MinvT, 0, 410), // Minv(+1, +1, -1) = -1
            Instruction::new(Opcode::Store, 0, 413),
            Instruction::new(Opcode::Halt, 0, 0),
        ];

        cpu.load_program(&mut mem, 0, &prog);
        cpu.run_ticks(&mut mem, 100);

        assert!(cpu.halted);
        assert_eq!(mem.read_word(410).unwrap().as_i64(), -1);
        assert_eq!(mem.read_word(411).unwrap().as_i64(), 1);
        assert_eq!(mem.read_word(412).unwrap().as_i64(), 1);
        assert_eq!(mem.read_word(413).unwrap().as_i64(), -1);

        // Verifica que os Trit Flips ternários são substancialmente menores que os Bit Flips binários de 32 bits
        assert!(cpu.telemetry.ternary_trit_flips < cpu.telemetry.binary32_bit_flips);
        assert!(cpu.telemetry.digit_flip_reduction_pct() > 50.0);
    }

    #[test]
    fn test_bceg_three_way_branch_and_compound_jumps() {
        let mut mem = Memory::new();
        let mut cpu = Cpu::new();

        // Testa BCEG despachando para 3 rotas distintas em 1 única instrução:
        // Compara ACC com IX[0] (que vale 10):
        // - Caso 1: ACC = 5  (< 10) -> fallthrough (PC + 1) -> grava 111 em MEM[300]
        // - Caso 2: ACC = 10 (== 10) -> salta offset_eq (+4) -> grava 222 em MEM[301]
        // - Caso 3: ACC = 20 (> 10) -> salta offset_gt (+7) -> grava 333 em MEM[302]
        let prog = [
            // 0: IX[0] = 10 (valor de referência para o BCEG)
            Instruction::new(Opcode::LoadIxImm, 0, 10),
            // 1: Caso 1: ACC = 5 (< IX[0])
            Instruction::new(Opcode::LoadImm, 0, 5),
            // 2: BCEG IX0, eq=+4 (vai p/ 6), gt=+7 (vai p/ 9) -> como 5 < 10, segue para 3!
            Instruction::new_bceg(0, 4, 7),
            // 3: Rota Less (< 10): grava 111 em MEM[300] e vai para o Caso 2 (PC = 5)
            Instruction::new(Opcode::LoadImm, 0, 111),
            Instruction::new(Opcode::Store, 0, 300),
            // 5: Caso 2: ACC = 10 (== IX[0]) -> BCEG em 6 salta para eq (+3 -> PC = 9)
            Instruction::new(Opcode::LoadImm, 0, 10),
            // 6: BCEG IX0, eq=+3 (vai p/ 9), gt=+6 (vai p/ 12)
            Instruction::new_bceg(0, 3, 6),
            // 7..8: armadilha se não saltar
            Instruction::new(Opcode::Halt, 0, 0),
            Instruction::new(Opcode::Halt, 0, 0),
            // 9: Rota Equal (== 10): grava 222 em MEM[301] e testa Caso 3
            Instruction::new(Opcode::LoadImm, 0, 222),
            Instruction::new(Opcode::Store, 0, 301),
            // 11: Caso 3: ACC = 20 (> IX[0])
            Instruction::new(Opcode::LoadImm, 0, 20),
            // 12: BCEG IX0, eq=+2 (vai p/ 14), gt=+3 (vai p/ 15)
            Instruction::new_bceg(0, 2, 3),
            // 13..14: armadilha se não saltar para gt
            Instruction::new(Opcode::Halt, 0, 0),
            Instruction::new(Opcode::Halt, 0, 0),
            // 15: Rota Greater (> 10): grava 333 em MEM[302] e para
            Instruction::new(Opcode::LoadImm, 0, 333),
            Instruction::new(Opcode::Store, 0, 302),
            Instruction::new(Opcode::Halt, 0, 0),
        ];

        cpu.load_program(&mut mem, 0, &prog);
        cpu.run_ticks(&mut mem, 100);

        assert!(cpu.halted);
        assert_eq!(mem.read_word(300).unwrap().as_i64(), 111);
        assert_eq!(mem.read_word(301).unwrap().as_i64(), 222);
        assert_eq!(mem.read_word(302).unwrap().as_i64(), 333);
    }

    #[test]
    fn test_cpu_traps_prohibit_silent_failures() {
        let mut mem = Memory::new();
        let mut cpu = Cpu::new();

        // 1. Opcode inválido (99 nos 5 trits de topo) deve disparar CpuTrap::InvalidInstruction
        let invalid_word = Word21::new(99 * crate::word::POW3[16]);
        mem.write_word(0, invalid_word).unwrap();
        let res1 = cpu.step_checked(&mut mem);
        assert_eq!(
            res1,
            Err(CpuTrap::InvalidInstruction {
                pc: 0,
                raw_word: invalid_word
            })
        );

        // 2. Divisão por zero em Opcode::DivInt deve disparar CpuTrap::DivisionByZero
        cpu.reset(10);
        mem.write_word(500, Word21::ZERO).unwrap();
        let div_zero_prog = [
            Instruction::new(Opcode::LoadImm, 0, 42),
            Instruction::new(Opcode::DivInt, 0, 500),
            Instruction::new(Opcode::Halt, 0, 0),
        ];
        cpu.load_program(&mut mem, 10, &div_zero_prog);
        assert_eq!(cpu.step_checked(&mut mem), Ok(true));
        assert_eq!(
            cpu.step_checked(&mut mem),
            Err(CpuTrap::DivisionByZero {
                pc: 11,
                opcode: Opcode::DivInt
            })
        );

        // 3. Acesso fora do espaço de 14 trits (ex: PC = 9_999_999) deve disparar CpuTrap::MemoryFault
        cpu.reset(9_999_999);
        assert_eq!(
            cpu.step_checked(&mut mem),
            Err(CpuTrap::MemoryFault {
                pc: 9_999_999,
                error: crate::memory::MemoryError::OutOfBounds(9_999_999)
            })
        );
    }

    #[test]
    fn test_setun58_setun70_authentic_instructions_and_alvarez_converter() {
        let mut mem = Memory::new();
        let mut cpu = Cpu::new();

        // 1. Verifica conversor recorrente 2->3 de Ramil Alvarez (1974) em portas lógicas
        for sample in [0i32, 1, -1, 42, -42, 59_049, -59_049, i32::MAX, i32::MIN] {
            let (trits, carries) = Word21::alvarez_from_i32_gates(sample);
            let w = Word21::from_trits(&trits);
            assert_eq!(w.as_i64(), sample as i64);
            if sample.unsigned_abs() > 1 {
                assert!(carries > 0);
            }
        }

        // 2. Verifica MUL_ZERO (++0), MAC_POS (+++ / LBT) e MAC_NEG (++-):
        // Produto vetorial 2D estilo BSP: dx * ldy - dy * ldx
        // Seja dx = 4.0, ldy = 3.0, dy = 2.0, ldx = 5.0 -> 4*3 - 2*5 = 12 - 10 = 2.0
        mem.write_word(600, TernaryFixed::from_int(4).raw_word()).unwrap(); // dx = 4.0
        mem.write_word(601, TernaryFixed::from_int(3).raw_word()).unwrap(); // ldy = 3.0
        mem.write_word(602, TernaryFixed::from_int(2).raw_word()).unwrap(); // dy = 2.0
        mem.write_word(603, TernaryFixed::from_int(5).raw_word()).unwrap(); // ldx = 5.0

        // 3. Verifica SMT (B19), SAT (B21) e SIT (B20) da Setun-70:
        // Seja A = 0t+-0 e B = 0t+--
        // SMT(+-0, +--) = (+*+, -*-, 0*-) = ++0 (+1*9 + 1*3 + 0 = 12)
        // SAT(+-0, +--) = (thresh(+,+), thresh(-,-), thresh(0,-)) = +-- (+1*9 - 1*3 - 1 = 5)
        // SIT(+-0, +--) = (+, -, -) pois apenas o trit Zero em A assume o trit '-' de B = +-- (5)
        let word_a = Word21::from_balanced_str("+-0").unwrap();
        let word_b = Word21::from_balanced_str("+--").unwrap();
        mem.write_word(610, word_a).unwrap();
        mem.write_word(611, word_b).unwrap();

        let prog = [
            // ACC = dx (4.0); MUL 601 (++0 Setun-58) -> R = 4.0, ACC = 3.0 * 4.0 = 12.0
            Instruction::new(Opcode::Load, 0, 600),
            Instruction::new(Opcode::Mul, 0, 601),
            // R = dy (2.0); MAC_NEG 603 -> ACC = 12.0 - 5.0 * 2.0 = 2.0
            Instruction::new(Opcode::LoadR, 0, 602),
            Instruction::new(Opcode::MacNeg, 0, 603),
            Instruction::new(Opcode::Store, 0, 604),
            // MAC_POS 603 -> ACC = 2.0 + 5.0 * 2.0 = 12.0
            Instruction::new(Opcode::MacPos, 0, 603),
            Instruction::new(Opcode::Store, 0, 605),
            // SMT: A (*) B -> ++0 (12)
            Instruction::new(Opcode::Load, 0, 610),
            Instruction::new(Opcode::Smt, 0, 611),
            Instruction::new(Opcode::Store, 0, 612),
            // SAT: ThresholdAdd(A, B) -> +-- (5)
            Instruction::new(Opcode::Load, 0, 610),
            Instruction::new(Opcode::Sat, 0, 611),
            Instruction::new(Opcode::Store, 0, 613),
            // SIT: A + B * (1 - |A|) -> +-- (5)
            Instruction::new(Opcode::Load, 0, 610),
            Instruction::new(Opcode::Sit, 0, 611),
            Instruction::new(Opcode::Store, 0, 614),
            Instruction::new(Opcode::Halt, 0, 0),
        ];

        cpu.load_program(&mut mem, 0, &prog);
        cpu.run_ticks(&mut mem, 50);

        assert!(cpu.halted);
        assert_eq!(
            mem.read_word(604).unwrap(),
            TernaryFixed::from_int(2).raw_word()
        );
        assert_eq!(
            mem.read_word(605).unwrap(),
            TernaryFixed::from_int(12).raw_word()
        );
        assert_eq!(mem.read_word(612).unwrap().as_i64(), 12);
        assert_eq!(mem.read_word(613).unwrap().as_i64(), 5);
        assert_eq!(mem.read_word(614).unwrap().as_i64(), 5);
    }
}

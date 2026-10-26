//! Subsistema de I/O Mapeado em Memória (MMO / MMIO), Controlador DMA/Loader de Arquivos Binários
//! (`DOOM.WAD`) e Interface Nativa / FFI para `doomgeneric`.
//!
//! Este módulo implementa:
//! 1. **DMA/Loader Ternário (`WadDmaLoader`)**: lê fluxos de bytes binários (ex: `DOOM.WAD`)
//!    do disco ou de buffers em memória e transcodifica em palavras ternárias balanceadas (`Word21`),
//!    suportando tanto:
//!    - Modo `TrytePerByte` (1 byte binário -> 1 `Tryte6` de 6 trits centrado em `[-128, +127]`).
//!    - Modo `PackedI32` (grupos de 4 bytes Little-Endian `i32` -> 1 `Word21` de 21 trits,
//!      já que $3^{21} > 2^{32}$).
//!    - Parser de cabeçalho WAD (`IWAD`/`PWAD`) capaz de localizar o lump `PLAYPAL` e atualizar
//!      automaticamente a paleta VGA da memória.
//! 2. **Ponte de Plataforma `doomgeneric`**: exporta as 5 funções obrigatórias com ABI C (`extern "C"`):
//!    - `DG_Init`
//!    - `DG_DrawFrame`
//!    - `DG_SleepMs`
//!    - `DG_GetTicksMs`
//!    - `DG_GetKey`

use crate::cpu::Cpu;
use crate::memory::{
    Memory, MemoryError, FB_SIZE, MMIO_KEY_STATE, MMIO_TICKS_MS, MMIO_WAD_SIZE, WAD_DMA_BASE,
};
use crate::word::{Tryte6, Word21};
use std::collections::VecDeque;
use std::fs;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

/// Modos de transcodificação binário-para-ternário durante transferência DMA.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DmaTranscodeMode {
    /// Cada byte `u8` (`0..=255`) é transcodificado em uma palavra contendo um `Tryte6`
    /// balanceado (`byte as i16 - 128` em `[-128, +127]`).
    TrytePerByte,
    /// Cada bloco de 4 bytes Little-Endian (`i32`) é transcodificado diretamente em uma
    /// `Word21` ternária balanceada (1 palavra de 21 trits por inteiro de 32 bits).
    PackedI32LittleEndian,
}

/// Descritor de um Lump extraído do diretório de um arquivo WAD (`IWAD` ou `PWAD`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WadLumpInfo {
    /// Nome ASCII de até 8 caracteres do lump (ex: `"PLAYPAL"`, `"E1M1"`, `"VERTEXES"`).
    pub name: String,
    /// Offset em bytes no arquivo WAD original.
    pub file_pos: usize,
    /// Tamanho em bytes do lump.
    pub size: usize,
    /// Endereço base na memória ternária onde o lump foi carregado (se mapeado).
    pub ternary_addr: usize,
}

/// Controlador de DMA e Loader capaz de ler arquivos binários (`DOOM.WAD`) e transcodificar
/// em palavras ternárias balanceadas na inicialização.
pub struct WadDmaLoader;

impl WadDmaLoader {
    /// Transcodifica um slice de bytes binários para a memória ternária a partir de `base_addr`.
    /// Retorna o número de palavras `Word21` gravadas na memória.
    pub fn load_bytes(
        mem: &mut Memory,
        base_addr: usize,
        bytes: &[u8],
        mode: DmaTranscodeMode,
    ) -> Result<usize, MemoryError> {
        match mode {
            DmaTranscodeMode::TrytePerByte => {
                if base_addr + bytes.len() > mem.raw_cells().len() {
                    return Err(MemoryError::DmaOverflow {
                        base: base_addr,
                        len: bytes.len(),
                    });
                }
                for (i, &b) in bytes.iter().enumerate() {
                    let tryte = Tryte6::from_u8_centered(b);
                    mem.write_word_fast((base_addr + i) as i64, Word21::new(tryte.as_i64()));
                }
                mem.write_word_fast(MMIO_WAD_SIZE as i64, Word21::new(bytes.len() as i64));
                Ok(bytes.len())
            }
            DmaTranscodeMode::PackedI32LittleEndian => {
                let word_count = (bytes.len() + 3) / 4;
                if base_addr + word_count > mem.raw_cells().len() {
                    return Err(MemoryError::DmaOverflow {
                        base: base_addr,
                        len: word_count,
                    });
                }
                for (chunk_idx, chunk) in bytes.chunks(4).enumerate() {
                    let mut buf = [0u8; 4];
                    buf[..chunk.len()].copy_from_slice(chunk);
                    let val_i32 = i32::from_le_bytes(buf);
                    mem.write_word_fast(
                        (base_addr + chunk_idx) as i64,
                        Word21::from_i32(val_i32),
                    );
                }
                mem.write_word_fast(MMIO_WAD_SIZE as i64, Word21::new(word_count as i64));
                Ok(word_count)
            }
        }
    }

    /// Reconstrói bytes binários a partir de palavras ternárias carregadas em `TrytePerByte`.
    pub fn read_bytes_tryte(mem: &Memory, base_addr: usize, len: usize) -> Result<Vec<u8>, MemoryError> {
        if base_addr + len > mem.raw_cells().len() {
            return Err(MemoryError::OutOfBounds((base_addr + len) as i64));
        }
        let mut out = Vec::with_capacity(len);
        for i in 0..len {
            let w = mem.read_word_fast((base_addr + i) as i64);
            let tryte = Tryte6::from_i64(w.as_i64());
            out.push(tryte.to_u8_centered());
        }
        Ok(out)
    }

    /// Lê um arquivo binário de disco (como `DOOM.WAD` ou `DOOM1.WAD`), transcodifica seu conteúdo
    /// para a memória ternária em `base_addr` e, caso seja um arquivo WAD válido contendo o lump
    /// `PLAYPAL`, aplica automaticamente a paleta de 256 cores na região VGA da memória.
    pub fn load_wad_file<P: AsRef<Path>>(
        mem: &mut Memory,
        path: P,
        base_addr: usize,
        mode: DmaTranscodeMode,
    ) -> Result<(usize, Vec<WadLumpInfo>), String> {
        let bytes = fs::read(path.as_ref())
            .map_err(|e| format!("Falha ao ler arquivo WAD {:?}: {}", path.as_ref(), e))?;
        Self::load_wad_from_slice(mem, &bytes, base_addr, mode)
    }

    /// Analisa um buffer WAD (`IWAD`/`PWAD`), extrai seu diretório de lumps, carrega `PLAYPAL` se
    /// presente e transcodifica a imagem binária completa para a memória ternária via DMA.
    pub fn load_wad_from_slice(
        mem: &mut Memory,
        bytes: &[u8],
        base_addr: usize,
        mode: DmaTranscodeMode,
    ) -> Result<(usize, Vec<WadLumpInfo>), String> {
        let words_written = Self::load_bytes(mem, base_addr, bytes, mode)
            .map_err(|e| format!("Erro de DMA ao carregar WAD: {:?}", e))?;

        let mut lumps = Vec::new();
        if bytes.len() >= 12 && (&bytes[0..4] == b"IWAD" || &bytes[0..4] == b"PWAD") {
            let num_lumps = i32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
            let info_table_ofs =
                i32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;

            if info_table_ofs + num_lumps.saturating_mul(16) <= bytes.len() {
                for i in 0..num_lumps {
                    let entry_ofs = info_table_ofs + i * 16;
                    let file_pos = i32::from_le_bytes([
                        bytes[entry_ofs],
                        bytes[entry_ofs + 1],
                        bytes[entry_ofs + 2],
                        bytes[entry_ofs + 3],
                    ]) as usize;
                    let size = i32::from_le_bytes([
                        bytes[entry_ofs + 4],
                        bytes[entry_ofs + 5],
                        bytes[entry_ofs + 6],
                        bytes[entry_ofs + 7],
                    ]) as usize;
                    let name_bytes = &bytes[entry_ofs + 8..entry_ofs + 16];
                    let name_len = name_bytes
                        .iter()
                        .position(|&b| b == 0)
                        .unwrap_or(name_bytes.len());
                    let name = String::from_utf8_lossy(&name_bytes[..name_len]).to_string();

                    if name == "PLAYPAL" && file_pos + 768 <= bytes.len() && size >= 768 {
                        mem.load_playpal(&bytes[file_pos..file_pos + 768]);
                    }

                    let ternary_addr = match mode {
                        DmaTranscodeMode::TrytePerByte => base_addr + file_pos,
                        DmaTranscodeMode::PackedI32LittleEndian => base_addr + (file_pos / 4),
                    };

                    lumps.push(WadLumpInfo {
                        name,
                        file_pos,
                        size,
                        ternary_addr,
                    });
                }
            }
        }

        Ok((words_written, lumps))
    }
}

/// Evento de teclado enfileirado para consumo por `DG_GetKey`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    /// `true` se a tecla foi pressionada (`1`), `false` se foi liberada (`0`).
    pub pressed: bool,
    /// Código de tecla compatível com `doomkeys.h`.
    pub doom_key: u8,
}

/// Endereços de entrada da ROM de Firmware Ternário residente na `SetunMemory`.
pub const ROM_FIXED_MUL_ENTRY: i64 = 100;
pub const ROM_FIXED_DIV_ENTRY: i64 = 120;
pub const ROM_POINT_ON_SIDE_ENTRY: i64 = 140;
pub const ROM_APROX_DIST_ENTRY: i64 = 220;

/// Máquina Virtual Integrada (`SetunDoomMachine`) unificando CPU, Memória, Framebuffer RGBA,
/// Relógio Monotônico e Fila de Eventos de Teclado para integração com `doomgeneric`.
pub struct SetunDoomMachine {
    pub cpu: Cpu,
    pub memory: Memory,
    pub start_instant: Instant,
    pub key_queue: VecDeque<KeyEvent>,
    /// Buffer RGBA (`320 * 200 * 4` bytes) atualizado a cada chamada de `DG_DrawFrame`.
    pub rgba_frame: Vec<u8>,
    /// Contador de quadros renderizados.
    pub frames_drawn: u64,
}

impl SetunDoomMachine {
    /// Cria e inicializa a máquina Setun-Doom completa e grava o Firmware ROM ternário na memória.
    pub fn new() -> Self {
        let mut machine = Self {
            cpu: Cpu::new(),
            memory: Memory::new(),
            start_instant: Instant::now(),
            key_queue: VecDeque::with_capacity(64),
            rgba_frame: vec![0u8; FB_SIZE * 4],
            frames_drawn: 0,
        };
        machine.install_firmware_rom();
        machine
    }

    /// Compila e carrega na `SetunMemory` as sub-rotinas de firmware em código de máquina
    /// ternário de 21 trits (`ROM_FIXED_MUL`, `ROM_FIXED_DIV`, `ROM_POINT_ON_SIDE`).
    fn install_firmware_rom(&mut self) {
        use crate::assembler::SetunAssembler;

        let mul_asm = r#"
            LOAD 50
            MUL 51
            STORE 52
            HALT
        "#;
        let mul_prog = SetunAssembler::assemble_at(mul_asm, ROM_FIXED_MUL_ENTRY)
            .expect("Falha ao montar ROM_FIXED_MUL na inicializacao");
        self.cpu
            .load_program(&mut self.memory, ROM_FIXED_MUL_ENTRY, &mul_prog);

        let div_asm = r#"
            LOAD 50
            DIV 51
            STORE 52
            HALT
        "#;
        let div_prog = SetunAssembler::assemble_at(div_asm, ROM_FIXED_DIV_ENTRY)
            .expect("Falha ao montar ROM_FIXED_DIV na inicializacao");
        self.cpu
            .load_program(&mut self.memory, ROM_FIXED_DIV_ENTRY, &div_prog);

        let pos_asm = r#"
            LOAD 64
            JMP_Z .ldx_is_zero
            LOAD 65
            JMP_Z .ldy_is_zero
            LOAD 60
            SUB 62
            STORE 66
            LOAD 61
            SUB 63
            STORE 67
            LOAD 65
            SHIFT -10
            STORE 68
            LOAD 64
            SHIFT -10
            MUL_ZERO 67
            LOAD_R 66
            MAC_NEG 68
            JMP_N .ret_zero
            JMP .ret_one
        .ldx_is_zero:
            LOAD 60
            CMP_T 62
            JMP_LE .ldx0_x_le_lx
            LOAD 65
            JMP_N .ret_one
            JMP .ret_zero
        .ldx0_x_le_lx:
            LOAD 65
            JMP_P .ret_one
            JMP .ret_zero
        .ldy_is_zero:
            LOAD 61
            CMP_T 63
            JMP_LE .ldy0_y_le_ly
            LOAD 64
            JMP_P .ret_one
            JMP .ret_zero
        .ldy0_y_le_ly:
            LOAD 64
            JMP_N .ret_one
            JMP .ret_zero
        .ret_one:
            LOAD_IMM 1
            STORE 69
            HALT
        .ret_zero:
            LOAD_IMM 0
            STORE 69
            HALT
        "#;
        let pos_prog = SetunAssembler::assemble_at(pos_asm, ROM_POINT_ON_SIDE_ENTRY)
            .expect("Falha ao montar ROM_POINT_ON_SIDE na inicializacao");
        self.cpu
            .load_program(&mut self.memory, ROM_POINT_ON_SIDE_ENTRY, &pos_prog);

        // Constante 0.5 em TernaryFixed (11.10) em MEM[73] para P_AproxDistance
        self.memory
            .write_word_fast(73, crate::fixed::TernaryFixed::from_f64(0.5).raw_word());

        let dist_asm = r#"
            LOAD 70
            JMP_GE .dx_pos
            STI.T
            STORE 70
        .dx_pos:
            LOAD 71
            JMP_GE .dy_pos
            STI.T
            STORE 71
        .dy_pos:
            LOAD 70
            ADD 71
            STORE 72
            LOAD 70
            CMP_T 71
            JMP_N .dx_smaller
            LOAD 71
            JMP .sub_half
        .dx_smaller:
            LOAD 70
        .sub_half:
            MUL 73
            STORE 74
            LOAD 72
            SUB 74
            STORE 75
            HALT
        "#;
        let dist_prog = SetunAssembler::assemble_at(dist_asm, ROM_APROX_DIST_ENTRY)
            .expect("Falha ao montar ROM_APROX_DIST na inicializacao");
        self.cpu
            .load_program(&mut self.memory, ROM_APROX_DIST_ENTRY, &dist_prog);
    }

    /// Executa uma rotina da ROM de firmware a partir de `entry_pc` até atingir `HALT`,
    /// abortando imediatamente se qualquer `CpuTrap` ocorrer ou se não parar em `max_cycles`.
    #[inline]
    pub fn execute_rom_routine(&mut self, entry_pc: i64, max_cycles: usize) {
        self.cpu.pc = entry_pc;
        self.cpu.halted = false;
        self.cpu.trap = None;
        self.cpu.run_ticks(&mut self.memory, max_cycles);
        assert!(
            self.cpu.halted && self.cpu.trap.is_none(),
            "Rotina ROM em PC={} nao concluiu em HALT limpo (halted={}, trap={:?})",
            entry_pc,
            self.cpu.halted,
            self.cpu.trap
        );
    }

    /// Reinicia o relógio e os registradores de plataforma (`DG_Init`).
    pub fn dg_init(&mut self) {
        self.start_instant = Instant::now();
        self.key_queue.clear();
        self.frames_drawn = 0;
        self.memory
            .write_word_fast(MMIO_TICKS_MS as i64, Word21::ZERO);
        self.memory
            .write_word_fast(MMIO_KEY_STATE as i64, Word21::ZERO);
    }

    /// Lê a faixa de memória de 64.000 `Tryte6` do framebuffer ternário e traduz para o buffer
    /// RGBA (`320x200x4` bytes) consumível por uma janela gráfica no host (`DG_DrawFrame`).
    pub fn dg_draw_frame(&mut self) -> &[u8] {
        self.memory.render_framebuffer_rgba(&mut self.rgba_frame);
        self.frames_drawn += 1;
        &self.rgba_frame
    }

    /// Pausa a thread atual por `ms` milissegundos (`DG_SleepMs`).
    pub fn dg_sleep_ms(&self, ms: u32) {
        if ms > 0 {
            thread::sleep(Duration::from_millis(ms as u64));
        }
    }

    /// Retorna o tempo decorrido desde `dg_init` em milissegundos e atualiza o registrador
    /// mapeado em memória `MMIO_TICKS_MS` (`DG_GetTicksMs`).
    pub fn dg_get_ticks_ms(&mut self) -> u32 {
        let elapsed = self.start_instant.elapsed().as_millis() as u32;
        self.memory
            .write_word_fast(MMIO_TICKS_MS as i64, Word21::new(elapsed as i64));
        elapsed
    }

    /// Enfileira um evento de tecla no controlador de teclado e reflete no MMIO.
    pub fn push_key_event(&mut self, pressed: bool, doom_key: u8) {
        self.key_queue.push_back(KeyEvent { pressed, doom_key });
        // Codifica estado no MMIO_KEY_STATE: sinal (+1 = pressionado, -1 = solto) * (doom_key + 1)
        let sign = if pressed { 1i64 } else { -1i64 };
        let encoded = sign * ((doom_key as i64) + 1);
        self.memory
            .write_word_fast(MMIO_KEY_STATE as i64, Word21::new(encoded));
    }

    /// Desenfileira o próximo evento de teclado (`DG_GetKey`).
    /// Retorna `Some((pressed, doom_key))` se houver tecla pendente, ou `None` se a fila estiver vazia.
    pub fn dg_get_key(&mut self) -> Option<(i32, u8)> {
        if let Some(ev) = self.key_queue.pop_front() {
            Some((if ev.pressed { 1 } else { 0 }, ev.doom_key))
        } else {
            self.memory
                .write_word_fast(MMIO_KEY_STATE as i64, Word21::ZERO);
            None
        }
    }

    /// Carrega um buffer WAD na região padrão `WAD_DMA_BASE` via DMA.
    pub fn load_wad_bytes(&mut self, bytes: &[u8]) -> Result<(usize, Vec<WadLumpInfo>), String> {
        WadDmaLoader::load_wad_from_slice(
            &mut self.memory,
            bytes,
            WAD_DMA_BASE,
            DmaTranscodeMode::TrytePerByte,
        )
    }

    /// Carrega um programa de instruções na memória ternária e posiciona `PC` em `base_addr`.
    pub fn load_program(&mut self, base_addr: i64, instructions: &[crate::cpu::Instruction]) {
        self.cpu.load_program(&mut self.memory, base_addr, instructions);
    }

    /// Executa um único passo da CPU (`step()`).
    #[inline(always)]
    pub fn step(&mut self) -> bool {
        self.cpu.step(&mut self.memory)
    }

    /// Executa até `count` ciclos da CPU (`run_ticks(count)`).
    pub fn run_ticks(&mut self, count: usize) -> usize {
        self.cpu.run_ticks(&mut self.memory, count)
    }
}

impl Default for SetunDoomMachine {
    fn default() -> Self {
        Self::new()
    }
}

/// Instância global thread-safe para atendimento direto das chamadas C FFI do `doomgeneric`.
static GLOBAL_MACHINE: OnceLock<Mutex<SetunDoomMachine>> = OnceLock::new();

/// Obtém referência ao mutex da máquina global Setun-Doom.
pub fn global_machine() -> &'static Mutex<SetunDoomMachine> {
    GLOBAL_MACHINE.get_or_init(|| Mutex::new(SetunDoomMachine::new()))
}

// ============================================================================
// Bindings C FFI para integração direta com `doomgeneric`
// ============================================================================

/// Inicializa a plataforma Setun-Doom para o loop principal do `doomgeneric`.
#[no_mangle]
pub extern "C" fn DG_Init() {
    if let Ok(mut machine) = global_machine().lock() {
        machine.dg_init();
    }
}

/// Traduz os 64.000 `Tryte6` do framebuffer ternário para o buffer RGBA de janela gráfica.
/// Se um ponteiro opcional `out_rgba_buffer` (`*mut u8` de 256.000 bytes) for fornecido pelo host,
/// copia também os pixels RGBA para o buffer externo.
#[no_mangle]
pub extern "C" fn DG_DrawFrame() {
    if let Ok(mut machine) = global_machine().lock() {
        let _ = machine.dg_draw_frame();
    }
}

/// Variante FFI estendida que copia o framebuffer RGBA renderizado diretamente para um buffer C
/// `out_rgba` de `320 * 200 * 4 = 256.000` bytes.
///
/// # Safety
/// `out_rgba` deve ser nulo ou apontar para pelo menos `256.000` bytes válidos para escrita.
#[no_mangle]
pub unsafe extern "C" fn DG_DrawFrameToBuffer(out_rgba: *mut u8) {
    if let Ok(mut machine) = global_machine().lock() {
        let rgba = machine.dg_draw_frame();
        if !out_rgba.is_null() {
            core::ptr::copy_nonoverlapping(rgba.as_ptr(), out_rgba, FB_SIZE * 4);
        }
    }
}

/// Suspende a execução por `ms` milissegundos.
#[no_mangle]
pub extern "C" fn DG_SleepMs(ms: u32) {
    if let Ok(machine) = global_machine().lock() {
        machine.dg_sleep_ms(ms);
    } else if ms > 0 {
        thread::sleep(Duration::from_millis(ms as u64));
    }
}

/// Retorna a contagem de milissegundos desde `DG_Init`.
#[no_mangle]
pub extern "C" fn DG_GetTicksMs() -> u32 {
    if let Ok(mut machine) = global_machine().lock() {
        machine.dg_get_ticks_ms()
    } else {
        0
    }
}

/// Consulta a fila de eventos de teclado do `doomgeneric`.
/// Preenche `*pressed` (`1` ou `0`) e `*doom_key` e retorna `1` se havia evento na fila, ou `0` caso contrário.
///
/// # Safety
/// `pressed` e `doom_key` devem ser ponteiros válidos e alinhados caso a fila contenha eventos.
#[no_mangle]
pub unsafe extern "C" fn DG_GetKey(pressed: *mut i32, doom_key: *mut u8) -> i32 {
    if pressed.is_null() || doom_key.is_null() {
        return 0;
    }
    if let Ok(mut machine) = global_machine().lock() {
        if let Some((is_pressed, key)) = machine.dg_get_key() {
            *pressed = is_pressed;
            *doom_key = key;
            return 1;
        }
    }
    0
}

/// Define o título da janela gráfica (`DG_SetWindowTitle` do `doomgeneric`).
#[no_mangle]
pub extern "C" fn DG_SetWindowTitle(_title: *const core::ffi::c_char) {}

/// Enfileira um evento de tecla vindo da janela SDL2/X11 diretamente no registrador
/// `MMIO_KEY_STATE` e na fila do controlador de teclado da máquina Setun-Doom.
#[no_mangle]
pub extern "C" fn setun_PushKeyEvent(pressed: i32, doom_key: u8) {
    if let Ok(mut machine) = global_machine().lock() {
        machine.push_key_event(pressed != 0, doom_key);
    }
}

// ============================================================================
// Ponte de Aritmética Ternária e Framebuffer para o motor C do `doomgeneric`
// ============================================================================

/// Substitui `FixedMul(fixed_t a, fixed_t b)` do `m_fixed.c` do Doom:
/// Grava os operandos em `MEM[50..51]`, executa o programa ternário `ROM_FIXED_MUL_ENTRY`
/// via `Cpu::step_checked()` (ciclo Fetch-Decode-Execute completo usando o multiplicador
/// físico de 42 trits e telemetria sem viés de base $3^{10}$ vs $2^{16}$) e lê `MEM[52]`.
#[no_mangle]
pub extern "C" fn setun_FixedMul(a: i32, b: i32) -> i32 {
    let ta = crate::fixed::TernaryFixed::from_doom_fixed16(a);
    let tb = crate::fixed::TernaryFixed::from_doom_fixed16(b);
    let mut machine = global_machine()
        .lock()
        .expect("Mutex da SetunDoomMachine corrompido em setun_FixedMul");
    machine.memory.write_word_fast(50, ta.raw_word());
    machine.memory.write_word_fast(51, tb.raw_word());
    machine.execute_rom_routine(ROM_FIXED_MUL_ENTRY, 8);
    let prod_word = machine.memory.read_word_fast(52);
    crate::fixed::TernaryFixed::from_raw(prod_word).to_doom_fixed16()
}

/// Substitui `FixedDiv(fixed_t a, fixed_t b)` do `m_fixed.c` do Doom:
/// Grava os operandos em `MEM[50..51]` e executa o programa ternário `ROM_FIXED_DIV_ENTRY`
/// via `Cpu::step_checked()`.
#[no_mangle]
pub extern "C" fn setun_FixedDiv(a: i32, b: i32) -> i32 {
    if (a.unsigned_abs() >> 14) >= b.unsigned_abs() {
        return if (a ^ b) < 0 { i32::MIN } else { i32::MAX };
    }
    let ta = crate::fixed::TernaryFixed::from_doom_fixed16(a);
    let tb = crate::fixed::TernaryFixed::from_doom_fixed16(b);
    let mut machine = global_machine()
        .lock()
        .expect("Mutex da SetunDoomMachine corrompido em setun_FixedDiv");
    machine.memory.write_word_fast(50, ta.raw_word());
    machine.memory.write_word_fast(51, tb.raw_word());
    machine.execute_rom_routine(ROM_FIXED_DIV_ENTRY, 8);
    let quot_word = machine.memory.read_word_fast(52);
    crate::fixed::TernaryFixed::from_raw(quot_word).to_doom_fixed16()
}

/// Executa `R_PointOnSide` / `R_PointOnSegSide` / `P_PointOnLineSide` do Doom
/// inteiramente dentro da CPU Setun-Doom via `ROM_POINT_ON_SIDE_ENTRY` (`PC=140`),
/// mantendo todos os valores intermediários (`dx`, `dy`, `left`, `right`) em `TernaryFixed` (`11.10`)
/// sem dupla quantização `16.16 <-> 11.10` no meio do determinante.
#[no_mangle]
pub extern "C" fn setun_PointOnSide(
    x: i32,
    y: i32,
    lx: i32,
    ly: i32,
    ldx: i32,
    ldy: i32,
) -> i32 {
    let tx = crate::fixed::TernaryFixed::from_doom_fixed16(x).raw_word();
    let ty = crate::fixed::TernaryFixed::from_doom_fixed16(y).raw_word();
    let tlx = crate::fixed::TernaryFixed::from_doom_fixed16(lx).raw_word();
    let tly = crate::fixed::TernaryFixed::from_doom_fixed16(ly).raw_word();
    let tldx = crate::fixed::TernaryFixed::from_doom_fixed16(ldx).raw_word();
    let tldy = crate::fixed::TernaryFixed::from_doom_fixed16(ldy).raw_word();

    let mut machine = global_machine()
        .lock()
        .expect("Mutex da SetunDoomMachine corrompido em setun_PointOnSide");
    machine.memory.write_word_fast(60, tx);
    machine.memory.write_word_fast(61, ty);
    machine.memory.write_word_fast(62, tlx);
    machine.memory.write_word_fast(63, tly);
    machine.memory.write_word_fast(64, tldx);
    machine.memory.write_word_fast(65, tldy);
    machine.execute_rom_routine(ROM_POINT_ON_SIDE_ENTRY, 32);
    machine.memory.read_word_fast(69).as_i64() as i32
}

/// Executa `P_AproxDistance(dx, dy)` do `p_maputl.c` inteiramente na CPU Setun-Doom
/// via `ROM_APROX_DIST_ENTRY` (`PC=220`), usando `STI.T` para valor absoluto ternário,
/// `CMP_T` para mínimo e `ADD`/`SUB`/`MUL` em `TernaryFixed` (`11.10`).
#[no_mangle]
pub extern "C" fn setun_AproxDistance(dx: i32, dy: i32) -> i32 {
    let tdx = crate::fixed::TernaryFixed::from_doom_fixed16(dx).raw_word();
    let tdy = crate::fixed::TernaryFixed::from_doom_fixed16(dy).raw_word();
    let mut machine = global_machine()
        .lock()
        .expect("Mutex corrompido em setun_AproxDistance");
    machine.memory.write_word_fast(70, tdx);
    machine.memory.write_word_fast(71, tdy);
    machine.execute_rom_routine(ROM_APROX_DIST_ENTRY, 32);
    let res_word = machine.memory.read_word_fast(75);
    crate::fixed::TernaryFixed::from_raw(res_word).to_doom_fixed16()
}

/// Executa o interpolador DDA de colunas de parede/sprite `R_DrawColumn` do `r_draw.c`
/// usando acumulador `TernaryFixed` (`11.10` trits, escala $3^{10} = 59.049$) e o somador
/// Ripple-Carry de 21 estágios (`Word21::gate_ripple_carry_add`) a cada pixel vertical.
///
/// # Safety
/// `dest`, `dc_source` e `dc_colormap` devem ser ponteiros válidos para o framebuffer e lumps do Doom.
#[no_mangle]
pub unsafe extern "C" fn setun_DrawColumn(
    dc_x: i32,
    dc_yl: i32,
    dc_yh: i32,
    centery: i32,
    dc_iscale: i32,
    dc_texturemid: i32,
    dc_source: *const u8,
    dc_colormap: *const u8,
    dest_start: *mut u8,
    screenwidth: i32,
) {
    let count = dc_yh - dc_yl;
    if count < 0 {
        return;
    }
    assert!(
        !dc_source.is_null() && !dc_colormap.is_null() && !dest_start.is_null(),
        "Ponteiro nulo em setun_DrawColumn"
    );
    assert!(
        dc_x >= 0 && (dc_x as usize) < crate::memory::FB_WIDTH && dc_yl >= 0 && (dc_yh as usize) < crate::memory::FB_HEIGHT,
        "Coordenadas fora de faixa em setun_DrawColumn: x={}, yl={}, yh={}",
        dc_x,
        dc_yl,
        dc_yh
    );

    // Passo e coordenada inicial em TernaryFixed (11.10 trits, escala 3^10 = 59049)
    let frac16_init = dc_texturemid.wrapping_add((dc_yl - centery).wrapping_mul(dc_iscale));
    let mut t_frac = crate::fixed::TernaryFixed::from_doom_fixed16(frac16_init).raw_word();
    let t_step = crate::fixed::TernaryFixed::from_doom_fixed16(dc_iscale).raw_word();

    let mut b_frac16 = frac16_init;
    let b_step16 = dc_iscale;

    let mut machine = global_machine()
        .lock()
        .expect("Mutex corrompido em setun_DrawColumn");

    let mut dest_ptr = dest_start;
    let mut y = dc_yl as usize;
    let x = dc_x as usize;

    for _ in 0..=count {
        // Extrai a linha inteira de textura deslocando 10 trits fisicamente nos fios (sem divisao binaria)
        let int_row = t_frac.shift_trits(-10).as_i64();
        let tex_row = (int_row.rem_euclid(128)) as usize;
        let raw_texel = *dc_source.add(tex_row) as usize;
        let lit_color = *dc_colormap.add(raw_texel);

        *dest_ptr = lit_color;
        let fb_offset = y * crate::memory::FB_WIDTH + x;
        machine.memory.raw_cells_mut()[crate::memory::FRAMEBUFFER_BASE + fb_offset] =
            Tryte6::from_palette_index(lit_color).as_i64();

        // Avança frac += fracstep usando o somador Ripple-Carry de 21 portas `Trit::full_add`
        let (next_t_frac, _cout, carries) =
            t_frac.gate_ripple_carry_add(t_step, crate::trit::Trit::Zero);
        let next_b_frac16 = b_frac16.wrapping_add(b_step16);

        machine.cpu.telemetry.adder_carry_propagations += carries as u64;
        machine
            .cpu
            .telemetry
            .record_fixed16_transition(t_frac, next_t_frac, b_frac16, next_b_frac16);
        machine.cpu.cycles += 1;

        t_frac = next_t_frac;
        b_frac16 = next_b_frac16;
        dest_ptr = dest_ptr.add(screenwidth as usize);
        y += 1;
    }
    machine.cpu.acc = t_frac;
}

/// Executa o interpolador de colunas com mapeamento de cores `R_DrawTranslatedColumn` do `r_draw.c`
/// (usado para sprites de jogadores e variações de monstros) usando acumulador `TernaryFixed`
/// e o somador Ripple-Carry de 21 portas `Trit::full_add`.
///
/// # Safety
/// Ponteiros devem ser válidos para leitura e escrita no framebuffer.
#[no_mangle]
pub unsafe extern "C" fn setun_DrawTranslatedColumn(
    dc_x: i32,
    dc_yl: i32,
    dc_yh: i32,
    centery: i32,
    dc_iscale: i32,
    dc_texturemid: i32,
    dc_source: *const u8,
    dc_colormap: *const u8,
    dc_translation: *const u8,
    dest_start: *mut u8,
    screenwidth: i32,
) {
    let count = dc_yh - dc_yl;
    if count < 0 {
        return;
    }
    assert!(
        !dc_source.is_null()
            && !dc_colormap.is_null()
            && !dc_translation.is_null()
            && !dest_start.is_null(),
        "Ponteiro nulo em setun_DrawTranslatedColumn"
    );

    let frac16_init = dc_texturemid.wrapping_add((dc_yl - centery).wrapping_mul(dc_iscale));
    let mut t_frac = crate::fixed::TernaryFixed::from_doom_fixed16(frac16_init).raw_word();
    let t_step = crate::fixed::TernaryFixed::from_doom_fixed16(dc_iscale).raw_word();

    let mut b_frac16 = frac16_init;
    let b_step16 = dc_iscale;

    let mut machine = global_machine()
        .lock()
        .expect("Mutex corrompido em setun_DrawTranslatedColumn");

    let mut dest_ptr = dest_start;
    let mut y = dc_yl as usize;
    let x = dc_x as usize;

    for _ in 0..=count {
        let int_row = t_frac.shift_trits(-10).as_i64();
        let tex_row = (int_row.rem_euclid(128)) as usize;
        let raw_texel = *dc_source.add(tex_row) as usize;
        let trans_texel = *dc_translation.add(raw_texel) as usize;
        let lit_color = *dc_colormap.add(trans_texel);

        *dest_ptr = lit_color;
        let fb_offset = y * crate::memory::FB_WIDTH + x;
        machine.memory.raw_cells_mut()[crate::memory::FRAMEBUFFER_BASE + fb_offset] =
            Tryte6::from_palette_index(lit_color).as_i64();

        let (next_t_frac, _cout, carries) =
            t_frac.gate_ripple_carry_add(t_step, crate::trit::Trit::Zero);
        let next_b_frac16 = b_frac16.wrapping_add(b_step16);

        machine.cpu.telemetry.adder_carry_propagations += carries as u64;
        machine
            .cpu
            .telemetry
            .record_fixed16_transition(t_frac, next_t_frac, b_frac16, next_b_frac16);
        machine.cpu.cycles += 1;

        t_frac = next_t_frac;
        b_frac16 = next_b_frac16;
        dest_ptr = dest_ptr.add(screenwidth as usize);
        y += 1;
    }
    machine.cpu.acc = t_frac;
}

/// Executa o rasterizador de invisibilidade/espectros `R_DrawFuzzColumn` do `r_draw.c`
/// usando o somador Ripple-Carry de 21 portas `Trit::full_add` para avançar `frac` a cada pixel.
///
/// # Safety
/// Ponteiros devem ser válidos.
#[no_mangle]
pub unsafe extern "C" fn setun_DrawFuzzColumn(
    dc_x: i32,
    dc_yl: i32,
    dc_yh: i32,
    centery: i32,
    dc_iscale: i32,
    dc_texturemid: i32,
    colormaps: *const u8,
    fuzzoffset: *const i32,
    fuzzpos_ptr: *mut i32,
    fuzztable_len: i32,
    dest_start: *mut u8,
    screenwidth: i32,
) {
    let count = dc_yh - dc_yl;
    if count < 0 {
        return;
    }
    assert!(
        !colormaps.is_null()
            && !fuzzoffset.is_null()
            && !fuzzpos_ptr.is_null()
            && !dest_start.is_null(),
        "Ponteiro nulo em setun_DrawFuzzColumn"
    );

    let frac16_init = dc_texturemid.wrapping_add((dc_yl - centery).wrapping_mul(dc_iscale));
    let mut t_frac = crate::fixed::TernaryFixed::from_doom_fixed16(frac16_init).raw_word();
    let t_step = crate::fixed::TernaryFixed::from_doom_fixed16(dc_iscale).raw_word();

    let mut b_frac16 = frac16_init;
    let b_step16 = dc_iscale;

    let mut machine = global_machine()
        .lock()
        .expect("Mutex corrompido em setun_DrawFuzzColumn");

    let mut dest_ptr = dest_start;
    let mut y = dc_yl as usize;
    let x = dc_x as usize;
    let mut fuzzpos = *fuzzpos_ptr;

    for _ in 0..=count {
        let f_off = *fuzzoffset.add(fuzzpos as usize);
        let sampled_pixel = *dest_ptr.offset(f_off as isize);
        let lit_color = *colormaps.add(6 * 256 + (sampled_pixel as usize));

        *dest_ptr = lit_color;
        let fb_offset = y * crate::memory::FB_WIDTH + x;
        machine.memory.raw_cells_mut()[crate::memory::FRAMEBUFFER_BASE + fb_offset] =
            Tryte6::from_palette_index(lit_color).as_i64();

        fuzzpos += 1;
        if fuzzpos >= fuzztable_len {
            fuzzpos = 0;
        }

        let (next_t_frac, _cout, carries) =
            t_frac.gate_ripple_carry_add(t_step, crate::trit::Trit::Zero);
        let next_b_frac16 = b_frac16.wrapping_add(b_step16);

        machine.cpu.telemetry.adder_carry_propagations += carries as u64;
        machine
            .cpu
            .telemetry
            .record_fixed16_transition(t_frac, next_t_frac, b_frac16, next_b_frac16);
        machine.cpu.cycles += 1;

        t_frac = next_t_frac;
        b_frac16 = next_b_frac16;
        dest_ptr = dest_ptr.add(screenwidth as usize);
        y += 1;
    }
    *fuzzpos_ptr = fuzzpos;
    machine.cpu.acc = t_frac;
}

/// Executa o interpolador DDA de planos horizontais (pisos e tetos) `R_DrawSpan` do `r_draw.c`
/// usando acumuladores `TernaryFixed` (`11.10` trits) e o somador Ripple-Carry de 21 estágios
/// (`Word21::gate_ripple_carry_add`) para `(u, v)` a cada pixel horizontal.
///
/// # Safety
/// `dest_start`, `ds_source` e `ds_colormap` devem ser ponteiros válidos.
#[no_mangle]
pub unsafe extern "C" fn setun_DrawSpan(
    ds_y: i32,
    ds_x1: i32,
    ds_x2: i32,
    ds_xfrac: i32,
    ds_yfrac: i32,
    ds_xstep: i32,
    ds_ystep: i32,
    ds_source: *const u8,
    ds_colormap: *const u8,
    dest_start: *mut u8,
) {
    let count = ds_x2 - ds_x1;
    if count < 0 {
        return;
    }
    assert!(
        !ds_source.is_null() && !ds_colormap.is_null() && !dest_start.is_null(),
        "Ponteiro nulo em setun_DrawSpan"
    );
    assert!(
        ds_x1 >= 0
            && (ds_x2 as usize) < crate::memory::FB_WIDTH
            && ds_y >= 0
            && (ds_y as usize) < crate::memory::FB_HEIGHT,
        "Coordenadas fora de faixa em setun_DrawSpan: y={}, x1={}, x2={}",
        ds_y,
        ds_x1,
        ds_x2
    );

    let mut tx_frac = crate::fixed::TernaryFixed::from_doom_fixed16(ds_xfrac).raw_word();
    let mut ty_frac = crate::fixed::TernaryFixed::from_doom_fixed16(ds_yfrac).raw_word();
    let tx_step = crate::fixed::TernaryFixed::from_doom_fixed16(ds_xstep).raw_word();
    let ty_step = crate::fixed::TernaryFixed::from_doom_fixed16(ds_ystep).raw_word();

    let mut bx_frac16 = ds_xfrac;
    let mut by_frac16 = ds_yfrac;

    let mut machine = global_machine()
        .lock()
        .expect("Mutex corrompido em setun_DrawSpan");

    let mut dest_ptr = dest_start;
    let y = ds_y as usize;
    let mut x = ds_x1 as usize;

    for _ in 0..=count {
        // Extrai as coordenadas inteiras de textura (u, v) deslocando 10 trits fisicamente nos fios
        let u_int = (tx_frac.shift_trits(-10).as_i64().rem_euclid(64)) as usize;
        let v_int = (ty_frac.shift_trits(-10).as_i64().rem_euclid(64)) as usize;
        let spot = (v_int * 64) + u_int;
        let raw_texel = *ds_source.add(spot) as usize;
        let lit_color = *ds_colormap.add(raw_texel);

        *dest_ptr = lit_color;
        let fb_offset = y * crate::memory::FB_WIDTH + x;
        machine.memory.raw_cells_mut()[crate::memory::FRAMEBUFFER_BASE + fb_offset] =
            Tryte6::from_palette_index(lit_color).as_i64();

        let (next_tx, _c1, carries_x) =
            tx_frac.gate_ripple_carry_add(tx_step, crate::trit::Trit::Zero);
        let (next_ty, _c2, carries_y) =
            ty_frac.gate_ripple_carry_add(ty_step, crate::trit::Trit::Zero);
        let next_bx16 = bx_frac16.wrapping_add(ds_xstep);
        let next_by16 = by_frac16.wrapping_add(ds_ystep);

        machine.cpu.telemetry.adder_carry_propagations += (carries_x + carries_y) as u64;
        machine
            .cpu
            .telemetry
            .record_fixed16_transition(tx_frac, next_tx, bx_frac16, next_bx16);
        machine
            .cpu
            .telemetry
            .record_fixed16_transition(ty_frac, next_ty, by_frac16, next_by16);
        machine.cpu.cycles += 2;

        tx_frac = next_tx;
        ty_frac = next_ty;
        bx_frac16 = next_bx16;
        by_frac16 = next_by16;
        dest_ptr = dest_ptr.add(1);
        x += 1;
    }
    machine.cpu.acc = tx_frac;
}

/// Sincroniza a paleta `PLAYPAL` de 256 cores RGB vinda de `I_SetPalette` do `doomgeneric`
/// com a região `PALETTE_BASE` da memória ternária Setun-Doom.
///
/// # Safety
/// `rgb_palette` deve apontar para 768 bytes (`256 * 3`).
#[no_mangle]
pub unsafe extern "C" fn setun_SyncPalette(rgb_palette: *const u8) {
    assert!(
        !rgb_palette.is_null(),
        "Ponteiro nulo passado para setun_SyncPalette"
    );
    let slice = core::slice::from_raw_parts(rgb_palette, 768);
    let mut machine = global_machine()
        .lock()
        .expect("Mutex corrompido em setun_SyncPalette");
    machine.memory.load_playpal(slice);
}

/// Transfere os `64.000` bytes do `I_VideoBuffer` (`320x200` índices de paleta do Doom)
/// para os `64.000` `Tryte6` mapeados em `FRAMEBUFFER_BASE` (`4_600_000..4_664_000`) da memória
/// ternária Setun-Doom e renderiza o buffer RGBA32 de saída para exibição gráfica.
///
/// # Safety
/// `video_buffer_320x200` deve apontar para `64.000` bytes e `out_rgba_320x200` para `256.000` bytes.
#[no_mangle]
pub unsafe extern "C" fn setun_BlitVideoBuffer(
    video_buffer_320x200: *const u8,
    out_rgba_320x200: *mut u8,
) {
    assert!(
        !video_buffer_320x200.is_null(),
        "Ponteiro nulo em setun_BlitVideoBuffer"
    );
    let vbuf = core::slice::from_raw_parts(video_buffer_320x200, FB_SIZE);
    let mut machine = global_machine()
        .lock()
        .expect("Mutex corrompido em setun_BlitVideoBuffer");
    for (i, &pal_idx) in vbuf.iter().enumerate() {
        let tryte = Tryte6::from_palette_index(pal_idx);
        machine.memory.raw_cells_mut()[crate::memory::FRAMEBUFFER_BASE + i] = tryte.as_i64();
    }
    let rgba = machine.dg_draw_frame();
    if !out_rgba_320x200.is_null() {
        core::ptr::copy_nonoverlapping(rgba.as_ptr(), out_rgba_320x200, FB_SIZE * 4);
    }
}

/// Imprime no terminal um resumo ao vivo da telemetria ternária acumulada pela CPU Setun-Doom
/// durante a execução do `doomgeneric`.
#[no_mangle]
pub extern "C" fn setun_PrintTelemetry(frame_num: u32) {
    let machine = global_machine()
        .lock()
        .expect("Mutex corrompido em setun_PrintTelemetry");
    let tel = machine.cpu.telemetry;
    println!(
        "[Setun-Doom Frame {:>4}] ISA Instrs: {:>8} | Gate Carries: {:>8} | Trit Flips: {:>9} (Half: {:>9}, Full: {:>7}) vs RV32I Bit Flips: {:>9} ({:+.2}% Flips | {:+.2}% Energy dV^2)",
        frame_num,
        machine.cpu.cycles,
        tel.adder_carry_propagations,
        tel.ternary_trit_flips,
        tel.ternary_half_swings,
        tel.ternary_full_swings,
        tel.binary32_bit_flips,
        -tel.digit_flip_reduction_pct(),
        -tel.weighted_energy_reduction_pct()
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::FRAMEBUFFER_BASE;

    #[test]
    fn test_wad_dma_transcode_and_playpal_extraction() {
        let mut mem = Memory::new();

        // Constrói uma imagem IWAD sintética com 1 lump chamado "PLAYPAL" de 768 bytes
        let mut wad = Vec::new();
        wad.extend_from_slice(b"IWAD");
        wad.extend_from_slice(&(1i32.to_le_bytes())); // 1 lump
        wad.extend_from_slice(&(28i32.to_le_bytes())); // diretório no offset 28 + 768 = 796? Vamos colocar dados em 12..780 e diretório em 780

        // Reajusta offset do diretório para 12 + 768 = 780
        wad.truncate(8);
        wad.extend_from_slice(&(780i32.to_le_bytes()));

        // Dados de PLAYPAL (768 bytes = 256 * 3)
        for i in 0..256 {
            wad.push(i as u8); // R
            wad.push((255 - i) as u8); // G
            wad.push(128u8); // B
        }
        assert_eq!(wad.len(), 780);

        // Entrada do diretório (16 bytes): filepos=12, size=768, name="PLAYPAL\0"
        wad.extend_from_slice(&(12i32.to_le_bytes()));
        wad.extend_from_slice(&(768i32.to_le_bytes()));
        wad.extend_from_slice(b"PLAYPAL\0");

        let (words, lumps) = WadDmaLoader::load_wad_from_slice(
            &mut mem,
            &wad,
            WAD_DMA_BASE,
            DmaTranscodeMode::TrytePerByte,
        )
        .unwrap();

        assert_eq!(words, wad.len());
        assert_eq!(lumps.len(), 1);
        assert_eq!(lumps[0].name, "PLAYPAL");

        // Verifica que os bytes podem ser recuperados perfeitamente da memória ternária
        let recovered = WadDmaLoader::read_bytes_tryte(&mem, WAD_DMA_BASE, wad.len()).unwrap();
        assert_eq!(recovered, wad);
    }

    #[test]
    fn test_doomgeneric_ffi_functions() {
        DG_Init();
        DG_SleepMs(2);
        let ticks = DG_GetTicksMs();
        assert!(ticks >= 1);

        // Escreve no framebuffer da máquina global e testa DG_DrawFrameToBuffer
        {
            let mut machine = global_machine().lock().unwrap();
            machine.memory.set_palette_entry(42, 10, 200, 30);
            let tryte = Tryte6::from_palette_index(42);
            machine
                .memory
                .write_word(FRAMEBUFFER_BASE as i64, Word21::new(tryte.as_i64()))
                .unwrap();
            machine.push_key_event(true, 0xAD); // KEY_UPARROW
        }

        let mut out_rgba = vec![0u8; FB_SIZE * 4];
        unsafe {
            DG_DrawFrameToBuffer(out_rgba.as_mut_ptr());
        }
        assert_eq!(&out_rgba[0..4], &[10, 200, 30, 255]);

        let mut pressed = 0i32;
        let mut key = 0u8;
        let has_key = unsafe { DG_GetKey(&mut pressed, &mut key) };
        assert_eq!(has_key, 1);
        assert_eq!(pressed, 1);
        assert_eq!(key, 0xAD);

        let empty_key = unsafe { DG_GetKey(&mut pressed, &mut key) };
        assert_eq!(empty_key, 0);
    }
}

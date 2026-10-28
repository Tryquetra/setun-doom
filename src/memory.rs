//! Espaço de Memória Linear Ternário ($3^{14} = 4.782.969$ palavras) e Mapeamento de Framebuffer VGA.
//!
//! O espaço de endereçamento de 14 trits balanceados pode ser acessado tanto por:
//! - Endereços lineares não-negativos `0 .. 4_782_969`.
//! - Endereços ternários balanceados simétricos `[-2_391_484 .. +2_391_484]`.
//!
//! Mapa de Memória (`Setun-Doom`):
//! - `0x000000 .. 4_600_000`: RAM Geral (Código, Dados, BSS, Heap e Pilha).
//! - `4_600_000 .. 4_664_000` (`FRAMEBUFFER_BASE .. +64_000`): Framebuffer VGA 320x200,
//!   onde cada palavra armazena um `Tryte6` de 6 trits (`[-364, +364]`) correspondente
//!   ao índice de paleta da cor do pixel.
//! - `4_664_000 .. 4_664_256` (`PALETTE_BASE .. +256`): Paleta de 256 cores RGB empacotadas.
//! - `4_664_256 .. 4_664_512` (`MMIO_BASE`): Registradores de I/O mapeados em memória (Timer, Teclado, DMA).
//! - `4_782_968` (`STACK_TOP`): Topo inicial padrão do Stack Pointer (`SP`).

use crate::word::{Tryte6, Word21, HALF_POW3, POW3};

/// Capacidade total do espaço de endereçamento de 14 trits: $3^{14} = 4.782.969$ palavras.
pub const MEMORY_SIZE: usize = POW3[14] as usize; // 4_782_969

/// Limite positivo de um endereço de 14 trits balanceado (`+2_391_484`).
pub const BALANCED_ADDR_HALF: i64 = HALF_POW3[14]; // 2_391_484

/// Resolução horizontal clássica do Doom (VGA Mode 13h).
pub const FB_WIDTH: usize = 320;

/// Resolução vertical clássica do Doom (VGA Mode 13h).
pub const FB_HEIGHT: usize = 200;

/// Quantidade total de pixels no framebuffer (`320 * 200 = 64.000`).
pub const FB_SIZE: usize = FB_WIDTH * FB_HEIGHT; // 64_000

/// Endereço linear base da região de Framebuffer VGA mapeada em memória.
pub const FRAMEBUFFER_BASE: usize = 4_600_000;

/// Endereço linear final (exclusivo) do Framebuffer VGA.
pub const FRAMEBUFFER_END: usize = FRAMEBUFFER_BASE + FB_SIZE; // 4_664_000

/// Endereço linear base da tabela de paleta VGA (256 entradas).
pub const PALETTE_BASE: usize = FRAMEBUFFER_END; // 4_664_000

/// Endereço linear base dos registradores MMIO (Timer, Teclado, DMA Status).
pub const MMIO_BASE: usize = PALETTE_BASE + 256; // 4_664_256
pub const MMIO_TICKS_MS: usize = MMIO_BASE;
pub const MMIO_KEY_STATE: usize = MMIO_BASE + 1;
pub const MMIO_WAD_SIZE: usize = MMIO_BASE + 2;

/// Endereço linear base padrão para carga de arquivos WAD via DMA.
pub const WAD_DMA_BASE: usize = 500_000;

/// Topo inicial da pilha (`SP`), crescendo em direção aos endereços inferiores.
pub const INITIAL_STACK_POINTER: i64 = 4_599_999;

/// Erros de acesso ao subsistema de memória ternária.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MemoryError {
    /// Endereço fora dos limites de $3^{14}$ posições (`0..4_782_969`).
    OutOfBounds(i64),
    /// Carga DMA excede a capacidade disponível na memória.
    DmaOverflow { base: usize, len: usize },
}

/// Memória linear da arquitetura Setun-Doom alocada em vetor contíguo de `i64` no host.
pub struct Memory {
    /// Vetor de `4.782.969` palavras de 64 bits contendo os valores de `Word21`.
    cells: Vec<i64>,
    /// Paleta RGBA de 256 cores (`[R, G, B, A]`) para tradução ultrarrápida em `DG_DrawFrame`.
    palette_rgba: [[u8; 4]; 256],
}

impl Memory {
    /// Aloca e inicializa a memória principal de $3^{14}$ palavras e uma paleta VGA padrão do Doom.
    pub fn new() -> Self {
        let mut mem = Self {
            cells: vec![0i64; MEMORY_SIZE],
            palette_rgba: [[0, 0, 0, 255]; 256],
        };
        mem.init_default_palette();
        mem
    }

    /// Inicializa uma paleta de 256 cores estilo Doom (tons de fogo, carmesim, metal e cinza)
    /// que pode ser sobrescrita pelo lump `PLAYPAL` do `DOOM.WAD`.
    fn init_default_palette(&mut self) {
        for i in 0..256 {
            let idx = i as u8;
            // Gradiente determinístico rico cobrindo tons escuros, vermelhos e dourados
            let r = idx.wrapping_mul(3).wrapping_add(idx >> 1);
            let g = idx.wrapping_mul(2);
            let b = idx ^ 0x55;
            self.palette_rgba[i] = [r, g, b, 255];
            let packed_rgb = ((r as i64) << 16) | ((g as i64) << 8) | (b as i64);
            self.cells[PALETTE_BASE + i] = Word21::new(packed_rgb).as_i64();
        }
    }

    /// Converte um endereço linear ou balanceado para índice válido `0..MEMORY_SIZE`.
    ///
    /// Suporta endereçamento linear direto `0..4_782_969` (padrão de vetores e código)
    /// e, para valores negativos em `[-2_391_484..-1]`, rebalse modular dentro de $3^{14}$.
    #[inline(always)]
    pub const fn resolve_address(addr: i64) -> Result<usize, MemoryError> {
        if addr >= 0 && (addr as usize) < MEMORY_SIZE {
            Ok(addr as usize)
        } else if addr >= -BALANCED_ADDR_HALF && addr < 0 {
            Ok((addr + (MEMORY_SIZE as i64)) as usize)
        } else {
            Err(MemoryError::OutOfBounds(addr))
        }
    }

    /// Lê uma palavra `Word21` do endereço especificado.
    #[inline(always)]
    pub fn read_word(&self, addr: i64) -> Result<Word21, MemoryError> {
        let idx = Self::resolve_address(addr)?;
        Ok(Word21::new(self.cells[idx]))
    }

    /// Lê uma palavra exigindo endereço válido na faixa de 14 trits (aborta se fora dos limites,
    /// proibindo wrap-around silencioso).
    #[inline(always)]
    pub fn read_word_fast(&self, addr: i64) -> Word21 {
        let idx = Self::resolve_address(addr)
            .unwrap_or_else(|e| panic!("Falha de leitura na memoria ternaria: {:?}", e));
        Word21::new(self.cells[idx])
    }

    /// Escreve uma palavra `Word21` no endereço especificado.
    /// Se o endereço cair na faixa do Framebuffer VGA (`4_600_000..4_664_000`),
    /// o valor é normalizado para um `Tryte6` de 6 trits balanceados (`[-364, +364]`).
    #[inline(always)]
    pub fn write_word(&mut self, addr: i64, val: Word21) -> Result<(), MemoryError> {
        let idx = Self::resolve_address(addr)?;
        if idx >= FRAMEBUFFER_BASE && idx < FRAMEBUFFER_END {
            let tryte = Tryte6::from_i64(val.as_i64());
            self.cells[idx] = tryte.as_i64();
        } else {
            self.cells[idx] = val.as_i64();
        }
        Ok(())
    }

    /// Escrita no caminho crítico da CPU exigindo endereço válido na faixa de 14 trits
    /// (aborta se fora dos limites, proibindo escrita fora de faixa silenciosa).
    #[inline(always)]
    pub fn write_word_fast(&mut self, addr: i64, val: Word21) {
        let idx = Self::resolve_address(addr)
            .unwrap_or_else(|e| panic!("Falha de escrita na memoria ternaria: {:?}", e));
        if idx >= FRAMEBUFFER_BASE && idx < FRAMEBUFFER_END {
            self.cells[idx] = Tryte6::from_i64(val.as_i64()).as_i64();
        } else {
            self.cells[idx] = val.as_i64();
        }
    }

    /// Escreve um pixel na posição `(x, y)` do framebuffer (`320x200`) usando um `Tryte6`.
    #[inline(always)]
    pub fn write_pixel_tryte(
        &mut self,
        x: usize,
        y: usize,
        color: Tryte6,
    ) -> Result<(), MemoryError> {
        if x >= FB_WIDTH || y >= FB_HEIGHT {
            return Err(MemoryError::OutOfBounds((y * FB_WIDTH + x) as i64));
        }
        let offset = y * FB_WIDTH + x;
        self.cells[FRAMEBUFFER_BASE + offset] = color.as_i64();
        Ok(())
    }

    /// Lê o `Tryte6` de 6 trits armazenado na coordenada `(x, y)` do framebuffer.
    #[inline(always)]
    pub fn read_pixel_tryte(&self, x: usize, y: usize) -> Result<Tryte6, MemoryError> {
        if x >= FB_WIDTH || y >= FB_HEIGHT {
            return Err(MemoryError::OutOfBounds((y * FB_WIDTH + x) as i64));
        }
        let offset = y * FB_WIDTH + x;
        Ok(Tryte6::from_i64(self.cells[FRAMEBUFFER_BASE + offset]))
    }

    /// Define uma cor na paleta VGA (`0..=255`) e sincroniza a região mapeada `PALETTE_BASE`.
    #[inline]
    pub fn set_palette_entry(&mut self, index: u8, r: u8, g: u8, b: u8) {
        let i = index as usize;
        self.palette_rgba[i] = [r, g, b, 255];
        let packed = ((r as i64) << 16) | ((g as i64) << 8) | (b as i64);
        self.cells[PALETTE_BASE + i] = Word21::new(packed).as_i64();
    }

    /// Carrega uma paleta `PLAYPAL` de 768 bytes (`256 * 3` bytes RGB) do Doom.
    pub fn load_playpal(&mut self, rgb_bytes: &[u8]) {
        let count = (rgb_bytes.len() / 3).min(256);
        for i in 0..count {
            let r = rgb_bytes[i * 3];
            let g = rgb_bytes[i * 3 + 1];
            let b = rgb_bytes[i * 3 + 2];
            self.set_palette_entry(i as u8, r, g, b);
        }
    }

    /// Retorna o slice bruto das `64.000` palavras do framebuffer ternário.
    #[inline(always)]
    pub fn framebuffer_slice(&self) -> &[i64] {
        &self.cells[FRAMEBUFFER_BASE..FRAMEBUFFER_END]
    }

    /// Renderiza as `64.000` posições do framebuffer ternário (`Tryte6`) em um buffer RGBA de
    /// `320 * 200 * 4 = 256.000` bytes (`u8`) ou `64.000` pixels `u32` (`0xAABBGGRR` / `0xFFRRGGBB`).
    pub fn render_framebuffer_rgba(&self, out_rgba: &mut [u8]) {
        assert!(out_rgba.len() >= FB_SIZE * 4);
        let fb = self.framebuffer_slice();
        for (i, &cell) in fb.iter().enumerate() {
            let tryte = Tryte6::from_i64(cell);
            let pal_idx = tryte.to_palette_index() as usize;
            let rgba = self.palette_rgba[pal_idx];
            let base = i * 4;
            out_rgba[base] = rgba[0];
            out_rgba[base + 1] = rgba[1];
            out_rgba[base + 2] = rgba[2];
            out_rgba[base + 3] = rgba[3];
        }
    }

    /// Acesso direto ao slice completo de células de memória.
    #[inline(always)]
    pub fn raw_cells(&self) -> &[i64] {
        &self.cells
    }

    /// Acesso mutável direto ao slice completo de células de memória.
    #[inline(always)]
    pub fn raw_cells_mut(&mut self) -> &mut [i64] {
        &mut self.cells
    }
}

impl Default for Memory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_capacity_and_addressing() {
        let mut mem = Memory::new();
        assert_eq!(mem.raw_cells().len(), 4_782_969);

        let w = Word21::from_i64(-987_654_321);
        mem.write_word(123_456, w).unwrap();
        assert_eq!(mem.read_word(123_456).unwrap(), w);
    }

    #[test]
    fn test_framebuffer_tryte6_mapping_and_rgba_translation() {
        let mut mem = Memory::new();
        mem.set_palette_entry(200, 255, 64, 32);

        // Escreve cor de paleta 200 como Tryte6 centrado (200 - 128 = +72) no pixel (10, 5)
        let color_tryte = Tryte6::from_palette_index(200);
        assert!(color_tryte.as_i16() >= -364 && color_tryte.as_i16() <= 364);

        let linear_addr = (FRAMEBUFFER_BASE + 5 * FB_WIDTH + 10) as i64;
        mem.write_word(linear_addr, Word21::from_i64(color_tryte.as_i64()))
            .unwrap();

        let read_back = mem.read_pixel_tryte(10, 5).unwrap();
        assert_eq!(read_back, color_tryte);
        assert_eq!(read_back.to_palette_index(), 200);

        // Traduz para RGBA e verifica o pixel (10, 5)
        let mut rgba = vec![0u8; FB_SIZE * 4];
        mem.render_framebuffer_rgba(&mut rgba);
        let px_offset = (5 * FB_WIDTH + 10) * 4;
        assert_eq!(&rgba[px_offset..px_offset + 4], &[255, 64, 32, 255]);
    }
}

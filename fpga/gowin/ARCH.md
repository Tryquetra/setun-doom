# Arquitetura do core REBEL-6 sintetizável (decisão)

Convergência dos blocos verificados para o objetivo "Doom na FPGA com a ISA
REBEL-6 e vídeo".

## Representação

- Registradores: 729 × 64 bits assinados na faixa balanceada
  `[-HALF, HALF]`, espelhando `Word24::raw()` do `word.rs`. O banco é a
  BRAM dual `rebel6_regfile.v` (8 `DPX9B`, 141 DFF), com 2 leituras + 1
  escrita por ciclo.
- Instruções: palavras de 32 trits empacotadas em 64 bits (2 bits por trit),
  decodificadas por `rebel6_decoder.v` (verificado 73/73).
- ALU: `rebel6_alu.v` sobre valores i64 (verificada 6.000/6.000), com wrap
  módulo 3^24 (ternária) e módulo 2^32 (binária).

## Pipeline multi-ciclo (proposto)

1. **Fetch**: `pc` → `imem` (leitura registrada). Grava a instrução.
2. **Decode/Read**: `rebel6_decoder` + leitura dos dois operandos no banco
   BRAM (latência 1 ciclo, dados chegam no ciclo seguinte).
3. **Execute**: ALU combinacional sobre os operandos lidos.
4. **Writeback/Memória**: escrita do resultado no banco (1 porta de escrita);
   para `lw/sw`, um estágio extra acessa `dmem` (também BRAM).

O banco dual permite ler os dois `rs` no mesmo ciclo e escrever `rd` sem
colisão; o risco é a latência registrada da leitura, resolvida com um
estágio de "bypass/forwarding" ou com pipeline de 3–4 estágios.

## Restrições de área

- Banco 729×64 dual: 8 `DPX9B` (dos 46 disponíveis), ~141 DFF.
- ALU + decoder somados ~3–4k LUT.
- `dmem` e `imem` precisam ser BRAM; a imagem Doom (~604 KB) não cabe em
  BRAM, exigindo SDRAM (8 MiB) com controlador + cache (H12).

## Pendente

- FSM/pipeline com banco BRAM (ciclos por instrução e forwarding).
- Equivalência por simulação de um programa curto vs `cpu.rs`.
- `dmem`/`imem` em BRAM e depois SDRAM.
- Framebuffer + PHY TMDS/HDMI.
- Síntese integrada e carga na Tang Nano 20K.

# Arquitetura do SoC REBEL-6 Sintetizável (Tang Nano 20K)

Sistema integrado em chip (SoC, System-on-Chip, sistema completo integrado em um único dispositivo) para execução nativa de Doom na placa Sipeed Tang Nano 20K (`GW2AR-LV18QN88C8/I7`) sobre a ISA (Instruction Set Architecture, arquitetura do conjunto de instruções do processador) REBEL-6 com saída de vídeo digital HDMI (High-Definition Multimedia Interface, interface digital de vídeo e áudio).

## 1. Núcleo REBEL-6 (`rebel6_core.v`)

- **Decodificador fatorado (`rebel6_decoder.v`)**: extrai cinco grupos compartilhados de 6 trits (`[-364, +364]`) e compõe imediatos de 12 e 24 trits por somas deslocadas (`* 729` e `* 531441`), cobrindo 73/73 instruções nos 7 formatos (`R, I, B, G, X, D, Y`) e expondo detecção de trit ilegal `11`.
- **Banco de registradores (`rebel6_regfile.v`)**: 729 registradores de 40 bits assinados em faixa balanceada `[-141_214_768_240, +141_214_768_240]`, mapeados em 6 blocos `DPX9B` de B-SRAM (Block Static Random-Access Memory, memória estática síncrona em blocos dedicada) com 2 portas de leitura e 1 de escrita por ciclo (`X0` fixo em zero).
- **ALU (`rebel6_alu.v`)**: ALU (Arithmetic Logic Unit, unidade lógica e aritmética do processador) combinacional de 40 bits implementando aritmética ternária com wrap módulo $3^{24}$ e camada binária RV32I (Reduced Instruction Set Computer Five 32-bit Integer, conjunto base de instruções inteiras de 32 bits) com wrap módulo $2^{32}$.
- **Unidade trit a trit (`rebel6_trit_unit.v`)**: motor sequencial sem divisores para `or.t, xor.t, and.t, ori.t, xori.t, andi.t, sh.t, shi.t, rotr.t, rot.t, majv.t, minv.t`.
- **Acesso desalinhado transparente**: estágios `S_MEM1` e `S_MEM2` recompõem leituras e escritas de 16 e 32 bits que cruzam fronteiras de palavra de 4 bytes.

## 2. Hierarquia de Memória e Armazenamento (`8 MiB SDRAM` + `8 MiB SPI Flash`)

- **Controlador SDRAM (`rebel6_sdram32.v`)**: opera sobre a SDRAM (Synchronous Dynamic Random-Access Memory, memória dinâmica síncrona integrada no encapsulamento) de 64 Mbit (`2M x 32` bits = 8 MiB) com inicialização automática de 200 us, auto-refresh a cada 14 us e escrita mascarada por byte (`O_sdram_dqm`).
  - `0x000000 .. 0x67FFFF` (`6,50 MiB`): janela de dados `dmem` do guest Doom (`.data`, `.rodata`, `.bss`, heap de 4 MiB e pilha até `6.500.000`).
  - `0x680000 .. 0x71FFFF` (`640 KiB`): janela Harvard de instruções `imem` (`75.515 x 64` bits).
- **Caches em B-SRAM**:
  - `rebel6_icache.v`: 1.024 linhas de 64 bits (8 KiB, 4 blocos `DPX9B`), mapeamento direto.
  - `rebel6_dcache.v`: 512 linhas de 32 bits (2 KiB, 2 blocos `DPX9B`), escrita write-through com invalidação por linha durante transferências de DMA (Direct Memory Access, acesso direto à memória sem intervenção da unidade central).
- **Motor autônomo de boot e syscalls (`rebel6_soc_host.v` + `rebel6_periph.v`)**:
  - Acorda a memória Flash SPI (Serial Peripheral Interface, barramento serial síncrono) `XT25F64B` com `0xAB`, carrega a imagem compacta `R6BT` gravada em `0x100000` (`749,8 KiB`) para a SDRAM e inicia o core.
  - Executa em hardware os 11 serviços `ecall` (`SYS_EXIT`, `SYS_WRITE` por UART, Universal Asynchronous Receiver-Transmitter, interface serial assíncrona, a `3.000.000` 8N1 com pacing `264` ciclos/byte na ponte BL616, `SYS_TICKS_MS`, `SYS_BLIT`, `SYS_GETKEY`, `SYS_WAD_OPEN/SEEK/READ/SIZE` lendo `DOOM1.WAD` direto de `0x200000` na Flash SPI, `SYS_SLEEP_MS` e `SYS_PALETTE`).
  - Um streamer de vídeo em segundo plano transmite cada quadro ao Linux: `FE ED 52 36 | frame# | paleta 768 | 64000 pixels crus | checksum XOR | \r\n[DOOM FRAME]\r\n | contagem de pixels`. Bytes de console são enfileirados (FIFO 255) e drenados quando o stream está ocioso; `SYS_BLIT`/`SYS_PALETTE` aguardam o fim do stream para não rasgar quadro. Teclado do Linux chega como pacotes binários `0xFD/0xFC + keycode` com FIFO de 16 e adiamento de keydown por tic de jogo.
## 3. Pipeline de Vídeo HDMI (`rebel6_hdmi.v`)

- **Relógios**: `rPLL` gera `126,0 MHz` a partir do cristal de `27,0 MHz` (pino 4) e `CLKDIV` divide por 5 para obter `25,2 MHz` (`640x480p @ 60,00 Hz`).
- **Memórias dedicadas de vídeo**: framebuffer dual-port de `64 KiB` (`320 x 200 x 8` bits em 32 blocos `DPB`) e paleta dual-port `256 x 24` bits, isolando a varredura de vídeo do tráfego de SDRAM.
- **Escalonamento 4:3 e serialização**: duplicação horizontal exata (`320 -> 640`) e cadência inteira `12:5` vertical (`200 -> 480`), seguidas de 3 codificadores TMDS (Transition-Minimized Differential Signaling, sinalização diferencial com transições minimizadas) 8b/10b, 4 primitivos `OSER10` e 4 buffers `TLVDS_OBUF` nos pinos `33..40`.

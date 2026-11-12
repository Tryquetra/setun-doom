# Progresso: Doom na FPGA (REBEL-6 e Vídeo HDMI)

Objetivo: executar o Doom na placa Sipeed Tang Nano 20K (`GW2AR-LV18QN88C8/I7`, família `GW2A-18C`) sobre a ISA (Instruction Set Architecture, arquitetura do conjunto de instruções do processador) REBEL-6 com vídeo na saída HDMI (High-Definition Multimedia Interface, interface digital de vídeo e áudio).

## Blocos Verificados

| Bloco | Arquivo | Verificação |
| :-- | :-- | :-- |
| Decoder fatorado (73 ops, 7 formatos) | `fpga/gowin/rebel6_decoder.v` | 73/73 instruções e registradores `0..728` idênticos a `encoding.rs` |
| ALU (Arithmetic Logic Unit, unidade lógica e aritmética) de 40 bits | `fpga/gowin/rebel6_alu.v` | Wrap ternário módulo $3^{24}$ e camada binária módulo $2^{32}$ |
| Unidade trit a trit sem divisores | `fpga/gowin/rebel6_trit_unit.v` | 12 operações (`or.t`, `xor.t`, `and.t`, shifts, rotações, maioria/minoria) |
| Banco dual 729x40b (`6 DPX9B`) | `fpga/gowin/rebel6_regfile.v` | 2 leituras + 1 escrita por ciclo em B-SRAM (Block Static Random-Access Memory, memória síncrona em blocos dedicada), `X0` fixo em zero |
| Core REBEL-6 completo | `fpga/gowin/rebel6_core.v` | 64/64 programas aleatórios full-ISA, 37/37 testes oficiais RV32I (Reduced Instruction Set Computer Five 32-bit Integer, base inteira de 32 bits) e paridade SHA-256 nos frames do Doom |
| Controlador SDRAM (Synchronous Dynamic Random-Access Memory, memória dinâmica síncrona) 32 bits | `fpga/gowin/rebel6_sdram32.v` | 8 MiB (`4 x 2048 x 256 x 32b`), auto-refresh a cada 14 us e escrita por byte |
| Leitor Flash SPI (Serial Peripheral Interface, barramento serial síncrono) e UART (Universal Asynchronous Receiver-Transmitter, interface serial assíncrona) | `fpga/gowin/rebel6_periph.v` | Wake-up `0xAB`, leitura contínua `0x03` (`doom_boot.bin` em `0x100000` e `DOOM1.WAD` em `0x200000`), console `3.000.000` 8N1 com pacing `264` ciclos/byte (`79,5 kB/s`, abaixo do teto sem perda da ponte BL616) |
| Framebuffer de leitura `clk_sys` (shared Port A) | `fpga/gowin/rebel6_hdmi.v` | Leitura do host a `64 KiB` sem novos blocos B-SRAM (32 DPB preservados) |
| Streamer de vídeo UART (fundo) | `fpga/gowin/rebel6_soc_host.v` | Quadro `FE ED 52 36 | frame# | paleta 768 | 64000 pixels crus | checksum XOR | \r\n[DOOM FRAME]\r\n | contagem de pixels`, iniciado a cada `SYS_BLIT`; CPU não trava (`SYS_BLIT`/`SYS_PALETTE` aguardam fim do stream); teclado binário `FD/FC + keycode` com FIFO 16 e adiamento de keydown por tic |


## Medições em Silício (`GW2AR-LV18QN88C8/I7`)

| Métrica | Valor Observado |
| :-- | --: |
| Recursos lógicos sintetizados (`LUT1..LUT4`) | `7.860 / 20.736` (`37,9%`) |
| Registradores sequenciais (`DFF*`) | `3.359 / 15.552` (`21,6%`) |
| Blocos B-SRAM (`DPB + DPX9B + SDPX9B`) | `45 / 46` (`97,8%`) |
| Multiplicadores `MULT18X18` | `1 / 48` (`2,1%`) |
| Frequência máxima pós-roteamento `clk_sys` (`21,00 MHz` operado) | `31,73 MHz` |
| Frequência máxima pós-roteamento `clk_pix` (`25,20 MHz` operado) | `55,26 MHz` |
| Boot frio (Flash SPI -> SDRAM -> 8 frames E1M1 renderizados) | `24,42 s` |
| Regime contínuo E1M1 na placa (`6,67` ciclos SoC/instrução) | `1,362 s/frame` (`0,73 FPS`, `~3,13 MIPS`) |
## Build e Execução Física

```bash
# 1. Empacotar imagem R6BT, sintetizar, rotear e gerar bitstream .fs
./scripts/build_fpga_doom.sh

# 2. Gravar imagem de boot (0x100000), DOOM1.WAD (0x200000), carregar bitstream e monitorar frames
python3 scripts/flash_and_monitor_fpga.py --flash-bitstream --target-frames 6

# 3. Verificar o stream ao vivo (decodifica quadros, checksum e contagem de pixels)
python3 scripts/fpga_stream_check.py /dev/ttyUSB0

# 4. Janela ao vivo no Linux (SDL2; teclado envia eventos binários para a FPGA)
cargo run --release --bin rebel6_fpga_view -- --port /dev/ttyUSB0
```

### Verificação da nova cadeia de stream

- Validação em simulação Verilator ponta a ponta: **31 quadros consecutivos** decodificados do stream da UART (`sim_soc`), todos com checksum XOR correto, contagem de pixels `64000` e PPMs de referência salvos (`fpga/build/soc/soc_uart_frame_*.ppm`).
- Verificação física: a ponte Sipeed BL616 encapula cada byte TX com `0xFA`; o cliente e o checker removem esse framing (flag `--raw-fa` desativa). A ponte pode requerer replug após sobrecarga de RX contínuo.
- `SYS_BLIT`/`SYS_PALETTE` aguardam `v_stream_active` cair para não rasgar quadro; `SYS_WRITE` usa FIFO de console de 255 bytes drenado quando o stream está ocioso.

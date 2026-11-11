# Progresso — Doom na FPGA (REBEL-6 + vídeo)

Objetivo ativo: executar o Doom na Tang Nano 20K (GW2A-18C) com a ISA
REBEL-6 e saída de vídeo.

## Estado verificado (commits na branch `feat/level-4-fpga-synthesis`)

| Bloco | Arquivo | Verificação | Commit |
| :-- | :-- | :-- | :-- |
| Decoder REBEL-6 | `fpga/gowin/rebel6_decoder.v` | 73/73 instruções vs `decode()` do Rust | `08e2f26` |
| ALU (ternária + binária) | `fpga/gowin/rebel6_alu.v` | 6.000/6.000 pares vs `word.rs`/`cpu.rs` | `05e45c5` |
| Banco 729 regs 48b (BRAM dual) | `fpga/gowin/trit_regfile.v` | 3–6 `DPX9B` | `079fe61` |
| Banco 729 regs 64b (BRAM dual) | `fpga/gowin/rebel6_regfile.v` | 8 `DPX9B`, 141 DFF | pendente commit |
| Área de operadores ternários | `fpga/gowin/trit_*.v` | hipóteses H1–H7 | `c66612c`…`d322ad1` |

## Fluxo do build

```bash
yosys -q -p "read_verilog fpga/gowin/rebel6_defs.v fpga/gowin/rebel6_decoder.v; \
  synth_gowin -top rebel6_decoder -json fpga/build/core/decoder.json"
```

## Verificação RTL vs software

- Ouro Rust gerado por `examples/dump_encode.rs` (73 instruções) e
  `examples/dump_alu.rs` (6.000 pares). Comparado via `yosys eval` sobre o
  `-tb` (cabos de depuração concatenados). Resultado: 0 divergências.

## Pendente

1. FSM fetch/decode/execute integrando decoder + banco + ALU + memória Harvard.
2. Equivalência por simulação de um programa curto vs `cpu.rs`.
3. Memória: framebuffer em BRAM + controlador SDRAM (imagem Doom ~604 KB).
4. Vídeo: PHY TMDS/HDMI (ou RGB LCD) a partir do framebuffer.
5. Síntese integrada + carga física na placa.

## Notas

- O banco 729×48 usa 2 bits por trit (código canônico, `11` reservado para erro).
- A ALU representa `Word24` como valor assinado de 64 bits na faixa balanceada,
  exatamente como o `word.rs`; a camada binária trunca a 32 bits.
- Divergências de área/energia do FPGA exigem medição (H9) e estão fora do
  escopo verificado até aqui.

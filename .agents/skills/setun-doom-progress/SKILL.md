---
name: setun-doom-progress
description: "Retomar o desenvolvimento do Setun-Doom em /home/robson/coding/setun-doom: emulador ternário balanceado de 21 trits que executa Doom, com níveis 0 (FFI no engine C), 1 (DBT RV32IM com guest bare-metal jogável), 2 (codegen/LLVM IR) e 3 (Setun-DOS e unikernel RISC-V). Use quando o pedido envolver setun-doom, DBT, RV32IM, Doom ternário, modo veracidade, modo rápido, RPN Setun-70, ou os binários doomgeneric_setun, setun-doom-rv32 e setun_dos. Triggers: setun-doom, setun doom, DBT ternário, RV32IM no Setun, doom ternário, modo veracidade, modo rápido, setun-doom-rv32, doom_rv32.elf, stack_machine, Setun-70 RPN."
triggers:
  - setun-doom
  - setun doom
  - DBT ternário
  - RV32IM no Setun
  - doom ternário
  - modo veracidade
  - modo rápido
  - setun-doom-rv32
  - doom_rv32.elf
  - stack_machine
  - Setun-70 RPN
---

# Setun-Doom — Skill de Continuidade

> Progresso salvo em **2026-10-27**, HEAD `d0d3627` na `main` (sincronizada com `origin`).
> Repositório: `/home/robson/coding/setun-doom`
> 74 testes, `cargo clippy --all-targets` limpo, unikernel RISC-V compilando.

---

## 1. Estado atual

| Camada | Estado | Artefato |
| :-- | :-- | :-- |
| Nível 0 | Doom em C com hooks ternários por FFI | `libsetun_ffi.a`, `doomgeneric_setun` |
| Nível 1 | DBT RV32IM com Doom bare-metal jogável | `setun-doom-rv32`, `vendor/doomgeneric/riscv32/doom_rv32.elf` |
| Nível 2 | Parser de IR LLVM, linker e `setun-ld`/`setun-run` | `src/codegen/`, `src/bin/setun_ld.rs` |
| Nível 3 | Setun-DOS, desktop e unikernel RISC-V | `setun_dos`, `crates/setun-kernel` |
| Setun-70 | Máquina de duas pilhas com assembler e demo | `src/stack_machine.rs` |

Dois modos de execução no DBT, com padrão veracidade na biblioteca:

- `SetunDbtEngine<true>` (`SetunDbtEngine::from_elf_bytes`): cada operação passa pelas portas ternárias, telemetria completa, lento.
- `SetunDbtEngine<false>` (`from_elf_bytes_fast` / `from_elf_bytes_fast_with_stack`): aritmética direta no host, sem telemetria, rápido.

O driver `setun-doom-rv32` usa o modo rápido por padrão e aceita `SETUN_RV32_VERACITY=1` para voltar às portas.

---

## 2. Comandos

```bash
# Verificação padrão
cargo test --release
cargo clippy --all-targets

# Nível 0: engine C nativo com HUD ternário
make -C vendor/doomgeneric/doomgeneric -f Makefile.setun
./vendor/doomgeneric/doomgeneric/doomgeneric_setun -iwad vendor/DOOM1.WAD

# Nível 1: guest RV32IM e driver
make -C vendor/doomgeneric/riscv32
cargo run --release --bin setun-doom-rv32

# Nível 1 em veracidade, headless, parando após N frames
SETUN_RV32_HEADLESS=1 SETUN_RV32_VERACITY=1 SETUN_RV32_MAX_FRAMES=20 \
  cargo run --release --bin setun-doom-rv32

# Benchmark sem os sleeps de tic do Doom (compara tempo, não instruções)
SETUN_RV32_HEADLESS=1 SETUN_RV32_NO_SLEEP=1 SETUN_RV32_MAX_FRAMES=300 \
  cargo run --release --bin setun-doom-rv32

# Unikernel RISC-V
cargo build --target riscv64gc-unknown-none-elf --release -p setun-kernel
./scripts/run_qemu.sh

# Testbench com demo Setun-70 RPN
./target/release/setun-doom --no-gui
```

Variáveis do driver: `SETUN_RV32_VERACITY`, `SETUN_RV32_HEADLESS`, `SETUN_RV32_MAX_FRAMES`, `SETUN_RV32_NO_SLEEP`, `SETUN_RV32_TRACE_WAD`.

---

## 3. Medições de referência

| Cenário | Resultado |
| :-- | --: |
| Nível 0, 1500 frames | 52 s, cross-check de hooks exato |
| Nível 1 rápido, headless | ~66 FPS, primeiro frame 0,2 s |
| Nível 1 rápido, janela com vsync | ~55 FPS |
| Nível 1 veracidade | primeiro frame 8,3 s, ~0,6 FPS |
| Veracidade, 20 frames | 193 808 320 trit flips, 204 113 886 bit flips, 61,41% de energia |
| DBT, lookup de bloco | 7,7 ns (HashMap) → 0,33 ns (índice direto) |
| DBT, operação traduzida | 32 → 16 bytes |
| Arena + ops compactas | 52 → 65 FPS headless |
| libc word-wise no guest | −23% instruções, veracidade 11,0 → 8,3 s |
| Oráculo FixedMul/FixedDiv | 19 391 amostras, erro dentro do limite 3^-10 |
| Oráculo BSP | 5 561 lados, 0 divergências fora do plano |

Perfil do caminho quente medido: `gate_ripple_carry_add` 217 ns, `gate_add_i32` 352 ns, Alvarez binário→ternário 1 139 ns, `write_reg` 72 ns, `count_trit_flips` 89 ns, acesso de byte à memória 1,5 ns. O gargalo do modo veracidade é aritmética de portas; o do modo rápido é acesso de memória por byte.

---

## 4. Invariantes e regras

1. **Veracidade é o padrão.** Qualquer caminho rápido novo precisa de opt-in explícito e teste que prove equivalência arquitetural e telemetria zerada. Ver `tests/integration_execution_modes.rs`.
2. **Sem falhas silenciosas.** Proibido `let _ =` em operação falível. Lints `let_underscore_drop` e `clippy::let_underscore_must_use` ligados em `src/lib.rs` e no kernel.
3. **Contratos `// SAFETY:`** em todo `unsafe`, cobrindo alinhamento, aliasing e premissas FFI.
4. **`setun-doom` permanece `rlib`.** O artefato C fica em `crates/setun-ffi` (`staticlib`/`cdylib`), porque o unikernel precisa compilar a lib para `riscv64gc-unknown-none-elf`.
5. **`m_fixed.c` tem dois caminhos.** Sob `SETUN_RV32_GUEST` usa produto de 64 bits e divisão fracionária de 32 bits; fora dele mantém o código original, que o teste de codegen do nível 2 compila.
6. **Sem em dash, sem estruturas contrastivas retóricas** nos textos e commits.
7. **Doom usa 35 Hz de tic e dorme 1 ms** em `TryRunTics`; por isso o modo rápido estabiliza perto de 66 FPS.

---

## 5. Armadilhas já encontradas

| Sintoma | Causa | Correção |
| :-- | :-- | :-- |
| `Bad R_RenderWallRange: 161 to 159` no guest | `FixedDiv` do C vendorizado estoura 32 bits em `r << 16` | caminho `SETUN_RV32_GUEST` com divisão longa de 32 bits |
| `W_GetNumForName: STCFN33 not found` | `printf` ignorava precisão `%.3d` | formatador com precisão de inteiro |
| ponteiro inválido ao abrir o WAD | `wad_file_t.file_class` nulo | classe `stdc_wad_file` apontada no `W_OpenFile` |
| guest para com `ReturnedFromMain` | `D_DoomMain` retorna após um tick | loop externo chamando `doomgeneric_Tick` |
| link do driver C usa código velho | `staticlib` removido do manifesto | `crates/setun-ffi` e `Makefile.setun` com caminho relativo |

---

## 6. Mapa de arquivos

- `src/dbt.rs`: tradução RV32IM, arena plana de operações, índice direto por PC, modos `VERACITY`.
- `src/mmo.rs`: FFI, contadores de hooks, lock com recuperação de envenenamento, ROMs de fixed-point e BSP.
- `src/stack_machine.rs`: Setun-70 de duas pilhas, assembler textual, testes e benchmark.
- `vendor/doomgeneric/riscv32/`: guest freestanding (headers, libc, soft-float, ABI `ecall`, linker, Makefile).
- `src/bin/setun_doom_rv32.rs`: driver host do nível 1 (SDL, paleta, teclado, relógio, WAD).
- `docs/rebel6.md` e `docs/papers/rebel-6-ismvl-2025.pdf`: referência do REBEL-6, mapeamento para os opcodes de 21 trits e o paper aceito em CC BY 4.0.
- `tests/integration_fixed_oracle.rs`, `tests/integration_bsp_oracle.rs`: oráculos contra o C vendorizado.
- `tests/integration_execution_modes.rs`: equivalência veracidade/rápido e telemetria zerada.
- `crates/setun-ffi/`: artefatos C (`libsetun_ffi.a`, `.so`).

---

## 7. Pendências

1. Caminho de acesso de 32 bits direto no DBT, para cortar o custo de memória por byte no modo rápido (estimativa de 20 a 30%).
2. `#[non_exhaustive]` nos enums públicos que evoluem (`Rv32iInstr`, `DmaTranscodeMode`) e corrigir a deriva do README (36 opcodes citados, 45 reais).
3. Rodar Miri por módulo depois do refactor da arena.
4. Levar o modo RPN do Setun-70 para o Setun-DOS ou para um parser infixa via Shunting-yard no testbench.
5. Decidir o destino de `src/dos/playpal.bin` (untracked, sem uso) e de `o que é aarch64?.md` (rascunho untracked).

---

## 8. Git

- Branch de trabalho: `main`, sincronizada com `origin` (`git@github.com:Tryquetra/setun-doom.git`).
- Últimos commits: `d0d3627` assembler RPN, `ff54549` máquina de duas pilhas, `745d45f` telemetria do driver, `49eecb7` nível 1 jogável e modo rápido, `e50add9` gitignore do guest.
- Antes de commitar: `cargo test --release`, `cargo clippy --all-targets`, e confirmar que o unikernel ainda compila.

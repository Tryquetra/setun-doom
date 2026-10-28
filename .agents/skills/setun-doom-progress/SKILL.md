---
name: setun-doom-progress
description: "Retomar o desenvolvimento do Setun-Doom: emulador ternário balanceado de 21 trits que executa Doom, com níveis 0 (FFI no engine C), 1 (DBT RV32IM com guest bare-metal jogável), 2 (codegen/LLVM IR), 3 (Setun-DOS e unikernel RISC-V), a máquina Setun-70 de duas pilhas, a documentação do REBEL-6 e o core REBEL-6 puro de 32 trits na branch feat/rebel6-pure. Use quando o pedido envolver setun-doom, DBT, RV32IM, Doom ternário, modo veracidade, modo rápido, RPN Setun-70, REBEL-6, core rebel6, R2R, carry com sinal, SIMD ternário ou os binários doomgeneric_setun, setun-doom-rv32, setun_dos, rebel6_doom, rebel6_run e rebel6_asm. Triggers: setun-doom, setun doom, DBT ternário, RV32IM no Setun, doom ternário, modo veracidade, modo rápido, setun-doom-rv32, doom_rv32.elf, stack_machine, Setun-70 RPN, REBEL-6, REBEL-2, paper REBEL, docs/rebel6, feat/rebel6-pure, rebel6, R2R, carry com sinal, SIMD ternário."
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
  - REBEL-6
  - REBEL-2
  - paper REBEL
  - docs/rebel6
  - feat/rebel6-pure
  - rebel6
  - R2R
  - carry com sinal
  - SIMD ternário
---

# Setun-Doom — Skill de Continuidade

> Progresso salvo em **2026-10-28**.
> Repositório principal: `/home/robson/coding/setun-doom`
> Worktree do core REBEL-6 puro: `/home/robson/coding/setun-doom-rebel6`
> Branch de trabalho atual: `feat/rebel6-pure`, último commit `d4d3e59`.
> 109 testes na branch, `cargo clippy --all-targets` limpo, unikernel RISC-V compilando.

---

## 1. Estado atual

| Camada | Estado | Artefato |
| :-- | :-- | :-- |
| Nível 0 | Doom em C com hooks ternários por FFI | `libsetun_ffi.a`, `doomgeneric_setun` |
| Nível 1 | DBT RV32IM com Doom bare-metal jogável | `setun-doom-rv32`, `vendor/doomgeneric/riscv32/doom_rv32.elf` |
| Nível 1.5 | Core REBEL-6 puro de 32 trits executando Doom via R2R | `rebel6_doom`, `rebel6_run`, `rebel6_asm`, `src/rebel6/` |
| Nível 2 | Parser de IR LLVM, linker e `setun-ld`/`setun-run` | `src/codegen/`, `src/bin/setun_ld.rs` |
| Nível 3 | Setun-DOS, desktop e unikernel RISC-V | `setun_dos`, `crates/setun-kernel` |
| Setun-70 | Máquina de duas pilhas com assembler e demo | `src/stack_machine.rs` |

Dois modos de execução no DBT, com padrão veracidade na biblioteca:

- `SetunDbtEngine<true>` (`SetunDbtEngine::from_elf_bytes`): cada operação passa pelas portas ternárias, telemetria completa, lento.
- `SetunDbtEngine<false>` (`from_elf_bytes_fast` / `from_elf_bytes_fast_with_stack`): aritmética direta no host, sem telemetria, rápido.

O driver `setun-doom-rv32` usa o modo rápido por padrão e aceita `SETUN_RV32_VERACITY=1` para voltar às portas.

A branch `feat/rebel6-pure` é independente do DBT: executa instruções REBEL-6 de 32 trits em um core próprio, com palavra de 24 trits, 729 registradores e memória Harvard. O guest Doom é compilado com clang para RV32I, traduzido pelo R2R, montado pelo assembler `.tas` e executado pelo core.

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

### Branch `feat/rebel6-pure`

```bash
cd /home/robson/coding/setun-doom-rebel6

# Uma vez por worktree: copia, patcha e compila o R2R vendorizado
./scripts/setup_r2r.sh

# Pipeline completo: C -> clang -S rv32i -> normalização -> R2R -> doom_all.tas
python3 scripts/build_rebel6_doom.py

# Binários
cargo build --release --bin rebel6_doom --bin rebel6_run --bin rebel6_asm

# Doom no core REBEL-6, parando no segundo frame
./target/release/rebel6_doom vendor/doomgeneric/riscv32/build/rebel6/doom_all.tas 2

# Executar uma imagem .tas arbitrária
./target/release/rebel6_run caminho/arquivo.tas 100000000

# Medir os kernels SIMD
cargo run --release --bin simd_bench
```

Variáveis do driver RV32: `SETUN_RV32_VERACITY`, `SETUN_RV32_HEADLESS`, `SETUN_RV32_MAX_FRAMES`, `SETUN_RV32_NO_SLEEP`, `SETUN_RV32_TRACE_WAD`.

Variáveis do runner REBEL-6: `REBEL6_FRAME_TRACE=1` imprime ciclos e tempo por frame; `REBEL6_WAD_TRACE=1` imprime leituras do WAD; `REBEL6_TRACE=1` imprime o PC a cada milhão de instruções.

---

## 3. Medições de referência

### DBT e níveis 0 a 3

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

### Core REBEL-6 puro

| Cenário | Resultado |
| :-- | --: |
| Testes oficiais RV32I (`RiscvTests/PreprocessedAssembly`) | 37/37 |
| Dhrystone, estado final dos 32 registradores | idêntico ao simulador R2R |
| Doom, imagem `.tas` | 70 999 instruções, 3 555 004 bytes de dados, entrada 69 760, base de dados 378 664 |
| Doom, frame 2 | 63 847 pixels não nulos, E1M1 com HUD e status bar |
| Doom, ciclos até o frame 2 | 31 302 550 |
| Doom, tempo até o frame 2 | ~60 s no host de desenvolvimento |
| Doom, regime | ~0,14 FPS, 3,83 M ciclos por frame, ~563 mil instruções/s |
| Paridade com o frame de 21 trits | 97,5% dos pixels |

O primeiro frame leva ~44 s porque inclui carga do WAD, montagem do nível e ~26 M instruções de inicialização. O relógio do guest é determinístico: 1 ms por 1000 instruções aposentadas.

### Carry com sinal

`cargo run --release --bin setun-doom -- --no-gui` imprime as três linhas `Signed Carry`.

| Workload | `+1` | `-1` | `0` | Absorvidos | Absorção | Cadeia média | Máx |
| :-- | --: | --: | --: | --: | --: | --: | --: |
| BSP `R_PointOnSide` | 203 | 578 | 11 819 | 552 | 70,7% | 1,41 | 5 |
| Suíte DBT, 37 testes | 1 284 | 839 | 127 993 | 1 565 | 73,7% | 1,36 | 6 |
| Doom RV32IM | 469 350 | 650 231 | 13 196 497 | 725 425 | 64,8% | 1,54 | 11 |

Leitura: a maior parte dos carries morre no estágio seguinte, as cadeias são curtas e os carries negativos dominam em BSP e Doom porque a subtração é feita por inversão. O Doom ainda tem 7,64% mais flips que a referência binária, então o ganho energético depende do workload.

### SIMD

| Experimento | Resultado |
| :-- | --: |
| Tritwise `AND.T`/`OR.T`/`XOR.T`, 32 Ki trits em L1 | 4,7x a 4,8x |
| Negação bitsliced, L1 | 2,8x |
| Conversão de palette 64 000 pixels | 1,5x a 2,0x, integrada |
| `R_DrawSpan` de 320 pixels | 0,94x, rejeitado |

O caminho de palette está em `Memory::render_framebuffer_rgba`. O `R_DrawSpan` vetorial foi removido porque a divisão por constante já é reduzida a multiplicação e shift, e o gather custa mais que o laço escalar.

---

## 4. REBEL-6 e a adaptação para 21 trits

Fonte: Steven Bos, Vetle Bodahl, Ole Christian Moholth e Henning Gundersen, *"REBEL-6: A 32-trit
balanced ternary instruction set architecture with R2R compiler pipeline for C"*, IEEE ISMVL 2025,
pp. 98-103, DOI `10.1109/ISMVL64713.2025.00028`. O accepted manuscript está em
`docs/papers/rebel-6-ismvl-2025.pdf` sob **CC BY 4.0**. O código de referência é
`vendor/RV32IToREBEL`, MIT. O documento de mapeamento completo é `docs/rebel6.md`.

| Item | REBEL-2 | REBEL-6 | Setun-Doom 21 trits | Core puro |
| :-- | :-- | :-- | :-- | :-- |
| Instrução | 10 trits | 32 trits | 21 trits | 32 trits |
| Palavra de dados | trytes de 6 trits | 4 trytes, 24 trits | `Word21` | `Word24` |
| Registradores | 9 de 2 trits | 729 de 24 trits | 9 de índice, campo de 2 trits | 729 de 24 trits |
| Instruções | 19 | 263 na Tabela I | 45 implementadas | 56 do simulador R2R mais a camada RV32I |
| Endereço | curto | 24 trits | 14 trits | 24 trits em instruções G/Y |
| Memória | von Neumann | Harvard | von Neumann | Harvard |

Mapeamento de 21 trits: `cmp.t` → `Opcode::CmpT`, `majv.t` → `MajvT`, `minv.t` → `MinvT`, `sti.t` → `Neg`,
`lw.t`/`sw.t` → `Load`/`Store`, `BCEG` (REBEL-2) → `Bceg`. `li2.t` não tem equivalente. O paper cita
21 trits como alternativa viável para cobrir `u32`, o que sustenta a escolha do projeto.

Cuidado com telemetria: o paper conta cada digit flip com peso igual e ignora RAM e barramento,
reportando 33,24% de redução no Dhrystone. O projeto mantém essa métrica e adiciona uma métrica
ponderada de energia (half swing com peso 0,25). Os percentuais de energia do projeto não são
comparáveis ao 33,24% do paper.

### Core puro

- `src/rebel6/word.rs`: `Word24`, wrap balanceado módulo `3^24`, shifts, rotações, lógica trit a trit, maioria/minoria, flips e energia.
- `src/rebel6/encoding.rs`: sete formatos (R, I, B, G, X, D, Y), encoder/decoder empacotado em 2 bits por trit em `u64`.
- `src/rebel6/mem.rs`: memória de dados endereçada em byte/tryte, loads e stores binários little-endian.
- `src/rebel6/cpu.rs`: 729 registradores, `X0` fixo, traps, fetch Harvard.
- `src/rebel6/asm.rs`: assembler `.tas` de duas passagens, diretivas, labels, relocações e pseudo-instruções.
- `src/bin/rebel6_*.rs`: runner, assembler CLI e host Doom com 11 serviços `ecall`.

O encoding veio da especificação de referência pública do REBEL-6, não da dissertação original, que saiu do endereço antigo da USN. O documento `docs/rebel6-pipeline.md` registra a fonte, as divergências e os limites.

### Pipeline R2R

1. `clang -S -march=rv32i` por arquivo C.
2. Normalização: comentários, `.asciz` para `.byte`, `%hi`/`%lo` com addend, pseudo-instruções `zext`/`sext`/`tail`, labels locais renomeados.
3. Concatenação em um único `.s`, porque o R2R aceita um arquivo e não tem linker.
4. R2R gera `doom_all.tas`.
5. `rebel6_asm`/`rebel6_doom` montam e executam.

O R2R vendorizado recebeu um patch em `scripts/r2r.patch`: parser aceita `symbol+offset`, `.string` sem aspas, `main` sem as simulações RV32I/REBEL-6 e o assembler tolera o `.tas` gerado antes de estourar o tamanho fixo de memória.

---

## 5. Invariantes e regras

1. **Veracidade é o padrão.** Qualquer caminho rápido novo precisa de opt-in explícito e teste que prove equivalência arquitetural e telemetria zerada. Ver `tests/integration_execution_modes.rs`.
2. **Sem falhas silenciosas.** Proibido `let _ =` em operação falível. Lints `let_underscore_drop` e `clippy::let_underscore_must_use` ligados em `src/lib.rs` e no kernel.
3. **Contratos `// SAFETY:`** em todo `unsafe`, cobrindo alinhamento, aliasing e premissas FFI.
4. **`setun-doom` permanece `rlib`.** O artefato C fica em `crates/setun-ffi` (`staticlib`/`cdylib`), porque o unikernel precisa compilar a lib para `riscv64gc-unknown-none-elf`.
5. **`m_fixed.c` tem dois caminhos.** Sob `SETUN_RV32_GUEST` usa produto de 64 bits e divisão fracionária de 32 bits; fora dele mantém o código original, que o teste de codegen do nível 2 compila.
6. **Sem em dash, sem estruturas contrastivas retóricas** nos textos e commits.
7. **Doom usa 35 Hz de tic e dorme 1 ms** em `TryRunTics`; por isso o modo rápido estabiliza perto de 66 FPS.
8. **O core REBEL-6 é `no_std`-compatível.** `src/rebel6/{word,encoding,mem,cpu}.rs` não usam `std`; `asm.rs` e os binários são `std`. SIMD fica atrás de `cfg(all(feature = "std", target_arch = "x86_64"))`.
9. **SIMD só entra com medição.** O caminho de palette ganhou; o `R_DrawSpan` perdeu e foi removido. Não integrar kernel sem benchmark antes e depois.
10. **R2R é ferramenta externa.** O diretório `vendor/RV32IToREBEL` é ignorado pelo git. `scripts/setup_r2r.sh` copia do worktree principal, aplica `scripts/r2r.patch` e compila. Uma worktree nova não tem o R2R até rodar o script.
11. **O guest Doom usa `-march=rv32i`.** `__mulsi3`, `__divsi3`, `__udivsi3`, `__modsi3`, `__umodsi3` estão em `src/softmul.c`. `__muldi3` usa shift-add para não recursar no RV32I.

---

## 6. Armadilhas já encontradas

| Sintoma | Causa | Correção |
| :-- | :-- | :-- |
| `Bad R_RenderWallRange: 161 to 159` no guest | `FixedDiv` do C vendorizado estoura 32 bits em `r << 16` | caminho `SETUN_RV32_GUEST` com divisão longa de 32 bits |
| `W_GetNumForName: STCFN33 not found` | `printf` ignorava precisão `%.3d` | formatador com precisão de inteiro |
| ponteiro inválido ao abrir o WAD | `wad_file_t.file_class` nulo | classe `stdc_wad_file` apontada no `W_OpenFile` |
| guest para com `ReturnedFromMain` | `D_DoomMain` retorna após um tick | loop externo chamando `doomgeneric_Tick` |
| link do driver C usa código velho | `staticlib` removido do manifesto | `crates/setun-ffi` e `Makefile.setun` com caminho relativo |
| R2R aborta em `.globl foo # comentário` | parser não ignora comentário na linha | remover comentários antes do R2R |
| R2R rejeita `symbol+offset` | parser só aceitava símbolo puro | patch em `parser.cpp` e `loadSymbolAddress` |
| strings do Doom deslocadas | `.string` do R2R emitia as aspas; escapes octais `\000` viravam `0`, `0` | expandir strings para `.byte` e decodificar octal com até 3 dígitos |
| `__muldi3` recursa infinito | multiplicação de 64 bits no RV32I chama o próprio helper | shift-add |
| palette preta no primeiro frame | `I_GetTime` dependia do relógio real | relógio determinístico de 1 ms por 1000 instruções |
| `R_TextureNumForName: IRT not found` | tabela `alphSwitchList` deslocada por strings com NUL extra | corrigir o escape octal na normalização |
| `draw_span` SIMD mais lento | divisão por constante já é força-reduzida; gather custa mais | manter escalar |

---

## 7. Mapa de arquivos

- `src/dbt.rs`: tradução RV32IM, arena plana de operações, índice direto por PC, modos `VERACITY`.
- `src/mmo.rs`: FFI, contadores de hooks, lock com recuperação de envenenamento, ROMs de fixed-point e BSP.
- `src/stack_machine.rs`: Setun-70 de duas pilhas, assembler textual, testes e benchmark.
- `src/rebel6/`: core REBEL-6 puro, encoding de 32 trits, memória, CPU e assembler `.tas`.
- `src/bin/rebel6_run.rs`: executa uma imagem `.tas`.
- `src/bin/rebel6_asm.rs`: monta uma imagem `.tas` e reporta o layout.
- `src/bin/rebel6_doom.rs`: host Doom com WAD, palette, blit, teclado, relógio e sleeps.
- `src/simd.rs`: kernels AVX2 bitsliced e conversão de palette.
- `src/bin/simd_bench.rs`: benchmark dos kernels e da palette.
- `scripts/setup_r2r.sh`: copia, patcha e compila o R2R.
- `scripts/build_rebel6_doom.py`: pipeline C → RV32I → R2R → `.tas`.
- `scripts/r2r.patch`: patch de compatibilidade do R2R.
- `vendor/doomgeneric/riscv32/`: guest freestanding (headers, libc, soft-float, soft-mul, ABI `ecall`, linker, Makefile).
- `src/bin/setun_doom_rv32.rs`: driver host do nível 1 (SDL, paleta, teclado, relógio, WAD).
- `docs/rebel6.md`, `docs/rebel6-pipeline.md`, `docs/carry-telemetry.md`, `docs/simd.md`: referência, pipeline, carry com sinal e SIMD.
- `docs/papers/rebel-6-ismvl-2025.pdf`: paper aceito em CC BY 4.0.
- `tests/integration_fixed_oracle.rs`, `tests/integration_bsp_oracle.rs`: oráculos contra o C vendorizado.
- `tests/integration_execution_modes.rs`: equivalência veracidade/rápido e telemetria zerada.
- `crates/setun-ffi/`: artefatos C (`libsetun_ffi.a`, `.so`).

---

## 8. Pendências

1. Caminho de acesso de 32 bits direto no DBT, para cortar o custo de memória por byte no modo rápido (estimativa de 20 a 30%).
2. `#[non_exhaustive]` nos enums públicos que evoluem (`Rv32iInstr`, `DmaTranscodeMode`) e corrigir a deriva do README (36 opcodes citados, 45 reais).
3. Rodar Miri por módulo depois do refactor da arena.
4. Levar o modo RPN do Setun-70 para o Setun-DOS ou para um parser infixa via Shunting-yard no testbench.
5. Implementar `li2.t` (dois imediatos de 6 trits) e avaliar `round` por shift e `min`/`max` ternários, citados no paper.
6. Decidir o destino de `src/dos/playpal.bin` (untracked, sem uso) e de `o que é aarch64?.md` (rascunho untracked).
7. Linker real de `.tas` no lugar da concatenação de unidades de tradução.
8. Medir carry com sinal no `Word24` do REBEL-6 com um modo de ripple de 24 estágios.
9. Fechar a fidelidade visual do Doom no core REBEL-6: a arma em primeira pessoa não é desenhada e alguns pixels de sprites mascarados divergem.
10. Avaliar AVX-512 (`vpscatterdd`, `vpermb`) para spans longos e colunas; o AVX2 de 320 pixels não compensa.

---

## 9. Git

- Branch de trabalho: `feat/rebel6-pure`, worktree `/home/robson/coding/setun-doom-rebel6`, rastreando `origin/feat/rebel6-pure`.
- Últimos commits: `d4d3e59` documentação SIMD, `4f87fcb` palette AVX2, `0834bf1` kernels SIMD, `a55d1e2` carry com sinal, `136802e` trace de frames, `18318c8` core REBEL-6 e pipeline Doom.
- `main` segue em `e2c1d16` no worktree principal, com o gate de precommit.
- Antes de commitar: `cargo test --release`, `cargo clippy --all-targets`, `cargo build --no-default-features --lib` e confirmar que o unikernel ainda compila.

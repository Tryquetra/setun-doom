# REBEL-6 no Setun-Doom

Documento de referência sobre o ISA **REBEL-6** e sobre como este projeto o adapta para uma palavra
de 21 trits. Fontes primárias e o mapeamento entre o paper e o código ficam aqui para evitar que a
atribuição se perca em comentários.

## 1. Fonte primária

* **Título:** *REBEL-6: A 32-trit balanced ternary instruction set architecture with R2R compiler
  pipeline for C*
* **Autores:** Steven Bos, Vetle Bodahl, Ole Christian Moholth, Henning Gundersen (University of
  South-Eastern Norway, Kongsberg)
* **Veículo:** 2025 IEEE 55th International Symposium on Multiple-Valued Logic (ISMVL), páginas 98 a
  103
* **DOI:** [10.1109/ISMVL64713.2025.00028](https://doi.org/10.1109/ISMVL64713.2025.00028)
* **Cópia local:** [`docs/papers/rebel-6-ismvl-2025.pdf`](papers/rebel-6-ismvl-2025.pdf), accepted
  manuscript distribuído pelo NVA (Sikt), licença **CC BY 4.0**
* **Código de referência:** [Soppe/RV32IToREBEL](https://github.com/Soppe/RV32IToREBEL), vendorizado
  em [`vendor/RV32IToREBEL`](../vendor/RV32IToREBEL), licença MIT
* **Dissertação:** MSc de Vetle Bodahl, USN, base do pipeline R2R (referência [12] do paper)

## 2. O que o ISA define

| Item | REBEL-2 | REBEL-6 |
| :-- | :-- | :-- |
| Largura da instrução | 10 trits | 32 trits |
| Palavra de dados | trytes de 6 trits | 4 trytes, 24 trits |
| Registradores | 9 | 729 ($3^6$) |
| Largura do registrador | 2 trits | 24 trits |
| Operandos por instrução | 2 a 4 | 2 a 4 |
| Contagem de instruções | 19 | 263 (229 ternárias, 27 RV32I, 7 candidatas) |
| Salto de PC por instrução | 1 | 1 |

Formatos de instrução do REBEL-6: `R`, `I`, `B`, `G`, `X`, `D` e `Y`. Os formatos `R`, `I`, `B` e `G`
também atendem à compatibilidade binária com RV32I; `X`, `D` e `Y` são exclusivamente ternários.

Instruções candidatas da Tabela II do paper:

| Instrução | Nome | Formato | Semântica |
| :-- | :-- | :-- | :-- |
| `majv.t` | Majority vote | D | `rd = n` quando pelo menos 2 de `{rs1, rs2, rs3}` valem `n`, trit a trit |
| `minv.t` | Minority vote | D | `rd = n` quando exatamente 1 de `{rs1, rs2, rs3}` vale `n` |
| `cmp.t` | Compare | R | `rd = +1` se `rs1 > rs2`, `0` se iguais, `-1` se menor |
| `li2.t` | Load two immediates | X | `rd1 = imm6a`, `rd2 = imm6b` |
| `sti.t` | Standard ternary inversion | R | inverte cada trit: `+1 -> -1`, `0 -> 0`, `-1 -> +1` |
| `lw.t` | Load word | G | `rd = mem[imm24]` |
| `sw.t` | Store word | Y | `mem[imm24] = rs` |

O paper registra 1,4% menos execuções de instrução e 33,24% menos digit flips no Dhrystone em relação
ao RV32I. A métrica de digit flips conta cada trit alterado com peso igual e ignora RAM e barramento
de dados.

Observação: o paper cita 263 instruções na Tabela I, 56 na seção II e 63 na discussão. A diferença é
uma inconsistência do próprio paper; este documento usa o valor da Tabela I.

## 3. Por que este projeto usa 21 trits

O paper diz explicitamente que palavras de 21 trits caberiam em inteiros de 32 bits, mas escolhe 24
trits (4 trytes) para simplificar o alinhamento e a tradução binário-ternário. O Setun-Doom escolhe
21 trits por outro motivo: $3^{21} = 10\,460\,353\,203 > 2^{32}$, então um único `Word21` cobre
`i32`, `u32` e somas sem estouro de dois valores de 32 bits, sem palavra extra.

Consequências da adaptação:

| Dimensão | REBEL-6 | Setun-Doom |
| :-- | :-- | :-- |
| Palavra | 32 trits | 21 trits (`Word21`) |
| Endereço | 24 trits (`imm24`) | 14 trits ($3^{14} = 4\,782\,969$ palavras) |
| Opcode | formatos de 32 trits | 5 trits ($3^5 = 243$ valores, 45 implementados) |
| Registradores | 729 de 24 trits | 9 registradores de índice (campo de 2 trits) |
| Compatibilidade RV32I | dual binary-ternary ALU | DBT RV32IM separado (`src/dbt.rs`) |

## 4. Mapeamento paper para código

| Recurso do paper | Opcode do projeto | Encoding | Status |
| :-- | :-- | :-- | :-- |
| `cmp.t` | `Opcode::CmpT` (`+16`) | `ACC = ACC <=> MEM[addr]` | adaptado para acumulador |
| `majv.t` | `Opcode::MajvT` (`+17`) | `ACC = MajorityVote(ACC, IX[reg], MEM[addr])` | adaptado |
| `minv.t` | `Opcode::MinvT` (`-17`) | `ACC = MinorityVote(ACC, IX[reg], MEM[addr])` | adaptado |
| `sti.t` | `Opcode::Neg` (`+3`) | inversão de fio dos 21 trits | adaptado |
| `li2.t` | não implementado | `LoadImm` carrega um único imediato de 14 trits | ausente |
| `lw.t` | `Opcode::Load` (`+4`) | `ACC = MEM[addr]` com 14 trits | adaptado |
| `sw.t` | `Opcode::Store` (`-4`) | `MEM[addr] = ACC` | adaptado |
| `bceg` (REBEL-2) | `Opcode::Bceg` (`+19`) | reg de 2 trits e dois offsets de 7 trits | adaptado |
| Formatos R/I/B/G/X/D/Y | layout de 21 trits (14 + 2 + 5) | operando, registrador, opcode | redesenhado |

O `BCEG` vem do REBEL-2, predecessor do REBEL-6. O tradutor vendorizado confirma o mapeamento de
`beq`, `blt`, `bge`, `bltu` e `bgeu` para `bceg`
([`rv32itorebel2.cpp:158-183`](../vendor/RV32IToREBEL/code/Translators/rv32itorebel2.cpp)).

## 5. Telemetria

O paper mede **digit flips** com peso igual por trit alterado. O Setun-Doom implementa essa contagem
(`PowerTelemetry::digit_flip_reduction_pct`) e adiciona uma métrica ponderada de energia, em que um
half swing (`0 <-> +1`) pesa 0,25 de um full swing (`-1 <-> +1`), exposta em
`PowerTelemetry::weighted_energy_reduction_pct`.

Consequências:

* O percentual de **digit flips** do projeto é comparável ao 33,24% do paper apenas em metodologia,
  não em valor, porque o workload e a largura da palavra diferem.
* O percentual de **energia** do projeto é uma extensão, não um número reportado pelo paper. O paper
  afirma que half swings gastam menos energia, mas conta todos os flips igualmente.
* A contagem cobre transições de registrador; RAM e barramento ficam fora, como no paper.

## 6. Verificação

| Item | Paper | Setun-Doom |
| :-- | :-- | :-- |
| Suíte oficial RV32I | 37 testes | 37/37 em `vendor/riscv_tests/elf` |
| Extensão M | não coberta | 37 testes RV32M e benchmark Doom RV32IM |
| Benchmark | Dhrystone modificado | Dhrystone, BSP, `R_DrawColumn`, busca de 3 vias |

## 7. O que ainda falta

1. Implementar `li2.t` (dois imediatos de 6 trits em uma instrução), que hoje não tem equivalente.
2. Avaliar `round` por shift e `min`/`max` ternários, citados no paper como parte da lista completa (o texto cita 56 instruções, a discussão cita 63 e a Tabela I cita 263).
3. Registrar em `THIRD_PARTY_NOTICES.md` qualquer trecho de código derivado do repositório vendorizado.

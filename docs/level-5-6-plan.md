# Level 5 and Level 6 Plan

Execution plan for the two roadmap levels added in the README: the native REBEL-6
toolchain (Level 5) and specification-first verification for machine-generated code
(Level 6). It records the next steps, the decisions taken to start, and the acceptance
criteria for each phase.

## Current baseline

| Item | State |
| :-- | :-- |
| C to REBEL-6 | clang `-march=rv32i`, translated by R2R |
| R2R tooling | source-level linker (`--link`), `.mto` objects, R6IM images, encoder, 37/37 RV32I tests, `--diff`, `--stats`, deterministic ctest |
| Native codegen | `src/codegen` parses LLVM IR and emits the 21-trit Setun ISA |
| Runtime | freestanding shim, 11 `ecall` services, no libc |
| Doom image | 75,509 instructions, 4,357,942 data trytes, 243,848 BSS trytes |

## Next steps

| Order | Step | Why now | Acceptance |
| --: | :-- | :-- | :-- |
| 1 | Move the build to `R2R --link` | Removes the 126,931-line concatenation and the Python scoping logic; the linker already exists | Identical `.tas` and image, frame 2 unchanged |
| 2 | Add `--emit-map` and `--emit-syms` to R2R | The diagnostic map is a Level 6 prerequisite and currently comes from Python | Map with unit, C line and combined line per expression |
| 3 | Write `docs/abi.md` | The ABI is the contract for the backend, the runtime and verification | Calling convention, register classes, stack, pointers, `char`, varargs, structs (draft written, `abi-v0`, awaits the freeze gate) |
| 4 | Single source for the ISA specification | The encoder is generated from the Rust spec; the assembler and simulator keep copies | One file generates encoder, assembler tables, decoder and tests |
| 5 | Retarget `src/codegen` to REBEL-6 | The LLVM IR pipeline exists, but emits the 21-trit Setun ISA | A whole C program compiles and runs on the REBEL-6 core |
| 6 | C harness: gcc-torture, Csmith, CoreMark, Dhrystone | The 37 RV32I tests prove the translator, not a C compiler | Pass rate and native-versus-translated comparison |
| 7 | Minimal runtime: start, syscalls, `malloc`, `stdio`, `setjmp` | Real C programs need a runtime | Hello world, allocation and I/O working |
| 8 | REBEL-6 decompiler to C or Rust with round-trip | This is the human view of Level 6 | Decompiled binary recompiles to the same behavior |

## Phases

**Level 5, foundation (steps 1 to 5).** Modernize the toolchain and start the native
backend. Exit criterion: an arithmetic C program without libc, compiled by LLVM for
REBEL-6 and executed on the core with the same result as the RV32I path.

**Level 5, completeness (steps 6 and 7).** ABI implemented, runtime and reduced libc,
conformance suite. Exit criterion: a real C subset with structs, varargs, pointers and
allocation, measured against the RV32I translation in instructions, cycles, density and
energy.

**Level 6 (steps 2, 4 and 8, deepened).** The specification becomes the executable
source, the decompiler provides the human view, and translation validation ties the view
to the binary. Exit criterion: a reproducible artifact with hash, map and a proof or test
of equivalence between source, IR, binary and view.

## Decisions taken to start

The rule for this phase: pick the easiest reversible option, and version anything that
is expensive to change later. The ABI starts as `abi-v0`, so a future 24-trit pointer
model can coexist with this one.

| Decision | Chosen now | Why it is the easiest | Cost to change later |
| :-- | :-- | :-- | :-- |
| Pointer width | 32-bit binary, byte addressed | Reuses the RV32I path, libc shims, Doom and the current data model | High, requires ABI v2 |
| `char` representation | 8-bit binary inside a 24-trit cell | C compatibility, current memory layout | High, C standard assumption |
| Endianness | Little-endian binary | Current data path and Doom | High |
| Calling convention | RISC-V-like: `a0`-`a7`, `ra`, `sp`, `s0`-`s11` | The translator and libc shim already use it | High |
| Active register window | 32 registers, spill to the 729 later | Same mapping as today | Low, internal to the backend |
| Backend | Retarget the existing `src/codegen` LLVM IR pipeline | Code exists; avoids writing a C frontend | Medium |
| Object format | Keep textual `.mto`, add numeric IDs later | Already works and is debuggable | Low |
| Linker model | Source-level `--link`, object-level later | Already tested | Low |
| Memory model | Flat 8 MiB, byte addressed | Current core and runtime | Medium |
| Verification | Differential tests and fuzzing first, proofs later | `--diff` and the test runner exist | Low |
| Threads, atomics, float | Deferred; single-threaded, soft-float later | Avoids A extension and F/D work | Medium |
| ABI versioning | `abi-v0` from day one | Cheap and enables later changes | Low |

## Risks

- A native backend without a runtime and libc does not run real C. Runtime and libc enter
  early, not at the end.
- A human view without translation validation creates false confidence. The decompiler
  ships only with a round-trip test.
- The textual `.mto` and string-based linker limit Level 6. Numeric IDs and consumed
  relocations must arrive before scaling.
- The ternary advantage must be measured on the native path, because the RV32I path
  carries binary semantics and leaves most of the 729 registers idle.

## First week

1. Open `feat/level-5-rebel6-native`.
2. Migrate `build_rebel6_doom.py` to `R2R --link` and compare image and frame.
3. Add `--emit-map` and `--emit-syms` to R2R.
4. Write `docs/abi.md` with the calling convention and the pointer model.
5. Create `tests/level5/` with an arithmetic C program and the `rebel6_cc` driver skeleton.

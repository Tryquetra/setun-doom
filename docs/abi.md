# REBEL-6 ABI, version `abi-v0`

Status: proposal, not frozen. This document records the decisions that the native backend, the
runtime and the verification harness depend on. Every decision carries its reversibility class
from [`directives.md`](directives.md): a hat is cheap to undo, a haircut costs a period of regrowth,
a tattoo is expensive or impossible to undo.

The document freezes when the first artifact leaves the repository: the Doom guest image plus the
arithmetic C program of `tests/level5/`. Until then, changing a tattoo still costs only an edit here.

## 1. Scope

This ABI (Application Binary Interface, contract for calls and data layout) covers the native
REBEL-6 path: the 32-trit instruction set, 24-trit data words, 729 registers, and code produced by
the native backend or by R2R.

Out of scope, decided elsewhere: object file format (`.mto` today), linker model (`--link` at the
source level today), decompiler output language, and the 21-trit Setun ISA, which keeps its own
conventions.

## 2. Data model

The model is ILP32 (int, long and pointer of 32 bits, the data model of the RV32I psABI, the
processor-specific ABI). It is the model clang already emits for `-march=rv32i`, so the RV32I
translation path, the guest libc shim and the Doom port keep working unchanged.

| C type | Size | Alignment | Notes |
| :-- | --: | --: | :-- |
| `char`, `signed char`, `unsigned char` | 1 | 1 | 8-bit binary value in a 24-trit cell |
| `short`, `unsigned short` | 2 | 2 | little-endian |
| `int`, `unsigned int` | 4 | 4 | little-endian |
| `long`, `unsigned long` | 4 | 4 | ILP32, not LP64 |
| `long long`, `unsigned long long` | 8 | 8 | lowered to `__muldi3` and shift-add on RV32I |
| `float`, `double` | 4, 8 | 4, 8 | soft-float only, no F/D registers |
| pointer, function pointer | 4 | 4 | binary value in a 24-trit cell, see section 5.2 |
| `_Bool` | 1 | 1 | `0` or `1` |
| struct, union | per member | per largest member | no bit-field packing beyond the C rules |

Aggregates follow the psABI rules: padding only where C requires it, no reordering of members, and
bit-fields allocated within the declared base type.

Class: tattoo. Recorded now because every later decision reads it.

## 3. Registers and calling convention

### 3.1 Register roles

The file has 729 registers of 24 trits. The ABI names the first 32 with the psABI RISC-V numbering;
the remaining 697 are backend-internal and may be used as a window or as spill space without
affecting binaries.

| Register | psABI name | Role in `abi-v0` | Saved by |
| :-- | :-- | :-- | :-- |
| `x0` | `zero` | hardwired zero | none |
| `x1` | `ra` | return address | caller |
| `x2` | `sp` | stack pointer, 16-byte aligned | callee restores |
| `x3` | `gp` | global pointer for small data, set at `_start` | none |
| `x4` | `tp` | thread pointer, reserved, zero in single-thread builds | none |
| `x5`-`x7` | `t0`-`t2` | temporaries | caller |
| `x8` | `s0`/`fp` | frame pointer, optional | callee |
| `x9` | `s1` | saved | callee |
| `x10`-`x17` | `a0`-`a7` | arguments, return values, syscall number in `a7` | caller |
| `x18`-`x27` | `s2`-`s11` | saved | callee |
| `x28`-`x31` | `t3`-`t6` | temporaries | caller |

### 3.2 Calls

- Arguments go in `a0` to `a7` in declaration order; the rest go on the stack in order, each
  argument aligned to its own alignment.
- Integer and pointer results return in `a0`; a 64-bit result uses `a0` for the low word and `a1`
  for the high word.
- A struct or union of at most 8 bytes returns in `a0` and `a1`; a larger one returns through a
  caller-allocated buffer whose address is passed as a hidden first argument in `a0`.
- A struct or union of at most 8 bytes passes by value in `a0` and `a1`; a larger one passes by
  value on the stack, aligned to 8 bytes.
- `sp` is 16-byte aligned at every call site. The frame holds the saved `ra` and the callee-saved
  registers it uses, in that order, below the incoming arguments.
- There is no red zone. Leaf functions still allocate a frame before using stack memory.
- Soft-float only: `float` and `double` arguments travel in integer registers as their bit pattern,
  and `a0` carries the number of vector arguments at every call, which is always zero.

Class: tattoo for everything a binary observes (register roles, argument and return placement,
stack alignment, absence of a red zone). The 697-register window is a hat: internal to the backend.

### 3.3 Status of the implementation

The RV32I guest today already follows these rules because clang generated it. The native backend
does not exist yet, so nothing contradicts the document. The backend must implement section 3
before the conformance suite runs.

## 4. System call and trap ABI

### 4.1 Call form

`ecall` is the only host interface. The service number goes in `a7` (`x17`), arguments in `a0` to
`a5`, and the result returns in `a0`. The number in `a7` reaches the host as the `ECALL.T` outcome
(`src/rebel6/cpu.rs:366-368`), and the guest side already writes it that way
(`vendor/doomgeneric/riscv32/src/setun_syscall.h:32`).

### 4.2 Services

| Number | Name | `a0` | `a1` | `a2` | Returns in `a0` |
| --: | :-- | :-- | :-- | :-- | :-- |
| 0 | `exit` | status | | | does not return |
| 1 | `write` | buffer address | length | | bytes written, or negative errno |
| 2 | `ticks_ms` | | | | milliseconds since start |
| 3 | `blit` | index buffer address | | | 0, or negative errno |
| 4 | `getkey` | pressed address | key address | | 1 if a key was written, 0 if none |
| 5 | `wad_open` | | | | 0 on success, negative errno |
| 6 | `wad_seek` | absolute offset | | | 0 on success, negative errno |
| 7 | `wad_read` | buffer address | length | | bytes read, or negative errno |
| 8 | `wad_size` | | | | size in bytes |
| 9 | `sleep_ms` | milliseconds | | | 0 |
| 10 | `palette` | 768-byte RGB address | | | 0, or negative errno |

### 4.3 Errors

Every service that can fail returns a negative errno in `a0`. The numbers are the Linux generic
values, borrowed instead of invented: `-EPERM (-1)`, `-ENOENT (-2)`, `-EIO (-5)`, `-EBADF (-9)`,
`-EFAULT (-14)`, `-EINVAL (-22)`. A value of zero from `wad_read` means end of file; a failure
returns a negative number.

This section changes current behavior. Today `SYS_WAD_OPEN` returns a boolean and `SYS_WAD_READ`
returns 0 both at end of file and on failure (`src/bin/rebel6_doom.rs:75`), which loses
information. The host and the guest shim both need the change before the ABI freezes.

### 4.4 Traps

A trap carries a cause number, the faulting program counter and the faulting address where one
applies. The numbering below maps one to one to the core enumeration
(`src/rebel6/cpu.rs:20`) and is proposed here; the core itself carries no numbers yet.

| Cause | Trap | Extra fields |
| --: | :-- | :-- |
| 0 | invalid instruction | word |
| 1 | invalid register index | index |
| 2 | memory fault | address |
| 3 | breakpoint | program counter |
| 4 | trap return without a frame | program counter |
| 5 | wait for interrupt without a controller | program counter |
| 6 | step limit reached | program counter |

Cause 6 is a host diagnostic, not an architectural trap. Interrupts and privilege levels do not
exist in `abi-v0`; `wfi.t` and `tret.t` trap as causes 5 and 4.

Class: tattoo for numbers, argument placement and the error convention, because binaries encode
them. The errno set is a haircut: adding a code later costs a table revision, changing an existing
one does not.

## 5. Memory model

### 5.1 Addressing

Memory is byte addressed. Addresses are cell indices; a cell is 24 trits, holds `0..=255` for
binary accesses and `-364..=364` for ternary accesses, and the host stores one `i16` per cell
(`src/rebel6/mem.rs:29`). Loads and stores are little-endian binary, as the RV32I guest expects
(`docs/rebel6-pipeline.md:87`).

Byte and halfword accesses are allowed at any address. Word accesses must be 4-byte aligned by ABI.
The core does not enforce that today: `load_u32` assembles four consecutive cells whatever the
address is (`src/rebel6/mem.rs:99`), so the ABI must not rely on the tolerance, and a future core
may raise cause 2 instead.

### 5.2 Pointer width

`abi-v0` pointers are 32-bit binary values. A 24-trit cell holds any 32-bit value, because
$3^{24} = 282{,}429{,}536{,}481$ exceeds $2^{32} - 1 = 4{,}294{,}967{,}295$. Pointer arithmetic is
binary, comparison is unsigned, and the value reaches the address unit unchanged.

The native address field of the `G` and `Y` instruction formats is 24 trits, so a 32-bit pointer
travels in a register without loss and only the low address range is populated. The populated
window is the flat data space that follows the instruction image; the size is a runtime parameter,
not an ABI constant.

### 5.3 Regions

| Region | Contents | Growth |
| :-- | :-- | :-- |
| Instruction image | assembled `.tas` code, read-only to the guest | fixed at load |
| Data | initialized `.data`, then `.bss` as NOBITS | fixed at load |
| Heap | `malloc` arena, grows upward from the end of data | runtime |
| Stack | grows downward from the stack top | runtime |

Current defaults: 8 MiB of data memory (`src/rebel6/asm.rs:19`). The stack top is not fixed: the
assembler exposes `DEFAULT_STACK_TOP = 0x40_0000` (`src/rebel6/asm.rs:23`) while the Doom runner
writes `6_500_000` into `sp` before starting (`src/bin/rebel6_doom.rs:29,245`). The ABI leaves the
stack top to the loader, and the two values are an open inconsistency to settle with the runtime,
not an ABI constant. The guest expects the host to place `sp` before `_start`
(`vendor/doomgeneric/riscv32/src/start.S`), so the image never carries it.

There is no MMIO (memory-mapped I/O, input and output mapped into memory) region in `abi-v0`.
Device access happens only through `ecall`. A memory-mapped device space would be a new region and a
new ABI version.

Class: tattoo for byte addressing, endianness and the absence of MMIO. Haircut for the exact region
sizes, which stay runtime parameters.

## 6. Versioning and freeze gate

- The version string is `abi-v0` and travels in the artifact manifest.
- The document freezes when the first artifact leaves the repository: Doom guest image plus the
  `tests/level5/` C program, both with a manifest.
- After the freeze, a tattoo change requires an ADR (Architecture Decision Record, record of an
  architecture decision) and a migration note, and produces `abi-v1`. Hats and haircuts change
  without a version bump.
- `abi-v1` is reserved for the 24-trit pointer and the data model that follows from it. It must
  coexist with `abi-v0`, not replace it, so the RV32I compatibility track keeps running.

## 7. Decision record

| Decision | Chosen | Class | Missing to freeze |
| :-- | :-- | :-- | :-- |
| Data model | ILP32 | tattoo | nothing, recorded in section 2 |
| Pointer width | 32-bit binary in a 24-trit cell | tattoo | the 24-trit experiment, section 8 |
| `char` | 8-bit binary in a 24-trit cell | tattoo | nothing, C requirement |
| Endianness | little-endian binary | tattoo | nothing, RV32I compatibility |
| Calling convention | psABI RISC-V ILP32, no red zone | tattoo | backend implementing section 3 |
| Register window | 32 named, 697 internal | hat | nothing |
| Syscall numbers | 0 to 10, `a7` selector | tattoo | host and guest shim adopting sections 4.2 and 4.3 |
| Error convention | negative errno in `a0` | tattoo | the same host and guest change |
| Trap causes | 0 to 6 | tattoo | nothing, enumerated in section 4.4 |
| Address granularity | byte addressed, 24-trit cell | tattoo | nothing, current core |
| MMIO | absent, `ecall` only | tattoo | nothing |
| Region sizes | runtime parameters | haircut | nothing |
| Threads, atomics, float | single-thread, no atomics, soft-float | haircut | review at the runtime milestone |
| Versioning | `abi-v0`, freeze at first distributed artifact | haircut | the gate event |

## 8. The experiment that gates `abi-v1`

A 24-trit pointer changes struct layout, `malloc`, pointer arithmetic, libc and every object file.
It pays only if the native path measures better than the binary pointer on the same work. The
experiment is part of Level 5 and runs before any freeze of `abi-v1`.

| Item | Requirement |
| :-- | :-- |
| Workload | the `tests/level5/` C program, then the Doom guest |
| Metrics | instructions retired, cycles, image density, trit flips, weighted energy |
| Comparison | the same program with 32-bit pointers, both executed on the core |
| Correctness | identical observable output; frame 2 parity for the Doom run |
| Decision | adopt 24 trits only if the aggregate improves without a correctness loss |

Until the experiment produces those numbers, `abi-v0` stands as the compatibility layer, and the
document says so.

## 9. Open items not decided here

- Object format: `.mto` textual today. Numeric identifiers and consumed relocations enter when a
  second consumer exists, when the build leaves the repository, or when dead-code elimination is
  needed. Class: haircut.
- Linker level: `--link` at the source level today. Object-level linking arrives with the object
  format. Class: hat.
- Decompiler language: Rust, reusing `encoding::decode` and `Word24`. Class: hat.
- Verification depth: differential equivalence and round-trip for the Level 6 gate; proofs
  restricted to the encoder, decoder and assembler tables. Class: haircut.
- Artifact manifest: SHA-256 of the image, the map, the symbol table, the tool commit and the frame
  hash. Class: haircut.
- `.p2align`: stays opt-in. The latent layout assumption that makes honoured alignment break the
  frame is a defect to locate, not a decision to keep. Class: hat.

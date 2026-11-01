#!/usr/bin/env python3
"""Builds `tests/level5/arith.c` through the RV32I translation path.

The Level 5 exit criterion is that the native backend and the RV32I translation reach the same
result on the core for the same C program. This script produces the translated side: clang for
`riscv32-unknown-elf`, the normalization the Doom build uses, and `R2R --link` for the
translation. The native side comes from `rebel6-cc`.

Usage: `scripts/build_level5_parity.py [-O level]`; prints the path of the `.tas` it wrote.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

from build_rebel6_doom import CFLAGS, R2R, compile_assembly, explain, normalize

ROOT = HERE.parent
LEVEL5 = ROOT / "tests" / "level5"
GUEST = ROOT / "vendor" / "doomgeneric" / "riscv32"
BUILD = GUEST / "build" / "level5"

# `abi-v0` places the stack pointer before the entry, and the entry returns `main`'s value in `a0`
# through the exit syscall, so both paths leave the same number behind.
START = """
	.text
	.globl _start
_start:
	call main
	li a7, 0
	ecall
"""


def flags(level: str) -> list[str]:
    """The Doom flags with the optimisation level replaced and the line directives dropped."""
    rest = [flag for flag in CFLAGS if not flag.startswith("-O") and not flag.startswith("-g")]
    return [f"-O{level}", *rest]


def main() -> None:
    level = "0"
    if "-O" in sys.argv:
        position = sys.argv.index("-O")
        if position + 1 >= len(sys.argv):
            raise SystemExit("usage: build_level5_parity.py [-O level]")
        level = sys.argv[position + 1]
    if not R2R.exists():
        raise SystemExit(f"{R2R} is missing; run ./scripts/setup_r2r.sh first")
    BUILD.mkdir(parents=True, exist_ok=True)

    units = [normalize(START, "start", None)]
    for index, source in enumerate((LEVEL5 / "arith.c", GUEST / "src" / "softmul.c")):
        units.append(normalize(compile_assembly(source, flags(level)), f"u{index}", source))

    paths: list[Path] = []
    for unit in units:
        stem = unit.path.stem if unit.path else "start"
        path = BUILD / f"{stem}.s"
        lines = list(unit.lines)
        if unit.path is not None:
            # File index 1 is the unit itself, so the linker can name the C source in its map.
            lines.insert(0, f'\t.file 1, "{unit.path}"')
        path.write_text("\n".join(lines) + "\n")
        paths.append(path)

    result = subprocess.run(
        [str(R2R), "--link", "-o", "arith_rv32i", *[str(path) for path in paths]],
        cwd=BUILD,
        text=True,
        capture_output=True,
    )
    tas = BUILD / "arith_rv32i.tas"
    if not tas.exists():
        explain(result.stdout + result.stderr, units)
        sys.stderr.write(result.stdout)
        sys.stderr.write(result.stderr)
        raise SystemExit("R2R produced no .tas")
    if result.returncode != 0:
        print(f"note: R2R exited {result.returncode} after writing the .tas (assembler stage)")
    print(tas)


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Build the doomgeneric guest for the REBEL-6 core.

Pipeline: clang -S (rv32i) -> text normalization -> single concatenated .s -> R2R -> .tas.

The R2R reference tool accepts one assembly file and has no linker, so every translation unit
is normalized and concatenated into one source before translation. Local labels are renamed per
file to keep them unique.
"""

from __future__ import annotations

import re
import shutil
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
GUEST = HERE.parent / "vendor" / "doomgeneric" / "riscv32"
ENGINE = GUEST.parent / "doomgeneric"
BUILD = GUEST / "build" / "rebel6"
R2R = HERE.parent / "vendor" / "RV32IToREBEL" / "build" / "RV32IToREBEL"

EXCLUDED = {
    "w_file_stdc.c",
    "i_cdmus.c",
    "i_joystick.c",
    "i_sdlmusic.c",
    "i_sdlsound.c",
    "i_allegrosound.c",
    "i_allegromusic.c",
    "i_sound.c",
}

CFLAGS = [
    "--target=riscv32-unknown-elf",
    "-march=rv32i",
    "-mabi=ilp32",
    "-O2",
    "-ffreestanding",
    "-fno-common",
    "-fno-stack-protector",
    "-fno-pic",
    "-mno-relax",
    "-G0",
    "-fno-addrsig",
    "-fno-asynchronous-unwind-tables",
    "-mllvm",
    "-enable-global-merge=false",
    "-fno-unwind-tables",
    f"-I{ENGINE}",
    f"-I{GUEST / 'src'}",
    f"-isystem{GUEST / 'include'}",
    "-DSETUN_RV32_GUEST",
    "-DCMAP256",
    "-DDOOMGENERIC_RESX=320",
    "-DDOOMGENERIC_RESY=200",
    "-DNORMALUNIX",
    "-D_DEFAULT_SOURCE",
]


def sources() -> list[Path]:
    engine = sorted(
        path
        for path in ENGINE.glob("*.c")
        if not path.name.startswith("doomgeneric_") and path.name not in EXCLUDED
    )
    port = [GUEST / "src" / name for name in ("libc.c", "platform.c", "stubs.c", "softfloat.c", "softmul.c")]
    return engine + port


def normalize(text: str, prefix: str) -> str:
    # Strip comments first: R2R mis-parses a trailing comment on `.globl`.
    # The scanner respects string literals so URLs and `#` inside `.string` survive.
    def strip_comments(source: str) -> str:
        out = []
        for line in source.splitlines():
            in_quote = False
            cut = len(line)
            i = 0
            while i < len(line):
                c = line[i]
                if c == '"' and (i == 0 or line[i - 1] != "\\"):
                    in_quote = not in_quote
                if not in_quote:
                    if c in "#;":
                        cut = i
                        break
                    if c == "/" and i + 1 < len(line) and line[i + 1] == "/":
                        cut = i
                        break
                i += 1
            out.append(line[:cut])
        return "\n".join(out)

    text = strip_comments(text)

    # Drop directives the reference assembler does not accept.
    drop = re.compile(
        r"^\s*\.(?:attribute|file|ident|option|type|size|addrsig|addrsig_sym|cfi_[a-z_]+|local|weak|hidden|protected)\b"
    )
    lines = [line for line in text.splitlines() if not drop.match(line)]
    text = "\n".join(lines)

    # R2R collapses lui/auipc hi/lo pairs and rejects addends inside a modifier.
    # Rewrite `lui rd, %hi(sym+off)` into `li rd, sym` plus `addi rd, rd, off`.
    def hi_lui(match: re.Match[str]) -> str:
        reg, expr = match.group(1), match.group(2)
        inner = re.fullmatch(r"([A-Za-z0-9_.$]+)([+-]\d+)?", expr.strip())
        if inner is None:
            return match.group(0)
        symbol, offset = inner.group(1), inner.group(2)
        out = f"\tli {reg}, {symbol}\n"
        if offset:
            out += f"\taddi {reg}, {reg}, {offset}\n"
        return out

    def hi_auipc(match: re.Match[str]) -> str:
        reg, expr = match.group(1), match.group(2)
        inner = re.fullmatch(r"([A-Za-z0-9_.$]+)([+-]\d+)?", expr.strip())
        if inner is None:
            return match.group(0)
        symbol, offset = inner.group(1), inner.group(2)
        out = f"\tauipc {reg}, %pcrel_hi({symbol})\n"
        if offset:
            out += f"\taddi {reg}, {reg}, {offset}\n"
        return out

    text = re.sub(r"^\s*lui\s+([a-z0-9]+),\s*%hi\(([^)]*)\)\s*$", hi_lui, text, flags=re.MULTILINE)
    text = re.sub(r"^\s*auipc\s+([a-z0-9]+),\s*%pcrel_hi\(([^)]*)\)\s*$", hi_auipc, text, flags=re.MULTILINE)
    text = re.sub(
        r"^\s*lui\s+([a-z0-9]+),\s*(0x[0-9a-fA-F]+|\d+)\s*$",
        lambda m: f"\tli {m.group(1)}, {int(m.group(2), 0) << 12}",
        text,
        flags=re.MULTILINE,
    )
    text = re.sub(r"%lo\([^)]*\)\(([A-Za-z0-9_]+)\)", r"0(\1)", text)
    text = re.sub(r",\s*%lo\([^)]*\)", ", 0", text)
    text = re.sub(r"%pcrel_lo\([^)]*\)", "0", text)

    # Clang emits `.asciz`; the reference assembler only knows `.string`.
    text = re.sub(r"\.asciz\b", ".string", text)
    text = re.sub(r"\.ascii\b", ".string", text)

    # Expand string literals into `.byte` sequences. R2R's lexer does not handle escaped
    # quotes inside `.string`, and `.byte` is unambiguous.
    def expand_string(match: re.Match[str]) -> str:
        body = match.group(1)
        out: list[int] = []
        i = 0
        while i < len(body):
            c = body[i]
            if c != "\\":
                out.extend(c.encode("utf-8"))
                i += 1
                continue
            i += 1
            if i >= len(body):
                break
            esc = body[i]
            mapping = {"n": 10, "t": 9, "r": 13, "\\": 92, '"': 34, "'": 39}
            if esc in "01234567":
                digits = esc
                i += 1
                while i < len(body) and len(digits) < 3 and body[i] in "01234567":
                    digits += body[i]
                    i += 1
                out.append(int(digits, 8) & 0xFF)
            elif esc in mapping:
                out.append(mapping[esc])
                i += 1
            elif esc == "x" and i + 2 < len(body):
                out.append(int(body[i + 1 : i + 3], 16))
                i += 3
            else:
                out.append(ord(esc))
                i += 1
        out.append(0)
        return "\t.byte " + ",".join(str(v) for v in out)

    text = re.sub(r'^\s*\.string\s+"((?:[^"\\]|\\.)*)"\s*$', expand_string, text, flags=re.MULTILINE)

    # `.comm` is not understood by the reference assembler; expand it into a bss symbol.
    def common(match: re.Match[str]) -> str:
        name, size = match.group(1), int(match.group(2))
        return f"\n.bss\n.globl {name}\n{name}:\n.zero {size}\n"

    text = re.sub(r"^\s*\.comm\s+([A-Za-z0-9_.$]+)\s*,\s*(\d+)(?:\s*,\s*\d+)?\s*$", common, text, flags=re.MULTILINE)

    # Collapse section variants onto the four sections R2R understands.
    def section(match: re.Match[str]) -> str:
        name = match.group(1)
        if ".text" in name:
            return ".section .text"
        if ".bss" in name:
            return ".section .bss"
        if ".rodata" in name:
            return ".section .rodata"
        return ".data"

    text = re.sub(r"\.section\s+([^,\n]+).*", section, text)

    # Expand clang pseudo-instructions the reference translator does not know.
    text = re.sub(r"^\s*zext\.b\s+(\w+),\s*(\w+)\s*$", r"\tandi \1, \2, 255", text, flags=re.MULTILINE)
    text = re.sub(r"^\s*zext\.h\s+(\w+),\s*(\w+)\s*$", r"\tslli \1, \2, 16\n\tsrli \1, \1, 16", text, flags=re.MULTILINE)
    text = re.sub(r"^\s*sext\.b\s+(\w+),\s*(\w+)\s*$", r"\tslli \1, \2, 24\n\tsrai \1, \1, 24", text, flags=re.MULTILINE)
    text = re.sub(r"^\s*sext\.h\s+(\w+),\s*(\w+)\s*$", r"\tslli \1, \2, 16\n\tsrai \1, \1, 16", text, flags=re.MULTILINE)
    text = re.sub(r"^\s*tail\s+(\S+)\s*$", r"\tjal x0, \1", text, flags=re.MULTILINE)
    text = re.sub(r"^\s*call\s+(\S+)\s*$", r"\tjal ra, \1", text, flags=re.MULTILINE)
    text = re.sub(r"^\s*j\s+(\S+)\s*$", r"\tjal x0, \1", text, flags=re.MULTILINE)
    text = re.sub(r"^\s*jr\s+(\w+)\s*$", r"\tjalr x0, 0(\1)", text, flags=re.MULTILINE)

    # Numeric local labels (`1:` / `1b`) are file-local.
    text = re.sub(r"^\s*(\d+):", lambda m: f".L{prefix}_num{m.group(1)}:", text, flags=re.MULTILINE)
    text = re.sub(r"\b(\d+)b\b", lambda m: f".L{prefix}_num{m.group(1)}", text)
    text = re.sub(r"\b(\d+)f\b", lambda m: f".L{prefix}_num{m.group(1)}", text)

    # Rename file-local labels and every reference to them.
    def rename(match: re.Match[str]) -> str:
        return f".L{prefix}_{match.group(1)}"

    text = re.sub(r"\.L([A-Za-z0-9_.$]*)", rename, text)
    return text


def run(cmd: list[str], cwd: Path) -> None:
    result = subprocess.run(cmd, cwd=cwd, text=True, capture_output=True)
    if result.returncode != 0:
        sys.stderr.write(result.stdout)
        sys.stderr.write(result.stderr)
        raise SystemExit(f"command failed ({result.returncode}): {' '.join(cmd)}")


def main() -> None:
    BUILD.mkdir(parents=True, exist_ok=True)
    chunks: list[str] = ["\t.text\n"]
    for index, source in enumerate(sources()):
        stem = source.stem
        asm = BUILD / f"{stem}.s"
        run(["clang", *CFLAGS, "-S", str(source), "-o", str(asm)], cwd=GUEST)
        chunks.append(normalize(asm.read_text(), f"f{index}"))

    start = (GUEST / "src" / "start.S").read_text()
    start = start.replace("la gp, __global_pointer$", "li gp, 0")
    start = start.replace(".option push", "").replace(".option norelax", "").replace(".option pop", "")
    chunks.append(normalize(start, "start"))

    # Heap for the libc shim.
    chunks.append(
        """
\t.data
\t.globl _heap_start
_heap_start:
\t.zero 4194304
\t.globl _heap_end
_heap_end:
"""
    )

    combined = BUILD / "doom_all.s"
    combined.write_text("\n".join(chunks))
    print(f"combined: {combined} ({len(combined.read_text().splitlines())} lines)")

    result = subprocess.run([str(R2R), str(combined)], cwd=BUILD, text=True, capture_output=True)
    tas = BUILD / "doom_all.tas"
    if not tas.exists():
        sys.stderr.write(result.stdout)
        sys.stderr.write(result.stderr)
        raise SystemExit("R2R produced no .tas")
    if result.returncode != 0:
        print(f"note: R2R exited {result.returncode} after writing the .tas (assembler stage)")
    print(f"translated: {tas} ({len(tas.read_text().splitlines())} lines)")


if __name__ == "__main__":
    main()

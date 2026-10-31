#!/usr/bin/env python3
"""Build the doomgeneric guest for the REBEL-6 core.

Pipeline: clang -S (rv32i) -> text normalization -> single concatenated .s -> R2R -> .tas.

The R2R reference tool accepts one assembly file and has no linker, so every translation unit
is normalized and concatenated into one source before translation. clang writes assembly to
stdout, so no per-unit `.s` files touch the disk.

Scope handling replaces a linker: every symbol that clang does not declare `.globl` is local to
its translation unit, so it is renamed with the unit prefix. Without that pass, two file-scope
statics of the same name (`plr` in `am_map.c` and `hu_stuff.c`, `anims` in `wi_stuff.c` and
`p_spec.c`) share one address in the combined image.

Alongside `doom_all.s` the build writes two indexes:

* `doom_all.map`: combined line -> translation unit and original line.
* `doom_all.syms`: emitted symbol -> scope, translation unit and original line.
"""

from __future__ import annotations

import re
import subprocess
import sys
from dataclasses import dataclass, field
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
    "-gline-directives-only",
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

IDENT = re.compile(r"[A-Za-z_.$][\w.$]*")
GLOBALS = re.compile(r"^\s*\.(?:globl|global|weak)\s+(.+?)\s*$")
LOCALS = re.compile(r"^\s*\.local\s+(.+?)\s*$")
LABEL = re.compile(r"^([A-Za-z_.$][\w.$]*):")
COMM = re.compile(r"^\s*\.comm\s+([A-Za-z_.$][\w.$]*)\s*,\s*(\d+)(?:\s*,\s*\d+)?\s*$")
DROP = re.compile(
    r"^\s*\.(?:attribute|file|ident|option|type|size|addrsig|addrsig_sym|cfi_[a-z_]+|local|weak|hidden|protected|loc)\b"
)
DEAD_LABEL = re.compile(r"^\s*\.L(?:tmp|func_begin|func_end)\d+:\s*$")
FILE_DIRECTIVE = re.compile(r'^\s*\.file\s+(?:(\d+)\s+)?(.*)$')
LOC_DIRECTIVE = re.compile(r"^\s*\.loc\s+(\d+)\s+(\d+)")
HI_LUI = re.compile(r"^\s*lui\s+([a-z0-9]+),\s*%hi\(([^)]*)\)\s*$")
HI_AUIPC = re.compile(r"^\s*auipc\s+([a-z0-9]+),\s*%pcrel_hi\(([^)]*)\)\s*$")
LUI_IMM = re.compile(r"^\s*lui\s+([a-z0-9]+),\s*(0x[0-9a-fA-F]+|\d+)\s*$")
STRING = re.compile(r'^\s*\.string\s+"((?:[^"\\]|\\.)*)"\s*$')


def sources() -> list[Path]:
    engine = sorted(
        path
        for path in ENGINE.glob("*.c")
        if not path.name.startswith("doomgeneric_") and path.name not in EXCLUDED
    )
    port = [GUEST / "src" / name for name in ("libc.c", "platform.c", "stubs.c", "softfloat.c", "softmul.c")]
    return engine + port


def strip_comment(line: str) -> str:
    """Cuts a `#`, `;` or `//` comment, leaving string literals untouched."""
    in_quote = False
    i = 0
    while i < len(line):
        c = line[i]
        if c == '"' and (i == 0 or line[i - 1] != "\\"):
            in_quote = not in_quote
        if not in_quote:
            if c in "#;":
                return line[:i]
            if c == "/" and i + 1 < len(line) and line[i + 1] == "/":
                return line[:i]
        i += 1
    return line


def collect_symbols(text: str) -> tuple[dict[str, int], set[str], set[str]]:
    """Returns (defined symbol -> source line, global symbols, symbols defined by `.comm`)."""
    defined: dict[str, int] = {}
    globals_: set[str] = set()
    common: set[str] = set()
    for number, raw in enumerate(text.splitlines(), 1):
        line = strip_comment(raw)
        match = GLOBALS.match(line)
        if match:
            globals_.update(name.strip() for name in match.group(1).split(",") if name.strip())
        match = LOCALS.match(line)
        if match:
            # A `.local` declaration wins over a later `.globl`, as in GNU as.
            globals_.difference_update(name.strip() for name in match.group(1).split(",") if name.strip())
        match = LABEL.match(line)
        if match and not match.group(1).startswith(".L"):
            defined.setdefault(match.group(1), number)
        match = COMM.match(line)
        if match:
            defined.setdefault(match.group(1), number)
            common.add(match.group(1))
    return defined, globals_, common


def rename_tokens(line: str, mapping: dict[str, str]) -> str:
    """Rewrites whole identifier tokens, skipping string literals."""
    if not mapping:
        return line
    out: list[str] = []
    i = 0
    while i < len(line):
        c = line[i]
        if c == '"':
            j = i + 1
            while j < len(line):
                if line[j] == "\\":
                    j += 2
                    continue
                if line[j] == '"':
                    j += 1
                    break
                j += 1
            out.append(line[i:j])
            i = j
            continue
        match = IDENT.match(line, i)
        if match:
            token = match.group(0)
            out.append(mapping.get(token, token))
            i = match.end()
            continue
        out.append(c)
        i += 1
    return "".join(out)


def expand_string(body: str) -> str:
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


def expand(line: str) -> list[str]:
    """Rewrites one source line into zero or more output lines."""
    match = HI_LUI.match(line)
    if match:
        reg, expr = match.group(1), match.group(2)
        inner = re.fullmatch(r"([A-Za-z0-9_.$]+)([+-]\d+)?", expr.strip())
        if inner:
            symbol, offset = inner.group(1), inner.group(2)
            out = [f"\tli {reg}, {symbol}"]
            if offset:
                out.append(f"\taddi {reg}, {reg}, {offset}")
            return out

    match = HI_AUIPC.match(line)
    if match:
        reg, expr = match.group(1), match.group(2)
        inner = re.fullmatch(r"([A-Za-z0-9_.$]+)([+-]\d+)?", expr.strip())
        if inner:
            symbol, offset = inner.group(1), inner.group(2)
            out = [f"\tauipc {reg}, %pcrel_hi({symbol})"]
            if offset:
                out.append(f"\taddi {reg}, {reg}, {offset}")
            return out

    match = LUI_IMM.match(line)
    if match:
        return [f"\tli {match.group(1)}, {int(match.group(2), 0) << 12}"]

    line = re.sub(r"\.asciz\b", ".string", line)
    line = re.sub(r"\.ascii\b", ".string", line)
    match = STRING.match(line)
    if match:
        return [expand_string(match.group(1))]

    match = re.fullmatch(r"\s*zext\.b\s+(\w+),\s*(\w+)\s*", line)
    if match:
        return [f"\tandi {match.group(1)}, {match.group(2)}, 255"]
    match = re.fullmatch(r"\s*zext\.h\s+(\w+),\s*(\w+)\s*", line)
    if match:
        return [f"\tslli {match.group(1)}, {match.group(2)}, 16", f"\tsrli {match.group(1)}, {match.group(1)}, 16"]
    match = re.fullmatch(r"\s*sext\.b\s+(\w+),\s*(\w+)\s*", line)
    if match:
        return [f"\tslli {match.group(1)}, {match.group(2)}, 24", f"\tsrai {match.group(1)}, {match.group(1)}, 24"]
    match = re.fullmatch(r"\s*sext\.h\s+(\w+),\s*(\w+)\s*", line)
    if match:
        return [f"\tslli {match.group(1)}, {match.group(2)}, 16", f"\tsrai {match.group(1)}, {match.group(1)}, 16"]
    match = re.fullmatch(r"\s*tail\s+(\S+)\s*", line)
    if match:
        return [f"\tjal x0, {match.group(1)}"]
    match = re.fullmatch(r"\s*call\s+(\S+)\s*", line)
    if match:
        return [f"\tjal ra, {match.group(1)}"]
    match = re.fullmatch(r"\s*j\s+(\S+)\s*", line)
    if match:
        return [f"\tjal x0, {match.group(1)}"]
    match = re.fullmatch(r"\s*jr\s+(\w+)\s*", line)
    if match:
        return [f"\tjalr x0, 0({match.group(1)})"]

    return [line]


def rewrite(line: str, prefix: str) -> str:
    """Applies the one-to-one text rewrites that keep the line count."""
    line = re.sub(r"%lo\([^)]*\)\(([A-Za-z0-9_]+)\)", r"0(\1)", line)
    line = re.sub(r",\s*%lo\([^)]*\)", ", 0", line)
    line = re.sub(r"%pcrel_lo\([^)]*\)", "0", line)

    def section(match: re.Match[str]) -> str:
        name = match.group(1)
        if ".text" in name:
            return ".section .text"
        if ".bss" in name:
            return ".section .bss"
        if ".rodata" in name:
            return ".section .rodata"
        return ".data"

    line = re.sub(r"\.section\s+([^,\n]+).*", section, line)

    # File-local labels: clang's `.L*` names and assembler numeric labels.
    line = re.sub(r"\.L([A-Za-z0-9_.$]*)", lambda m: f".L{prefix}_{m.group(1)}", line)
    line = re.sub(r"^\s*(\d+):", lambda m: f".L{prefix}_num{m.group(1)}:", line)
    line = re.sub(r"\b(\d+)b\b", lambda m: f".L{prefix}_num{m.group(1)}", line)
    line = re.sub(r"\b(\d+)f\b", lambda m: f".L{prefix}_num{m.group(1)}", line)
    return line


@dataclass
class Unit:
    """One translation unit after normalization."""

    path: Path | None
    lines: list[str] = field(default_factory=list)
    source: list[int] = field(default_factory=list)
    c_line: list[int] = field(default_factory=list)
    c_file: list[str] = field(default_factory=list)
    defined: dict[str, int] = field(default_factory=dict)
    globals: set[str] = field(default_factory=set)
    rename: dict[str, str] = field(default_factory=dict)
    common: set[str] = field(default_factory=set)
    label_loc: dict[int, tuple[str, int]] = field(default_factory=dict)


def debug_locations(text: str) -> tuple[list[tuple[str, int]], list[tuple[int, tuple[str, int]]]]:
    """Returns, per assembly line, the C file and line in effect.

    clang emits `.loc <file> <line>` with `-gline-directives-only`; the directives are dropped
    from the output but give every instruction its origin. An empty file name means the
    translation unit itself.
    """
    files: dict[int, str] = {}
    current = ("", 0)
    out: list[tuple[str, int]] = []
    positions: list[tuple[int, tuple[str, int]]] = []
    for number, raw in enumerate(text.splitlines()):
        match = FILE_DIRECTIVE.match(raw)
        if match:
            index = int(match.group(1)) if match.group(1) else 0
            quoted = re.findall(r'"((?:[^"\\]|\\.)*)"', match.group(2))
            if quoted:
                files[index] = quoted[-1]
        match = LOC_DIRECTIVE.match(raw)
        if match:
            index, line = int(match.group(1)), int(match.group(2))
            current = ("" if index in (0, 1) else files.get(index, ""), line)
            positions.append((number, current))
        out.append(current)
    return out, positions


def normalize(text: str, prefix: str, path: Path | None) -> Unit:
    defined, globals_, common = collect_symbols(text)
    rename = {
        name: f".L{prefix}_{name}"
        for name in defined
        if name not in globals_ and not name.startswith(".L")
    }
    locations, positions = debug_locations(text)
    label_loc: dict[int, tuple[str, int]] = {}
    for name, asm_line in defined.items():
        if name in common:
            continue
        index = asm_line - 1
        for position, location in positions:
            if position >= index:
                if position - index <= 8:
                    label_loc[asm_line] = location
                break

    unit = Unit(
        path=path,
        defined=defined,
        globals=globals_,
        rename=rename,
        common=common,
        label_loc=label_loc,
    )
    for number, raw in enumerate(text.splitlines(), 1):
        line = strip_comment(raw)
        if DROP.match(line) or DEAD_LABEL.match(line):
            continue
        c_file, c_line = locations[number - 1]
        is_common = bool(COMM.match(line))
        if is_common:
            # `.comm` sits after the last function, so its `.loc` is stale.
            c_file, c_line = "", 0
        for out in expand(line):
            out = rewrite(out, prefix)
            out = rename_tokens(out, rename)
            unit.lines.append(out)
            unit.source.append(number)
            unit.c_line.append(c_line or number)
            unit.c_file.append(c_file)
    return unit


def compile_assembly(source: Path) -> str:
    """Compiles one C file to assembly on stdout, no intermediate file."""
    result = subprocess.run(
        ["clang", *CFLAGS, "-S", str(source), "-o", "-"],
        cwd=GUEST,
        text=True,
        capture_output=True,
    )
    if result.returncode != 0:
        sys.stderr.write(result.stdout)
        sys.stderr.write(result.stderr)
        raise SystemExit(f"clang failed ({result.returncode}): {source}")
    return result.stdout


def write_indexes(units: list[Unit], map_path: Path, syms_path: Path) -> dict[str, list[tuple[int, int, str]]]:
    """Writes the line map and the symbol table.

    Returns symbol -> [(unit, line, file)], where an empty file means the unit's own source.
    """
    origin: dict[str, list[tuple[int, int, str]]] = {}
    map_rows = ["# combined\tunit\tline\tfile"]
    sym_rows = ["# emitted\tscope\tunit\tline\tfile\toriginal"]
    for index, unit in enumerate(units):
        label = str(unit.path) if unit.path else "<generated>"
        map_rows.append(f"# unit {index}\t{label}")
        located = dict(zip(unit.source, zip(unit.c_line, unit.c_file)))
        for name, asm_line in unit.defined.items():
            emitted = unit.rename.get(name, name)
            scope = "global" if name in unit.globals else "local"
            c_line, c_file = located.get(asm_line, (asm_line, ""))
            if name in unit.common:
                c_line, c_file = 0, ""
            elif asm_line in unit.label_loc:
                c_file, c_line = unit.label_loc[asm_line]
            origin.setdefault(name, []).append((index, c_line, c_file))
            origin.setdefault(emitted, []).append((index, c_line, c_file))
            sym_rows.append(f"{emitted}\t{scope}\t{index}\t{c_line}\t{c_file}\t{name}")
        for name in sorted(unit.globals - set(unit.defined)):
            sym_rows.append(f"{name}\tglobal\t{index}\t0\t\t{name}")

    combined_line = 0
    for index, unit in enumerate(units):
        for c_line, c_file in zip(unit.c_line, unit.c_file):
            combined_line += 1
            map_rows.append(f"{combined_line}\t{index}\t{c_line}\t{c_file}")

    map_path.write_text("\n".join(map_rows) + "\n")
    syms_path.write_text("\n".join(sym_rows) + "\n")
    return origin


def explain(output: str, origin: dict[str, list[tuple[int, int, str]]], units: list[Unit]) -> None:
    """Maps symbol names in an R2R failure message back to their translation unit."""
    patterns = (
        r"Failed to resolve label (\S+)",
        r"Failed to load symbol address of symbol (\S+)",
        r"Failed to parse (\S+)",
    )
    seen: set[str] = set()
    for pattern in patterns:
        for symbol in re.findall(pattern, output):
            symbol = symbol.strip("\"',;")
            if symbol in seen or symbol not in origin:
                continue
            seen.add(symbol)
            for unit_index, line, header in origin[symbol]:
                path = units[unit_index].path
                where = f"{path}:{line}" if not header else f"{path}:{line} ({header})"
                print(f"note: {symbol} is defined at {where}", file=sys.stderr)


def main() -> None:
    BUILD.mkdir(parents=True, exist_ok=True)

    units: list[Unit] = [Unit(path=None, lines=["\t.text"], source=[0], c_line=[0], c_file=[""])]
    for index, source in enumerate(sources()):
        units.append(normalize(compile_assembly(source), f"f{index}", source))

    start_path = GUEST / "src" / "start.S"
    start = start_path.read_text()
    start = start.replace("la gp, __global_pointer$", "li gp, 0")
    start = start.replace(".option push", "").replace(".option norelax", "").replace(".option pop", "")
    units.append(normalize(start, "start", start_path))

    # Heap for the libc shim.
    heap = [
        "",
        "\t.data",
        "\t.globl _heap_start",
        "_heap_start:",
        "\t.zero 4194304",
        "\t.globl _heap_end",
        "_heap_end:",
    ]
    units.append(Unit(path=None, lines=heap, source=[0] * len(heap), c_line=[0] * len(heap), c_file=[""] * len(heap)))

    combined: list[str] = []
    for unit in units:
        combined.extend(unit.lines)

    defined_at: dict[str, tuple[int, int]] = {}
    duplicates: list[str] = []
    line_number = 0
    for unit_index, unit in enumerate(units):
        for offset, line in enumerate(unit.lines):
            line_number += 1
            match = LABEL.match(line)
            if not match:
                continue
            name = match.group(1)
            if name in defined_at:
                first_unit, first_offset = defined_at[name]
                duplicates.append(
                    f"{name}: {units[first_unit].path} line {units[first_unit].c_line[first_offset]} and "
                    f"{units[unit_index].path} line {unit.c_line[offset]} (combined line {line_number})"
                )
            else:
                defined_at[name] = (unit_index, offset)
    if duplicates:
        for duplicate in duplicates:
            print(f"error: duplicate label {duplicate}", file=sys.stderr)
        raise SystemExit("duplicate labels in the combined assembly")

    combined_path = BUILD / "doom_all.s"
    combined_path.write_text("\n".join(combined))
    origin = write_indexes(units, BUILD / "doom_all.map", BUILD / "doom_all.syms")

    renamed = sum(len(unit.rename) for unit in units)
    owners: dict[str, list[int]] = {}
    for index, unit in enumerate(units):
        for name in unit.defined:
            owners.setdefault(name, []).append(index)
    collisions = {name: index for name, index in owners.items() if len(index) > 1}
    print(f"combined: {combined_path} ({len(combined)} lines)")
    print(f"scope: {renamed} local symbols renamed, {len(collisions)} duplicate definitions avoided")
    for name, index in sorted(collisions.items()):
        units_of = ", ".join(Path(str(units[i].path)).name if units[i].path else "<generated>" for i in index)
        print(f"       {name}: {units_of}")

    result = subprocess.run([str(R2R), str(combined_path)], cwd=BUILD, text=True, capture_output=True)
    tas = BUILD / "doom_all.tas"
    if not tas.exists():
        explain(result.stdout + result.stderr, origin, units)
        sys.stderr.write(result.stdout)
        sys.stderr.write(result.stderr)
        raise SystemExit("R2R produced no .tas")
    if result.returncode != 0:
        print(f"note: R2R exited {result.returncode} after writing the .tas (assembler stage)")
        explain(result.stdout + result.stderr, origin, units)
    print(f"translated: {tas} ({len(tas.read_text().splitlines())} lines)")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Build the doomgeneric guest for the REBEL-6 core.

Pipeline: clang -S (rv32i) -> text normalization -> one `.s` per unit -> `R2R --link` -> `.tas`.

Each translation unit is normalized into `build/rebel6/units/<stem>.s` and handed to the R2R
linker, which scopes every symbol the unit does not declare `.globl` with the unit prefix and
merges the units into one image. Two file-scope statics of the same name (`plr` in `am_map.c` and
`hu_stuff.c`, `anims` in `wi_stuff.c` and `p_spec.c`) therefore keep separate addresses without a
Python renaming pass.

Alongside the image the build writes two indexes:

* `doom_all.map`: unit -> its `.s` file, and per line the C file and line in effect.
* `doom_all.syms`: emitted symbol -> scope, unit, original line and original name.
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

    # zext, sext, tail, call, j and jr are translated by R2R.

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

    # Assembler numeric labels become named labels; `.L*` names are scoped by the linker.
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


def write_indexes(
    units: list[Unit], names: list[str], map_path: Path, syms_path: Path
) -> dict[str, list[tuple[int, int, str]]]:
    """Writes the per-unit line map and the symbol table.

    Returns symbol -> [(unit, line, file)], where an empty file means the unit's own source.
    """
    origin: dict[str, list[tuple[int, int, str]]] = {}
    map_rows = ["# unit\tlinker name\tsource\tassembly"]
    sym_rows = ["# emitted\tscope\tunit\tline\tfile\toriginal"]
    for index, unit in enumerate(units):
        label = str(unit.path) if unit.path else "<generated>"
        map_rows.append(f"# unit {index}\t{names[index]}\t{label}\tunits/{names[index]}.s")
        map_rows.append("# line\tc_line\tc_file")
        located = dict(zip(unit.source, zip(unit.c_line, unit.c_file)))
        for name, asm_line in unit.defined.items():
            scope = "global" if name in unit.globals else "local"
            emitted = name if scope == "global" else f".{names[index]}_{name}"
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
        for line_number, (c_line, c_file) in enumerate(zip(unit.c_line, unit.c_file), 1):
            map_rows.append(f"{line_number}\t{c_line}\t{c_file}")

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

    unit_dir = BUILD / "units"
    unit_dir.mkdir(parents=True, exist_ok=True)
    for stale in unit_dir.glob("*.s"):
        stale.unlink()
    legacy = BUILD / "doom_all.s"
    if legacy.exists():
        legacy.unlink()

    names: list[str] = []
    unit_paths: list[Path] = []
    seen: dict[str, int] = {}
    for index, unit in enumerate(units):
        stem = unit.path.stem if unit.path else ("text" if index == 0 else "heap")
        count = seen.get(stem, 0)
        seen[stem] = count + 1
        name = stem if count == 0 else f"{stem}_{count}"
        names.append(name)
        path = unit_dir / f"{name}.s"
        path.write_text("\n".join(unit.lines) + "\n")
        unit_paths.append(path)

    origin = write_indexes(units, names, BUILD / "doom_all.map", BUILD / "doom_all.syms")

    owners: dict[str, list[int]] = {}
    for index, unit in enumerate(units):
        for name in unit.defined:
            owners.setdefault(name, []).append(index)
    collisions = {name: index for name, index in owners.items() if len(index) > 1}
    print(f"units: {len(unit_paths)} files in {unit_dir} ({sum(len(unit.lines) for unit in units)} lines)")
    print(f"scope: {len(collisions)} same-named definitions resolved by the linker")
    for name, index in sorted(collisions.items()):
        units_of = ", ".join(Path(str(units[i].path)).name if units[i].path else "<generated>" for i in index)
        print(f"       {name}: {units_of}")

    result = subprocess.run(
        [str(R2R), "--link", "-o", "doom_all", *[str(path) for path in unit_paths]],
        cwd=BUILD,
        text=True,
        capture_output=True,
    )
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

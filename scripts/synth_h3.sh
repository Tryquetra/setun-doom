#!/usr/bin/env bash
# H3: word-level equality must be cheaper in the canonical code.
# Synthesizes 24-trit equality in both encodings for GW2A-18C and prints the
# cell counts taken from the synthesis JSON.
set -euo pipefail
cd "$(dirname "$0")/.."

BUILD="fpga/build/h3"
mkdir -p "$BUILD"

printf '%-9s %-26s %6s %6s %6s %6s\n' op code LUT4 LUT1 IBUF OBUF
for spec in \
    "word_eq trit_word_eq_canonical" \
    "word_eq trit_word_eq_redundant"; do
    set -- $spec
    op=$1
    top=$2
    yosys -q -p "read_verilog fpga/gowin/trit_word_eq.v; synth_gowin -top $top -json $BUILD/${top}.json" \
        > "$BUILD/${top}.yosys.log" 2>&1
    read -r lut4 lut1 ibuf obuf < <(
        python3 - "$BUILD/${top}.json" "$top" <<'PY'
import json, sys
with open(sys.argv[1]) as f:
    data = json.load(f)
cells = data["modules"][sys.argv[2]]["cells"].values()
counts = {}
for c in cells:
    counts[c["type"]] = counts.get(c["type"], 0) + 1
print(counts.get("LUT4", 0), counts.get("LUT1", 0),
      counts.get("IBUF", 0), counts.get("OBUF", 0))
PY
    )
    printf '%-9s %-26s %6s %6s %6s %6s\n' "$op" "$top" "$lut4" "$lut1" "$ibuf" "$obuf"
done

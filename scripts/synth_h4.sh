#!/usr/bin/env bash
# H4: dense packing must reduce bank memory at acceptable unpacking cost.
# Synthesizes the 48/40/39-bit banks and the two packers for GW2A-18C and
# prints the cell counts taken from the synthesis JSON.
set -euo pipefail
cd "$(dirname "$0")/.."

BUILD="fpga/build/h4"
mkdir -p "$BUILD"

printf '%-14s %-24s %6s %6s %6s %6s\n' op code LUT4 LUT1 DFF DPX9B
for spec in \
    "bank trit_bank_w48" \
    "bank trit_bank_w40" \
    "bank trit_bank_w39" \
    "pack pack_word40" \
    "pack pack_word39"; do
    set -- $spec
    op=$1
    top=$2
    yosys -q -p "read_verilog fpga/gowin/trit_pack.v; synth_gowin -top $top -json $BUILD/${top}.json" \
        > "$BUILD/${top}.yosys.log" 2>&1
    read -r lut4 lut1 dff dp < <(
        python3 - "$BUILD/${top}.json" "$top" <<'PY'
import json, sys
with open(sys.argv[1]) as f:
    data = json.load(f)
cells = data["modules"][sys.argv[2]]["cells"].values()
counts = {}
for c in cells:
    counts[c["type"]] = counts.get(c["type"], 0) + 1
print(counts.get("LUT4", 0), counts.get("LUT1", 0),
      counts.get("DFF", 0), counts.get("DPX9B", 0))
PY
    )
    printf '%-14s %-24s %6s %6s %6s %6s\n' "$op" "$top" "$lut4" "$lut1" "$dff" "$dp"
done

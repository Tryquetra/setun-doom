#!/usr/bin/env bash
# H5: register bank fits in BRAM, not in flip-flops.
# Synthesizes the dual-bank and single-bank variants for GW2A-18C and prints
# the full cell-type counts from the synthesis JSON.
set -euo pipefail
cd "$(dirname "$0")/.."

BUILD="fpga/build/h5"
mkdir -p "$BUILD"

for top in trit_regfile_dualbank trit_regfile_singlebank; do
    yosys -q -p "read_verilog fpga/gowin/trit_regfile.v; synth_gowin -top $top -json $BUILD/${top}.json" \
        > "$BUILD/${top}.yosys.log" 2>&1
    echo "== $top =="
    python3 - "$BUILD/${top}.json" "$top" <<'PY'
import json, sys
with open(sys.argv[1]) as f:
    data = json.load(f)
cells = data["modules"][sys.argv[2]]["cells"].values()
counts = {}
for c in cells:
    counts[c["type"]] = counts.get(c["type"], 0) + 1
for k in sorted(counts):
    print(f"  {k}: {counts[k]}")
PY
done

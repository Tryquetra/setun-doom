#!/usr/bin/env bash
# H1: canonical code must cost less logic than redundant code.
# Synthesizes min, max, sum mod 3, equality and negation over 24 trits in both
# encodings for GW2A-18C and prints the cell counts taken from the synthesis
# JSON, grouped by cell type.
set -euo pipefail
cd "$(dirname "$0")/.."

BUILD="fpga/build/h1"
mkdir -p "$BUILD"

printf '%-9s %-22s %6s %6s %6s %6s\n' op code LUT4 LUT1 IBUF OBUF
for spec in \
    "min trit_min_canonical" "min trit_min_redundant" \
    "max trit_max_canonical" "max trit_max_redundant" \
    "sum3 trit_sum3_canonical" "sum3 trit_sum3_redundant" \
    "eq trit_eq_canonical" "eq trit_eq_redundant" \
    "neg trit_neg_canonical" "neg trit_neg_redundant"; do
    set -- $spec
    op=$1
    top=$2
    yosys -q -p "read_verilog fpga/gowin/trit_ops.v fpga/gowin/trit_neg.v; synth_gowin -top $top -json $BUILD/${top}.json" \
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
    printf '%-9s %-22s %6s %6s %6s %6s\n' "$op" "$top" "$lut4" "$lut1" "$ibuf" "$obuf"
done

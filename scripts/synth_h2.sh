#!/usr/bin/env bash
# H2: negation must cost zero LUT in both encodings.
# Synthesizes fpga/gowin/trit_neg.v for the Tang Nano 20K (GW2A-18C) and prints
# the cell counts from yosys and the netlist content. The standalone negator has
# 96 top-level IO, so placement needs a pin map (CST); H2 measures synthesis,
# and PnR is only run when CST is provided via NEXTPNR_CST.
set -euo pipefail
cd "$(dirname "$0")/.."

DEVICE="${DEVICE:-GW2A-LV18QN88C8/I7}"
FAMILY="${FAMILY:-GW2A-18C}"
NEXTPNR="${NEXTPNR:-/tmp/nextpnr-src/build/nextpnr-himbaechel}"
BUILD="fpga/build/h2"
mkdir -p "$BUILD"

for v in canonical redundant; do
    echo "== $v =="
    yosys -p "read_verilog fpga/gowin/trit_neg.v; synth_gowin -top trit_neg_${v} -json $BUILD/${v}.json; write_verilog $BUILD/${v}_netlist.v" \
        > "$BUILD/${v}.yosys.log" 2>&1
    grep -E 'Number of cells|LUT4|DFF|IOB|_\\$_' "$BUILD/${v}.yosys.log" | tail -10 || true
    echo "LUT4 lines in netlist: $(grep -c 'LUT4' "$BUILD/${v}_netlist.v" || true)"
    if [ -n "${NEXTPNR_CST:-}" ]; then
        "$NEXTPNR" --device "$DEVICE" --json "$BUILD/${v}.json" \
            --write "$BUILD/${v}_pnr.json" -o family="$FAMILY" -o cst="$NEXTPNR_CST" \
            > "$BUILD/${v}.nextpnr.log" 2>&1
        grep -E 'Device utilisation|LUT4|IOB|Program finished' "$BUILD/${v}.nextpnr.log" | tail -6 || true
    fi
done

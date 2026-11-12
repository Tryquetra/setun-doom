#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

TAS_PATH="${1:-/home/robson/coding/setun-doom-rebel6/vendor/doomgeneric/riscv32/build/rebel6/doom_all.tas}"
WAD_PATH="${2:-vendor/DOOM1.WAD}"
OUT_DIR="fpga/build/soc"
mkdir -p "$OUT_DIR"

echo "[1/4] Packing compact R6BT boot image..."
cargo run --release --example pack_fpga_boot -- "$TAS_PATH" "fpga/build/doom_boot.bin" "$WAD_PATH"

echo "[2/4] Synthesizing REBEL-6 Doom SoC with Yosys (GW2A)..."
yosys -q -p "
read_verilog \
  fpga/gowin/rebel6_decoder.v \
  fpga/gowin/rebel6_alu.v \
  fpga/gowin/rebel6_regfile.v \
  fpga/gowin/rebel6_trit_unit.v \
  fpga/gowin/rebel6_core.v \
  fpga/gowin/rebel6_sdram32.v \
  fpga/gowin/rebel6_icache.v \
  fpga/gowin/rebel6_dcache.v \
  fpga/gowin/rebel6_periph.v \
  fpga/gowin/rebel6_hdmi.v \
  fpga/gowin/rebel6_soc_host.v \
  fpga/gowin/rebel6_soc_top.v;
synth_gowin -top rebel6_soc_top -family gw2a -json $OUT_DIR/rebel6_soc_top.json
"

echo "[3/4] Placing and routing with nextpnr-himbaechel (GW2AR-LV18QN88C8/I7)..."
NEXTPNR="${NEXTPNR:-$HOME/.local/bin/nextpnr-himbaechel}"
"$NEXTPNR" \
  --json "$OUT_DIR/rebel6_soc_top.json" \
  --write "$OUT_DIR/rebel6_soc_top_pnr.json" \
  --device "GW2AR-LV18QN88C8/I7" \
  --vopt family=GW2A-18C \
  --vopt cst=fpga/gowin/tangnano20k.cst \
  > "$OUT_DIR/nextpnr.log" 2>&1

echo "[4/4] Packing compressed Gowin bitstream with MSPI enabled as user GPIO..."
GOWIN_PACK="${GOWIN_PACK:-$HOME/.local/bin/gowin_pack}"
"$GOWIN_PACK" \
  -d GW2A-18C \
  -c \
  --mspi_as_gpio \
  -o "$OUT_DIR/rebel6_doom_soc.fs" \
  "$OUT_DIR/rebel6_soc_top_pnr.json"

ls -lh "$OUT_DIR/rebel6_doom_soc.fs" "fpga/build/doom_boot.bin"

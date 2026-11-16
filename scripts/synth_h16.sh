#!/usr/bin/env bash
# H16: Hardware verification, dynamic wire switching telemetry, yosys synthesis,
# and nextpnr-himbaechel Fmax (GW2AR-LV18QN88C8/I7) across N=8 hexagonal cells.
set -euo pipefail
cd "$(dirname "$0")/.."

BUILD="fpga/build/h16"
rm -rf "$BUILD"
mkdir -p "$BUILD/tb"

RTL="fpga/gowin/trit_hex_gray_h16.v"
NEXTPNR="${NEXTPNR:-${HOME}/.local/bin/nextpnr-himbaechel}"

cat > "$BUILD/tb/tb_h16.v" <<'V'
`timescale 1ns/1ps
module tb_h16;
    reg clk = 0;
    reg rst = 1;
    reg [15:0] cmd = 0;

    wire [47:0] st_can;
    wire [47:0] st_off;
    wire [71:0] st_oh9;
    wire [55:0] st_oh7;
    wire [23:0] st_hg3;

    hg_canon6b_n8   u_can (.clk(clk), .rst(rst), .cmd(cmd), .state(st_can));
    hg_offset6b_n8  u_off (.clk(clk), .rst(rst), .cmd(cmd), .state(st_off));
    hg_onehot9b_n8  u_oh9 (.clk(clk), .rst(rst), .cmd(cmd), .state(st_oh9));
    hg_onehot7b_n8  u_oh7 (.clk(clk), .rst(rst), .cmd(cmd), .state(st_oh7));
    hg_hexgray3b_n8 u_hg3 (.clk(clk), .rst(rst), .cmd(cmd), .state(st_hg3));

    always #5 clk = ~clk;

    reg [47:0] prev_can, prev_off;
    reg [71:0] prev_oh9;
    reg [55:0] prev_oh7;
    reg [23:0] prev_hg3;

    integer flips_can = 0, flips_off = 0, flips_oh9 = 0, flips_oh7 = 0, flips_hg3 = 0;
    integer glitch_can = 0, glitch_off = 0, glitch_oh9 = 0, glitch_oh7 = 0, glitch_hg3 = 0;

    function integer popcount72;
        input [71:0] x;
        integer k, c;
        begin
            c = 0;
            for (k = 0; k < 72; k = k + 1) c = c + x[k];
            popcount72 = c;
        end
    endfunction

    integer cyc, lane, d_c, d_o, d_9, d_7, d_g;
    reg [31:0] rng;
    reg [1:0] c_in;

    initial begin
        rng = 32'hCAFE_BABE;
        @(posedge clk); #1;
        rst = 0;
        prev_can = st_can;
        prev_off = st_off;
        prev_oh9 = st_oh9;
        prev_oh7 = st_oh7;
        prev_hg3 = st_hg3;

        // Phase 1: 6000 cycles of Doom-like cyclic sweep + occasional center reset
        for (cyc = 0; cyc < 6000; cyc = cyc + 1) begin
            for (lane = 0; lane < 8; lane = lane + 1) begin
                if (((cyc + lane) % 61) == 60)       c_in = 2'b11; // return to center
                else if (((cyc / 12) % 2) == 0)      c_in = 2'b01; // CCW sweep
                else                                 c_in = 2'b10; // CW sweep
                cmd[2*lane +: 2] = c_in;
            end
            @(posedge clk); #1;

            d_c = popcount72({24'b0, prev_can ^ st_can});
            d_o = popcount72({24'b0, prev_off ^ st_off});
            d_9 = popcount72(prev_oh9 ^ st_oh9);
            d_7 = popcount72({16'b0, prev_oh7 ^ st_oh7});
            d_g = popcount72({48'b0, prev_hg3 ^ st_hg3});

            flips_can = flips_can + d_c;
            flips_off = flips_off + d_o;
            flips_oh9 = flips_oh9 + d_9;
            flips_oh7 = flips_oh7 + d_7;
            flips_hg3 = flips_hg3 + d_g;

            for (lane = 0; lane < 8; lane = lane + 1) begin
                if (popcount72({66'b0, prev_can[6*lane+:6] ^ st_can[6*lane+:6]}) > 1) glitch_can = glitch_can + 1;
                if (popcount72({66'b0, prev_off[6*lane+:6] ^ st_off[6*lane+:6]}) > 1) glitch_off = glitch_off + 1;
                if (popcount72({63'b0, prev_oh9[9*lane+:9] ^ st_oh9[9*lane+:9]}) > 1) glitch_oh9 = glitch_oh9 + 1;
                if (popcount72({65'b0, prev_oh7[7*lane+:7] ^ st_oh7[7*lane+:7]}) > 1) glitch_oh7 = glitch_oh7 + 1;
                if (popcount72({69'b0, prev_hg3[3*lane+:3] ^ st_hg3[3*lane+:3]}) > 1) glitch_hg3 = glitch_hg3 + 1;
            end

            // Print compact verification trace on first 64 cycles
            if (cyc < 64) begin
                $display("T %0d %012h %012h %018h %014h %06h", cyc, st_can, st_off, st_oh9, st_oh7, st_hg3);
            end

            prev_can = st_can;
            prev_off = st_off;
            prev_oh9 = st_oh9;
            prev_oh7 = st_oh7;
            prev_hg3 = st_hg3;
        end

        $display("SWEEP_METRICS can=%0d(g=%0d) off=%0d(g=%0d) oh9=%0d(g=%0d) oh7=%0d(g=%0d) hg3=%0d(g=%0d)",
            flips_can, glitch_can, flips_off, glitch_off, flips_oh9, glitch_oh9,
            flips_oh7, glitch_oh7, flips_hg3, glitch_hg3);
        $finish;
    end
endmodule
V

iverilog -g2012 -o "$BUILD/tb/sim_h16" "$RTL" "$BUILD/tb/tb_h16.v"
vvp "$BUILD/tb/sim_h16" > "$BUILD/sim.log"
python3 - "$BUILD/sim.log" <<'PY'
import sys

CAN = {0b000: 0b000000, 0b001: 0b100001, 0b011: 0b100100, 0b010: 0b000110,
       0b110: 0b010010, 0b111: 0b011000, 0b101: 0b001001}
OFF = {0b000: 0b010101, 0b001: 0b000110, 0b011: 0b001001, 0b010: 0b011000,
       0b110: 0b100100, 0b111: 0b100001, 0b101: 0b010010}
OH9 = {0b000: 0b001001001, 0b001: 0b100001010, 0b011: 0b100010001, 0b010: 0b001010100,
       0b110: 0b010001100, 0b111: 0b010100001, 0b101: 0b001100010}
OH7 = {0b000: 1<<0, 0b001: 1<<1, 0b011: 1<<2, 0b010: 1<<3, 0b110: 1<<4, 0b111: 1<<5, 0b101: 1<<6}

checked = 0
with open(sys.argv[1]) as f:
    for line in f:
        line = line.strip()
        if line.startswith("T "):
            _, cyc, sc, so, s9, s7, sg = line.split()
            vc, vo, v9, v7, vg = int(sc, 16), int(so, 16), int(s9, 16), int(s7, 16), int(sg, 16)
            for lane in range(8):
                g = (vg >> (3*lane)) & 0x7
                assert ((vc >> (6*lane)) & 0x3F) == CAN[g], f"canon mismatch cyc={cyc} lane={lane}"
                assert ((vo >> (6*lane)) & 0x3F) == OFF[g], f"offset mismatch cyc={cyc} lane={lane}"
                assert ((v9 >> (9*lane)) & 0x1FF) == OH9[g], f"oh9 mismatch cyc={cyc} lane={lane}"
                assert ((v7 >> (7*lane)) & 0x7F) == OH7[g], f"oh7 mismatch cyc={cyc} lane={lane}"
            checked += 1
        elif line.startswith("SWEEP_METRICS"):
            print(line)
assert checked == 64, f"expected 64 checked traces, got {checked}"
print(f"VERIFIED {checked} multi-lane equivalence frames (all 5 codings identical)")
PY

cat > "$BUILD/h16_pnr.cst" <<'CST'
IO_LOC  "I_clk" 4;
IO_PORT "I_clk" IO_TYPE=LVCMOS33 PULL_MODE=UP;
IO_LOC  "I_btn[0]" 88;
IO_PORT "I_btn[0]" IO_TYPE=LVCMOS33 PULL_MODE=DOWN;
IO_LOC  "I_btn[1]" 87;
IO_PORT "I_btn[1]" IO_TYPE=LVCMOS33 PULL_MODE=DOWN;
IO_LOC  "O_led_n[0]" 15;
IO_PORT "O_led_n[0]" IO_TYPE=LVCMOS33 PULL_MODE=UP DRIVE=8;
CST

echo ""
echo "variant          Wires  LUT4  LUT1  MUX2   DFF  Fmax(MHz)"
echo "----------------------------------------------------------"
for top_w in \
    "hg_canon6b_n8 48" \
    "hg_offset6b_n8 48" \
    "hg_onehot9b_n8 72" \
    "hg_onehot7b_n8 56" \
    "hg_hexgray3b_n8 24"; do
    set -- $top_w
    top=$1; w=$2
    yosys -q -p "read_verilog $RTL; synth_gowin -top $top -family gw2a -noiopads -json $BUILD/${top}.json"
    yosys -q -p "read_verilog $RTL; synth_gowin -top pnr_${top} -family gw2a -json $BUILD/pnr_${top}.json"
    "$NEXTPNR" \
        --json "$BUILD/pnr_${top}.json" \
        --device "GW2AR-LV18QN88C8/I7" \
        --vopt family=GW2A-18C \
        --vopt cst="$BUILD/h16_pnr.cst" \
        --freq 27 \
        >"$BUILD/${top}_pnr.log" 2>&1 || true
    python3 - "$BUILD/${top}.json" "$BUILD/${top}_pnr.log" "$top" "$w" <<'PY'
import json, re, sys
with open(sys.argv[1]) as f:
    data = json.load(f)
cells = data["modules"][sys.argv[3]]["cells"].values()
counts = {}
for c in cells:
    t = c["type"]
    counts[t] = counts.get(t, 0) + 1
lut4 = counts.get("LUT4", 0)
lut1 = counts.get("LUT1", 0)
mux2 = counts.get("MUX2_LUT5", 0)
dff = sum(v for k, v in counts.items() if k.startswith("DFF"))
with open(sys.argv[2]) as f:
    log = f.read()
m = re.findall(r"Max frequency for clock[^:]*:\s*([0-9.]+)\s*MHz", log)
fmax = m[-1] if m else "N/A"
print(f"{sys.argv[3]:<15} {int(sys.argv[4]):>5} {lut4:>5} {lut1:>5} {mux2:>5} {dff:>5} {fmax:>10}")
PY
done

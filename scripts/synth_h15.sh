#!/usr/bin/env bash
# H15: full datapath cost per trit coding on GW2A-18C.
# Four variants (canonical, offset, one-hot bridge, one-hot full storage) are
# verified by an iverilog testbench (2000 random cycles) whose trace is
# checked by a Python model, and synthesized at N=24; cell counts come from
# the synthesis JSON.
set -euo pipefail
cd "$(dirname "$0")/.."

BUILD="fpga/build/h15"
rm -rf "$BUILD"
mkdir -p "$BUILD"

count_cells() {
    local json=$1 top=$2
    python3 - "$json" "$top" <<'PY'
import json, sys
with open(sys.argv[1]) as f:
    data = json.load(f)
cells = data["modules"][sys.argv[2]]["cells"].values()
counts = {}
for c in cells:
    counts[c["type"]] = counts.get(c["type"], 0) + 1
bram = counts.get("DPX9B", 0) + counts.get("DPB", 0) + counts.get("SDPX9B", 0)
print(counts.get("LUT4", 0), counts.get("LUT1", 0), bram)
PY
}

check_trace() {
    # $1 = mode: 0 canonical, 1 offset, 2 onehot bridge, 3 onehot full
    python3 - "$1" <<'PY'
import sys

mode = int(sys.argv[1])
N = 2

def bal(s):
    d = ((s + 1) % 3 + 3) % 3 - 1
    c = (s - d) // 3
    return d, c

def add_words(a, b):
    out = [0] * N
    carry = 0
    for i in range(N):
        s = (a[i] - 1) + (b[i] - 1) + carry
        d, carry = bal(s)
        out[i] = d + 1
    return out

mem = [[1] * N for _ in range(32)]
phase = 0
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    parts = line.split()
    if parts[0] == "W":
        wa = int(parts[1]); raw = int(parts[2], 2)
        if mode == 3:
            for i in range(N):
                mem[wa][i] = (raw >> (3 * i)) & 7
        else:
            for i in range(N):
                mem[wa][i] = (raw >> (2 * i)) & 3
    elif parts[0] == "S":
        ra = int(parts[1]); rb = int(parts[2]); y = int(parts[3], 2)
        ref = add_words(mem[ra], mem[rb])
        for i in range(N):
            if mode == 3:
                expect = 1 << ref[i]
                got = (y >> (3 * i)) & 7
            else:
                expect = (ref[i] & 1) | ((ref[i] >> 1) << 1)
                got = (y >> (2 * i)) & 3
            if got != expect:
                print(f"FAIL phase {phase} trit {i} got {got} expect {expect} "
                      f"(ra {ra} rb {rb} y {y:0{2*N if mode != 3 else 3*N}b})")
                sys.exit(1)
        phase += 1
print("trace OK (2000 phases)")
PY
}

# N=2 wrappers; iverilog and Verilator both need the parameter fixed
cat > "$BUILD/dp_n2.v" <<'V'
module dp_canonical_n2(input clk, we, input [4:0] ra, rb, wa,
    input [3:0] wdata, output [3:0] y);
    dp_canonical #(.N(2)) u (.*);
endmodule
module dp_offset_n2(input clk, we, input [4:0] ra, rb, wa,
    input [3:0] wdata, output [3:0] y);
    dp_offset #(.N(2)) u (.*);
endmodule
module dp_onehot_bridge_n2(input clk, we, input [4:0] ra, rb, wa,
    input [3:0] wdata, output [3:0] y);
    dp_onehot_bridge #(.N(2)) u (.*);
endmodule
module dp_onehot_full_n2(input clk, we, input [4:0] ra, rb, wa,
    input [5:0] wdata, output [5:0] y);
    dp_onehot_full #(.N(2)) u (.*);
endmodule
V

# trit_add.v holds the decode/recode module that iverilog cannot elaborate;
# keep only the direct adder module for simulation
awk '/^module trit_add_direct/,/^endmodule/' fpga/gowin/trit_add.v \
    > "$BUILD/trit_add_direct_only.v"

TB_DIR="$BUILD/tb"
mkdir -p "$TB_DIR"

for spec in \
    "dp_canonical_n2 0 4 4" \
    "dp_offset_n2 1 4 4" \
    "dp_onehot_bridge_n2 2 4 4" \
    "dp_onehot_full_n2 3 6 6"; do
    set -- $spec
    top=$1; mode=$2; w=$3; ow=$4

    cat > "$TB_DIR/tb_$top.v" <<V
\`timescale 1ns/1ps
module tb;
    reg clk = 0;
    reg we = 0;
    reg [4:0] ra = 0, rb = 0, wa = 0;
    reg [$w-1:0] wdata = 0;
    $top dut (
        .clk(clk), .we(we), .ra(ra), .rb(rb), .wa(wa),
        .wdata(wdata), .y(y)
    );
    always #5 clk = ~clk;

    integer i, phase;
    reg [31:0] state;

    function [31:0] rnd;
        input dummy;
        begin
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            rnd = state;
        end
    endfunction

    initial begin
        state = 32'h12345678;
        for (i = 0; i < 32; i = i + 1) begin
            we = 1; wa = i;
            wdata = ($w == 6) ? 6'b001_001 : 4'b0101;
            @(posedge clk); #1;
        end
        we = 0;
        for (i = 0; i < 32; i = i + 1) begin
            ra = i; rb = i;
            @(posedge clk); #1;
        end

        for (phase = 0; phase < 2000; phase = phase + 1) begin
            we = 0;
            ra = rnd(1'b0) % 32;
            rb = rnd(1'b0) % 32;
            @(posedge clk); #1;
            \$display("S %0d %0d %0d %b", phase, ra, rb, y);

            wa = rnd(1'b0) % 32;
            if ($w == 6)
                wdata = (6'b100 << 0) | (6'b100 << 3);
            else
                wdata = (2'b10 << 0) | (2'b10 << 2);
            case (rnd(1'b0) % 3)
                0: begin
                    if ($w == 6) wdata = (6'b010 << 0) | (6'b010 << 3);
                    else wdata = (2'b01 << 0) | (2'b01 << 2);
                end
                1: begin
                    if ($w == 6) wdata = (6'b100 << 0) | (6'b001 << 3);
                    else wdata = (2'b10 << 0) | (2'b01 << 2);
                end
                default: begin end
            endcase
            \$display("W %0d %b", wa, wdata);
            we = 1;
            @(posedge clk); #1;
        end
        \$finish;
    end
endmodule
V

    iverilog -g2005 -o "$TB_DIR/tb_$top.vvp" \
        "$TB_DIR/tb_$top.v" \
        "$BUILD/dp_n2.v" \
        fpga/gowin/trit_datapath.v \
        "$BUILD/trit_add_direct_only.v" \
        fpga/gowin/trit_add_offset.v \
        fpga/gowin/trit_add_onehot.v \
        fpga/gowin/trit_recode.v
    vvp "$TB_DIR/tb_$top.vvp" > "$TB_DIR/tb_$top.trace"
    check_trace "$mode" < "$TB_DIR/tb_$top.trace" | sed "s/^/$top: /"
done

printf '%-20s %6s %6s %6s %6s\n' code LUT4 LUT1 BRAM TOTAL
declare -A RESULT
for spec in \
    "dp_canonical 0" \
    "dp_offset 1" \
    "dp_onehot_bridge 2" \
    "dp_onehot_full 3"; do
    set -- $spec
    top=$1; coding=$2
    yosys -q -p "read_verilog fpga/gowin/trit_datapath.v fpga/gowin/trit_add.v fpga/gowin/trit_add_offset.v fpga/gowin/trit_add_onehot.v fpga/gowin/trit_recode.v; synth_gowin -top $top -json $BUILD/${top}.json" \
        > "$BUILD/${top}.yosys.log" 2>&1
    read -r lut4 lut1 bram < <(count_cells "$BUILD/${top}.json" "$top")
    total=$((lut4 + lut1))
    RESULT[$top]="$lut4 $lut1 $bram $total"
    printf '%-20s %6s %6s %6s %6s\n' "$top" "$lut4" "$lut1" "$bram" "$total"
done

for top in dp_canonical dp_offset dp_onehot_bridge dp_onehot_full; do
    read -r lut4 lut1 bram total <<< "${RESULT[$top]}"
    echo "METRIC dp_lut_cells=$total"
    echo "METRIC dp_lut4=$lut4"
    echo "METRIC dp_lut1=$lut1"
    echo "METRIC dp_bram_blocks=$bram"
    echo "ASI variant=$top"
done

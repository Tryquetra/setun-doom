// Register file for the REBEL-6 core, H5 of docs/fpga-hypotheses.md.
// 729 registers of 24 trits packed at 2 bits per trit = 48 bits each.
// Reads are registered (BRAM-friendly style).

// Single bank: one synchronous write port and one synchronous read port.
module trit_bank_1r1w #(
    parameter DEPTH = 729,
    parameter WIDTH = 48
) (
    input  wire clk,
    input  wire [9:0] raddr,
    input  wire [9:0] waddr,
    input  wire [WIDTH-1:0] din,
    input  wire we,
    output reg [WIDTH-1:0] dout
);
    (* ram_style = "block" *)
    reg [WIDTH-1:0] mem [0:DEPTH-1];
    always @(posedge clk) begin
        if (we)
            mem[waddr] <= din;
        dout <= mem[raddr];
    end
endmodule

// Dual-bank register file: two mirrored banks give two read ports and one
// write port in a single clock cycle.
module trit_regfile_dualbank #(
    parameter DEPTH = 729,
    parameter WIDTH = 48
) (
    input  wire clk,
    input  wire [9:0] raddr_a,
    input  wire [9:0] raddr_b,
    input  wire [9:0] waddr,
    input  wire [WIDTH-1:0] din,
    input  wire we,
    output wire [WIDTH-1:0] dout_a,
    output wire [WIDTH-1:0] dout_b
);
    trit_bank_1r1w #(.DEPTH(DEPTH), .WIDTH(WIDTH)) bank_a (
        .clk(clk), .raddr(raddr_a), .waddr(waddr), .din(din),
        .we(we), .dout(dout_a)
    );
    trit_bank_1r1w #(.DEPTH(DEPTH), .WIDTH(WIDTH)) bank_b (
        .clk(clk), .raddr(raddr_b), .waddr(waddr), .din(din),
        .we(we), .dout(dout_b)
    );
endmodule

// Single-bank register file: port A reads every cycle; port B reads only
// when no write happens in the same cycle.
module trit_regfile_singlebank #(
    parameter DEPTH = 729,
    parameter WIDTH = 48
) (
    input  wire clk,
    input  wire [9:0] raddr_a,
    input  wire [9:0] raddr_b,
    input  wire [9:0] waddr,
    input  wire [WIDTH-1:0] din,
    input  wire we,
    output wire [WIDTH-1:0] dout_a,
    output reg [WIDTH-1:0] dout_b
);
    trit_bank_1r1w #(.DEPTH(DEPTH), .WIDTH(WIDTH)) bank (
        .clk(clk), .raddr(raddr_b), .waddr(waddr), .din(din),
        .we(we), .dout(dout_b)
    );
    trit_bank_1r1w #(.DEPTH(DEPTH), .WIDTH(WIDTH)) bank_a (
        .clk(clk), .raddr(raddr_a), .waddr(waddr), .din(din),
        .we(1'b0), .dout(dout_a)
    );
endmodule

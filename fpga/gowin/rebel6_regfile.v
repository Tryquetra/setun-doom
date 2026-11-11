// Register bank for the revised core: 729 registers of 64-bit signed values
// (Word24.raw() balanced representation). Two mirrored banks give two read
// ports and one write port per cycle, stored in BRAM.
module trit_bank64_1r1w #(
    parameter DEPTH = 729
) (
    input wire clk,
    input wire [9:0] raddr,
    input wire [9:0] waddr,
    input wire [63:0] din,
    input wire we,
    output reg [63:0] dout
);
    (* ram_style = "block" *)
    reg [63:0] mem [0:DEPTH-1];
    always @(posedge clk) begin
        if (we)
            mem[waddr] <= din;
        dout <= mem[raddr];
    end
endmodule

module rebel6_regfile_dualbank #(
    parameter DEPTH = 729
) (
    input wire clk,
    input wire [9:0] raddr_a,
    input wire [9:0] raddr_b,
    input wire [9:0] waddr,
    input wire [63:0] din,
    input wire we,
    output wire [63:0] dout_a,
    output wire [63:0] dout_b
);
    trit_bank64_1r1w #(.DEPTH(DEPTH)) bank_a (
        .clk(clk), .raddr(raddr_a), .waddr(waddr), .din(din),
        .we(we), .dout(dout_a)
    );
    trit_bank64_1r1w #(.DEPTH(DEPTH)) bank_b (
        .clk(clk), .raddr(raddr_b), .waddr(waddr), .din(din),
        .we(we), .dout(dout_b)
    );
endmodule

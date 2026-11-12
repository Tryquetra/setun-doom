// Dual-bank BRAM register file for the REBEL-6 core (729 registers).
// Parameterized width W (default 40 bits covers [-HALF, +HALF] in 6 DPX9B blocks).
// Register X0 is hardwired to zero on both read ports and ignores writes.
module trit_bank_1r1w #(
    parameter DEPTH = 729,
    parameter W     = 40
) (
    input  wire         clk,
    input  wire   [9:0] raddr,
    input  wire   [9:0] waddr,
    input  wire [W-1:0] din,
    input  wire         we,
    output reg  [W-1:0] dout
);
    (* ram_style = "block" *)
    reg [W-1:0] mem [0:DEPTH-1];

    integer init_i;
    initial begin
        for (init_i = 0; init_i < DEPTH; init_i = init_i + 1)
            mem[init_i] = {W{1'b0}};
    end

    always @(posedge clk) begin
        if (we)
            mem[waddr] <= din;
        dout <= mem[raddr];
    end
endmodule

module rebel6_regfile_dualbank #(
    parameter DEPTH = 729,
    parameter W     = 40
) (
    input  wire         clk,
    input  wire   [9:0] raddr_a,
    input  wire   [9:0] raddr_b,
    input  wire   [9:0] waddr,
    input  wire [W-1:0] din,
    input  wire         we,
    output wire [W-1:0] dout_a,
    output wire [W-1:0] dout_b
);
    wire         real_we = we & (waddr != 10'd0);
    wire [W-1:0] raw_a, raw_b;
    reg          zero_a, zero_b;

    always @(posedge clk) begin
        zero_a <= (raddr_a == 10'd0);
        zero_b <= (raddr_b == 10'd0);
    end

    trit_bank_1r1w #(.DEPTH(DEPTH), .W(W)) bank_a (
        .clk(clk), .raddr(raddr_a), .waddr(waddr), .din(din),
        .we(real_we), .dout(raw_a)
    );
    trit_bank_1r1w #(.DEPTH(DEPTH), .W(W)) bank_b (
        .clk(clk), .raddr(raddr_b), .waddr(waddr), .din(din),
        .we(real_we), .dout(raw_b)
    );

    assign dout_a = zero_a ? {W{1'b0}} : raw_a;
    assign dout_b = zero_b ? {W{1'b0}} : raw_b;
endmodule

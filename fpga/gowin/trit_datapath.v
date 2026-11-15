// H15: one datapath slice in four trit codings.
//
// Slice: register file of 32 N-trit words (two banks written in parallel for
// 2 reads + 1 write per cycle, registered reads like BRAM access) feeding an
// adder of the coding under test.
// Variants:
//   0 dp_canonical  - 2-bit canonical storage + H7 direct carry adder
//   1 dp_offset      - 2-bit canonical storage (same word format) + H14
//                      offset-coded adder
//   2 dp_onehot_bridge - 2-bit canonical storage, recode to one-hot at the
//                      read ports, H14 one-hot adder, recode back
//   3 dp_onehot_full - 3-bit one-hot storage + H14 one-hot adder
module trit_datapath #(
    parameter N = 24,
    parameter CODING = 0
) (
    input  wire clk,
    input  wire we,
    input  wire [4:0] ra,
    input  wire [4:0] rb,
    input  wire [4:0] wa,
    input  wire [2*N-1:0] wdata,
    output wire [2*N-1:0] y
);
    localparam W = (CODING >= 3) ? 3 * N : 2 * N;
    localparam OUTW = (CODING >= 3) ? 3 * N : 2 * N;

    reg [W-1:0] memA [0:31];
    reg [W-1:0] memB [0:31];
    reg [W-1:0] da, db;
    always @(posedge clk) begin
        if (we) begin
            memA[wa] <= wdata;
            memB[wa] <= wdata;
        end
        da <= memA[ra];
        db <= memB[rb];
    end
    wire [OUTW-1:0] yw;

    generate
        if (CODING == 0) begin : ADDCAN
            trit_add_direct #(.N(N)) u_add (
                .a(da), .b(db), .y(yw)
            );
        end else if (CODING == 1) begin : ADDOFF
            trit_add_offset #(.N(N)) u_add (
                .a(da), .b(db), .cin(2'b01),
                .y(yw), .cout()
            );
        end else if (CODING == 2) begin : ADDBRG
            wire [3*N-1:0] oha, ohb, ohs;
            trit_recode_2b_to_1h #(.N(N)) u_cvtA (.a(da), .y(oha));
            trit_recode_2b_to_1h #(.N(N)) u_cvtB (.a(db), .y(ohb));
            trit_add_onehot #(.N(N)) u_add (
                .a(oha), .b(ohb), .cin(3'b001),
                .y(ohs), .cout()
            );
            trit_recode_1h_to_2b #(.N(N)) u_cvtS (.a(ohs), .y(yw));
        end else begin : ADDFULL
            trit_add_onehot #(.N(N)) u_add (
                .a(da), .b(db), .cin(3'b001),
                .y(yw), .cout()
            );
        end
    endgenerate

    assign y = yw;
endmodule

module dp_canonical #(
    parameter N = 24
) (
    input wire clk, we,
    input wire [4:0] ra, rb, wa,
    input wire [2*N-1:0] wdata,
    output wire [2*N-1:0] y
);
    trit_datapath #(.N(N), .CODING(0)) u (
        .clk(clk), .we(we), .ra(ra), .rb(rb), .wa(wa),
        .wdata(wdata), .y(y)
    );
endmodule

module dp_offset #(
    parameter N = 24
) (
    input wire clk, we,
    input wire [4:0] ra, rb, wa,
    input wire [2*N-1:0] wdata,
    output wire [2*N-1:0] y
);
    trit_datapath #(.N(N), .CODING(1)) u (
        .clk(clk), .we(we), .ra(ra), .rb(rb), .wa(wa),
        .wdata(wdata), .y(y)
    );
endmodule

module dp_onehot_bridge #(
    parameter N = 24
) (
    input wire clk, we,
    input wire [4:0] ra, rb, wa,
    input wire [2*N-1:0] wdata,
    output wire [2*N-1:0] y
);
    trit_datapath #(.N(N), .CODING(2)) u (
        .clk(clk), .we(we), .ra(ra), .rb(rb), .wa(wa),
        .wdata(wdata), .y(y)
    );
endmodule

module dp_onehot_full #(
    parameter N = 24
) (
    input wire clk, we,
    input wire [4:0] ra, rb, wa,
    input wire [3*N-1:0] wdata,
    output wire [3*N-1:0] y
);
    trit_datapath #(.N(N), .CODING(3)) u (
        .clk(clk), .we(we), .ra(ra), .rb(rb), .wa(wa),
        .wdata(wdata), .y(y)
    );
endmodule

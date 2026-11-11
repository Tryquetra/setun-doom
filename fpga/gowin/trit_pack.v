// H4 of docs/fpga-hypotheses.md: register bank width variants and the
// packing converters that reduce storage.
//
// The raw canonical word takes 48 bits (2 bits per trit). Five trits fit in
// one byte (3^5 = 243 <= 256) by storing the shifted base-3 value, giving a
// 40-bit word. The whole 24-trit word fits in 39 bits because
// 3^24 < 2^39, giving dense binary packing.

module trit_bank_w48 #(
    parameter DEPTH = 729
) (
    input wire clk,
    input wire [9:0] raddr,
    input wire [9:0] waddr,
    input wire [47:0] din,
    input wire we,
    output reg [47:0] dout
);
    (* ram_style = "block" *)
    reg [47:0] mem [0:DEPTH-1];
    always @(posedge clk) begin
        if (we)
            mem[waddr] <= din;
        dout <= mem[raddr];
    end
endmodule

module trit_bank_w40 #(
    parameter DEPTH = 729
) (
    input wire clk,
    input wire [9:0] raddr,
    input wire [9:0] waddr,
    input wire [39:0] din,
    input wire we,
    output reg [39:0] dout
);
    (* ram_style = "block" *)
    reg [39:0] mem [0:DEPTH-1];
    always @(posedge clk) begin
        if (we)
            mem[waddr] <= din;
        dout <= mem[raddr];
    end
endmodule

module trit_bank_w39 #(
    parameter DEPTH = 729
) (
    input wire clk,
    input wire [9:0] raddr,
    input wire [9:0] waddr,
    input wire [38:0] din,
    input wire we,
    output reg [38:0] dout
);
    (* ram_style = "block" *)
    reg [38:0] mem [0:DEPTH-1];
    always @(posedge clk) begin
        if (we)
            mem[waddr] <= din;
        dout <= mem[raddr];
    end
endmodule

// Pack five canonical trits into one byte: value = sum (digit_i+1) * 3^i.
module pack_5trits #(
    parameter BASE = 0
) (
    input  wire [9:0] t,
    output wire [7:0] packed
);
    wire [1:0] d0 = {t[1], t[0]};
    wire [1:0] d1 = {t[3], t[2]};
    wire [1:0] d2 = {t[5], t[4]};
    wire [1:0] d3 = {t[7], t[6]};
    wire [1:0] d4 = {t[9], t[8]};
    wire [2:0] v0 = (d0 == 2'b01) ? 3'sd2 : (d0 == 2'b10) ? 3'sd0 : 3'sd1;
    wire [2:0] v1 = (d1 == 2'b01) ? 3'sd2 : (d1 == 2'b10) ? 3'sd0 : 3'sd1;
    wire [3:0] v2 = (d2 == 2'b01) ? 4'sd2 : (d2 == 2'b10) ? 4'sd0 : 4'sd1;
    wire [4:0] v3 = (d3 == 2'b01) ? 5'sd2 : (d3 == 2'b10) ? 5'sd0 : 5'sd1;
    wire [5:0] v4 = (d4 == 2'b01) ? 6'sd2 : (d4 == 2'b10) ? 6'sd0 : 6'sd1;
    wire [7:0] s0 = v0;
    wire [7:0] s1 = v1 * 3;
    wire [7:0] s2 = v2 * 9;
    wire [7:0] s3 = v3 * 27;
    wire [7:0] s4 = v4 * 81;
    assign packed = s0 + s1 + s2 + s3 + s4;
endmodule

// Pack the full 24-trit word into 40 bits (five bytes).
module pack_word40 (
    input  wire [47:0] t,
    output wire [39:0] packed
);
    wire [7:0] b0, b1, b2, b3, b4;
    pack_5trits p0 (.t(t[9:0]), .packed(b0));
    pack_5trits p1 (.t(t[19:10]), .packed(b1));
    pack_5trits p2 (.t(t[29:20]), .packed(b2));
    pack_5trits p3 (.t(t[39:30]), .packed(b3));
    pack_5trits p4 (.t({2'b00, t[47:40]}), .packed(b4));
    assign packed = {b4, b3, b2, b1, b0};
endmodule

// Pack the full 24-trit word into 39 bits by direct base-3 value:
// value = sum (digit_i+1) * 3^i, which spans 0..3^24-1.
module pack_word39 (
    input  wire [47:0] t,
    output wire [38:0] packed
);
    wire [37:0] term [0:23];
    genvar i;
    generate
        for (i = 0; i < 24; i = i + 1) begin : terms
            wire [1:0] d = {t[2*i+1], t[2*i]};
            wire [2:0] v = (d == 2'b01) ? 3'sd2 : (d == 2'b10) ? 3'sd0 : 3'sd1;
            if (i == 0)
                assign term[0] = {35'b0, v};
            else if (i == 1)
                assign term[1] = {34'b0, v} * 3;
            else if (i == 2)
                assign term[2] = {33'b0, v} * 9;
            else if (i == 3)
                assign term[3] = {32'b0, v} * 27;
            else if (i == 4)
                assign term[4] = {31'b0, v} * 81;
            else if (i == 5)
                assign term[5] = {30'b0, v} * 243;
            else if (i == 6)
                assign term[6] = {29'b0, v} * 729;
            else if (i == 7)
                assign term[7] = {28'b0, v} * 2187;
            else if (i == 8)
                assign term[8] = {27'b0, v} * 6561;
            else if (i == 9)
                assign term[9] = {26'b0, v} * 19683;
            else if (i == 10)
                assign term[10] = {25'b0, v} * 59049;
            else if (i == 11)
                assign term[11] = {24'b0, v} * 177147;
            else if (i == 12)
                assign term[12] = {23'b0, v} * 531441;
            else if (i == 13)
                assign term[13] = {22'b0, v} * 1594323;
            else if (i == 14)
                assign term[14] = {21'b0, v} * 4782969;
            else if (i == 15)
                assign term[15] = {20'b0, v} * 14348907;
            else if (i == 16)
                assign term[16] = {19'b0, v} * 43046721;
            else if (i == 17)
                assign term[17] = {18'b0, v} * 129140163;
            else if (i == 18)
                assign term[18] = {17'b0, v} * 387420489;
            else if (i == 19)
                assign term[19] = {16'b0, v} * 1162261467;
            else if (i == 20)
                assign term[20] = {15'b0, v} * 3486784401;
            else if (i == 21)
                assign term[21] = {14'b0, v} * 10460353203;
            else if (i == 22)
                assign term[22] = {13'b0, v} * 31381059609;
            else
                assign term[23] = {12'b0, v} * 94143178827;
        end
    endgenerate
    wire [38:0] acc [0:23];
    assign acc[0] = term[0];
    generate
        for (i = 1; i < 24; i = i + 1) begin : sums
            assign acc[i] = acc[i-1] + term[i];
        end
    endgenerate
    assign packed = acc[23];
endmodule

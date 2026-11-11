// Three 24-trit adders, H7 of docs/fpga-hypotheses.md. The canonical code is
// used throughout: 00 = 0, 01 = +1, 10 = -1, 11 = error. Addition wraps
// modulo 3^N (the final carry is dropped). Trit i sits in bits
// [2i+1:2i], hi = bit 2i+1, lo = bit 2i.

// Variant A: direct balanced-ternary ripple carry. The carry is a trit in
// {-1, 0, +1} decoded into plus/neg flags; the sum digit and the next carry
// come from the signed sum of the flags.
module trit_add_direct #(
    parameter N = 24
) (
    input  wire [2*N-1:0] a,
    input  wire [2*N-1:0] b,
    output wire [2*N-1:0] y
);
    wire [N:0] cp, cn; // carry plus and carry neg, one per stage
    assign cp[0] = 1'b0;
    assign cn[0] = 1'b0;
    genvar i;
    generate
        for (i = 0; i < N; i = i + 1) begin : stages
            wire ap = a[2*i]   & ~a[2*i+1];
            wire an = a[2*i+1] & ~a[2*i];
            wire bp = b[2*i]   & ~b[2*i+1];
            wire bn = b[2*i+1] & ~b[2*i];
            wire signed [3:0] t =
                $signed({1'b0, ap}) - $signed({1'b0, an}) +
                $signed({1'b0, bp}) - $signed({1'b0, bn}) +
                $signed({1'b0, cp[i]}) - $signed({1'b0, cn[i]});
            // {s_hi, s_lo, cnext_hi, cnext_lo}; cp encodes +1 as lo, cn -1 as hi
            assign y[2*i+1] = (t == 4'sd2) | (t == -4'sd1);
            assign y[2*i]   = (t == 4'sd1) | (t == -4'sd2);
            assign cp[i+1] = (t == 4'sd2) | (t == 4'sd3);
            assign cn[i+1] = (t == -4'sd2) | (t == -4'sd3);
        end
    endgenerate
endmodule

// Variant B: decode each operand to binary (Horner over powers of 3), add in
// plain binary, then recode the sum back to balanced ternary digits. The two
// conversions are automatic functions whose locals are per-call, so no shared
// state exists between the decode and recode steps.
module trit_add_decode_recode #(
    parameter N = 24
) (
    input  wire [2*N-1:0] a,
    input  wire [2*N-1:0] b,
    output reg [2*N-1:0] y
);
    function automatic [63:0] trit_value(input [1:0] t);
        begin
            case (t)
                2'b01: trit_value = 64'sd1;
                2'b10: trit_value = -64'sd1;
                default: trit_value = 64'sd0;
            endcase
        end
    endfunction

    function automatic [63:0] word_value(input [2*N-1:0] w);
        integer k;
        reg signed [63:0] acc;
        begin
            acc = 64'sd0;
            for (k = N - 1; k >= 0; k = k - 1)
                acc = (acc << 1) + acc + trit_value(w[2*k+1:2*k]);
            word_value = acc;
        end
    endfunction

    function automatic [2*N-1:0] bal_recode(input signed [63:0] v);
        integer k;
        reg signed [63:0] t, r, d;
        begin
            t = v;
            for (k = 0; k < N; k = k + 1) begin
                r = t % 64'sd3;
                if (r == 64'sd2) d = -64'sd1;
                else if (r == -64'sd2) d = 64'sd1;
                else d = r;
                bal_recode[2*k]   = (d == 64'sd1);
                bal_recode[2*k+1] = (d == -64'sd1);
                t = (t - d) / 64'sd3;
            end
        end
    endfunction

    always @* begin
        y = bal_recode(word_value(a) + word_value(b));
    end
endmodule

// Variant C: ripple over the binary-encoded trits. Each stage computes the
// signed sum t = lo_a - hi_a + lo_b - hi_b + lo_c - hi_c in {-3..3} with a
// 4-bit signed adder and derives the sum digit and the next carry from the
// comparisons on t.
module trit_add_binary_ripple #(
    parameter N = 24
) (
    input  wire [2*N-1:0] a,
    input  wire [2*N-1:0] b,
    output wire [2*N-1:0] y
);
    wire [N:0] c_hi, c_lo; // raw two-bit carry codes
    assign c_hi[0] = 1'b0;
    assign c_lo[0] = 1'b0;
    genvar i;
    generate
        for (i = 0; i < N; i = i + 1) begin : stages
            wire signed [3:0] t =
                $signed({1'b0, a[2*i]})   - $signed({1'b0, a[2*i+1]}) +
                $signed({1'b0, b[2*i]})   - $signed({1'b0, b[2*i+1]}) +
                $signed({1'b0, c_lo[i]})  - $signed({1'b0, c_hi[i]});
            assign y[2*i+1] = (t == 4'sd2) | (t == -4'sd1);
            assign y[2*i]   = (t == 4'sd1) | (t == -4'sd2);
            assign c_hi[i+1] = (t == -4'sd2) | (t == -4'sd3);
            assign c_lo[i+1] = (t == 4'sd2) | (t == 4'sd3);
        end
    endgenerate
endmodule

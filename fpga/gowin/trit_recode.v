// H15: conversions between the 2-bit canonical trit code (used in memory)
// and the 3-bit one-hot code (used by the one-hot adder of H14).
//
// 2b -> 1h: 00 -> 001, 01 -> 010, 10 -> 100, 11 -> 001 (reserved code is
// mapped to zero so the conversion is total; the input side normally never
// produces 11).
// 1h -> 2b: value = bit[1] - bit[2], so 001 -> 00, 010 -> 01, 100 -> 10;
// multi-hot inputs decode deterministically (e.g. 111 -> 00).
module trit_recode_2b_to_1h #(
    parameter N = 24
) (
    input  wire [2*N-1:0] a,
    output wire [3*N-1:0] y
);
    genvar i;
    generate
        for (i = 0; i < N; i = i + 1) begin : trits
            wire ap = a[2*i]   & ~a[2*i+1];
            wire an = a[2*i+1] & ~a[2*i];
            assign y[3*i+2] = an;
            assign y[3*i+1] = ap;
            assign y[3*i]   = ~(ap | an);
        end
    endgenerate
endmodule

module trit_recode_1h_to_2b #(
    parameter N = 24
) (
    input  wire [3*N-1:0] a,
    output wire [2*N-1:0] y
);
    genvar i;
    generate
        for (i = 0; i < N; i = i + 1) begin : trits
            assign y[2*i]   = a[3*i+1];
            assign y[2*i+1] = a[3*i+2];
        end
    endgenerate
endmodule

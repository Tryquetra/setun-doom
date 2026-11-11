// Trit negation in the two 2-bit-per-trit encodings, H2 of docs/fpga-hypotheses.md.
//
// Canonical code (matches src/rebel6/encoding.rs): 00 = 0, 01 = +1, 10 = -1,
// 11 = error. Negation is a per-trit bit swap, pure wiring.
//
// Redundant code: 00 and 11 both mean 0. Negation maps +1 (01) to -1 (10) and
// 0 to 0 via 00 <-> 11, which is a per-bit inversion, pure wiring.

// t_out[i] = swap(t_in[i]): bit 2i and 2i+1 exchange.
module trit_neg_canonical #(
    parameter N = 24
) (
    input  wire [2*N-1:0] t_in,
    output wire [2*N-1:0] t_out
);
    genvar i;
    generate
        for (i = 0; i < N; i = i + 1) begin : trits
            assign t_out[2*i+1] = t_in[2*i];
            assign t_out[2*i]   = t_in[2*i+1];
        end
    endgenerate
endmodule

// t_out[i] = ~t_in[i].
module trit_neg_redundant #(
    parameter N = 24
) (
    input  wire [2*N-1:0] t_in,
    output wire [2*N-1:0] t_out
);
    assign t_out = ~t_in;
endmodule

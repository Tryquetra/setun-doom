// Word-level equality over N trits, H3 of docs/fpga-hypotheses.md.
//
// Canonical: trits have one code per value, so the word is equal iff all
// 2N bits match. That is the XOR reduction of 48 bits with an inversion at
// the end, no normalization.
//
// Redundant: the zero has two realizations (00 and 11), so trits are equal
// iff their decoded signs match, which collapses the two zero codes before
// the reduction.

module trit_word_eq_canonical #(
    parameter N = 24
) (
    input  wire [2*N-1:0] a,
    input  wire [2*N-1:0] b,
    output wire eq
);
    wire [2*N-1:0] diff = a ^ b;
    assign eq = ~|diff;
endmodule

module trit_word_eq_redundant #(
    parameter N = 24
) (
    input  wire [2*N-1:0] a,
    input  wire [2*N-1:0] b,
    output wire eq
);
    wire [N-1:0] eq_t;
    genvar i;
    generate
        for (i = 0; i < N; i = i + 1) begin : trits
            wire ap = a[2*i]   & ~a[2*i+1];
            wire an = a[2*i+1] & ~a[2*i];
            wire az = ~(a[2*i] ^ a[2*i+1]);
            wire bp = b[2*i]   & ~b[2*i+1];
            wire bn = b[2*i+1] & ~b[2*i];
            wire bz = ~(b[2*i] ^ b[2*i+1]);
            assign eq_t[i] = (ap & bp) | (an & bn) | (az & bz);
        end
    endgenerate
    assign eq = &eq_t;
endmodule

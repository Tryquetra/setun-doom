// Ternary operators over N trits in the two 2-bit-per-trit encodings,
// H1 of docs/fpga-hypotheses.md.
//
// Trit i occupies bits [2i+1:2i] with hi = bit 2i+1, lo = bit 2i.
// Meaning: lo=1,hi=0 is +1 (01); hi=1,lo=0 is -1 (10); both 0 is 0 (00).
// The redundant code also accepts hi=lo=1 as 0.
//
// Each module decodes every input trit into plus/neg/zero flags and then
// implements the operator over the flags. For the redundant code the zero
// flag is an XNOR (00 or 11), for the canonical code a NOR (00 only).
// Outputs only use the codes 00, 01 and 10, legal in both encodings.

module trit_min_canonical #(
    parameter N = 24
) (
    input  wire [2*N-1:0] a,
    input  wire [2*N-1:0] b,
    output wire [2*N-1:0] y
);
    genvar i;
    generate
        for (i = 0; i < N; i = i + 1) begin : trits
            wire ap = a[2*i]   & ~a[2*i+1];
            wire an = a[2*i+1] & ~a[2*i];
            wire bp = b[2*i]   & ~b[2*i+1];
            wire bn = b[2*i+1] & ~b[2*i];
            assign y[2*i+1] = an | bn;
            assign y[2*i]   = ap & bp;
        end
    endgenerate
endmodule

module trit_min_redundant #(
    parameter N = 24
) (
    input  wire [2*N-1:0] a,
    input  wire [2*N-1:0] b,
    output wire [2*N-1:0] y
);
    genvar i;
    generate
        for (i = 0; i < N; i = i + 1) begin : trits
            wire ap = a[2*i]   & ~a[2*i+1];
            wire an = a[2*i+1] & ~a[2*i];
            wire bp = b[2*i]   & ~b[2*i+1];
            wire bn = b[2*i+1] & ~b[2*i];
            assign y[2*i+1] = an | bn;
            assign y[2*i]   = ap & bp;
        end
    endgenerate
endmodule

module trit_max_canonical #(
    parameter N = 24
) (
    input  wire [2*N-1:0] a,
    input  wire [2*N-1:0] b,
    output wire [2*N-1:0] y
);
    genvar i;
    generate
        for (i = 0; i < N; i = i + 1) begin : trits
            wire ap = a[2*i]   & ~a[2*i+1];
            wire an = a[2*i+1] & ~a[2*i];
            wire bp = b[2*i]   & ~b[2*i+1];
            wire bn = b[2*i+1] & ~b[2*i];
            assign y[2*i+1] = an & bn;
            assign y[2*i]   = ap | bp;
        end
    endgenerate
endmodule

module trit_max_redundant #(
    parameter N = 24
) (
    input  wire [2*N-1:0] a,
    input  wire [2*N-1:0] b,
    output wire [2*N-1:0] y
);
    genvar i;
    generate
        for (i = 0; i < N; i = i + 1) begin : trits
            wire ap = a[2*i]   & ~a[2*i+1];
            wire an = a[2*i+1] & ~a[2*i];
            wire bp = b[2*i]   & ~b[2*i+1];
            wire bn = b[2*i+1] & ~b[2*i];
            assign y[2*i+1] = an & bn;
            assign y[2*i]   = ap | bp;
        end
    endgenerate
endmodule

module trit_sum3_canonical #(
    parameter N = 24
) (
    input  wire [2*N-1:0] a,
    input  wire [2*N-1:0] b,
    output wire [2*N-1:0] y
);
    genvar i;
    generate
        for (i = 0; i < N; i = i + 1) begin : trits
            wire ap = a[2*i]   & ~a[2*i+1];
            wire an = a[2*i+1] & ~a[2*i];
            wire az = ~a[2*i] & ~a[2*i+1];
            wire bp = b[2*i]   & ~b[2*i+1];
            wire bn = b[2*i+1] & ~b[2*i];
            wire bz = ~b[2*i] & ~b[2*i+1];
            assign y[2*i]   = (ap & bz) | (bp & az) | (an & bn);
            assign y[2*i+1] = (an & bz) | (bn & az) | (ap & bp);
        end
    endgenerate
endmodule

module trit_sum3_redundant #(
    parameter N = 24
) (
    input  wire [2*N-1:0] a,
    input  wire [2*N-1:0] b,
    output wire [2*N-1:0] y
);
    genvar i;
    generate
        for (i = 0; i < N; i = i + 1) begin : trits
            wire ap = a[2*i]   & ~a[2*i+1];
            wire an = a[2*i+1] & ~a[2*i];
            wire az = ~(a[2*i] ^ a[2*i+1]);
            wire bp = b[2*i]   & ~b[2*i+1];
            wire bn = b[2*i+1] & ~b[2*i];
            wire bz = ~(b[2*i] ^ b[2*i+1]);
            assign y[2*i]   = (ap & bz) | (bp & az) | (an & bn);
            assign y[2*i+1] = (an & bz) | (bn & az) | (ap & bp);
        end
    endgenerate
endmodule

module trit_eq_canonical #(
    parameter N = 24
) (
    input  wire [2*N-1:0] a,
    input  wire [2*N-1:0] b,
    output wire [2*N-1:0] y
);
    genvar i;
    generate
        for (i = 0; i < N; i = i + 1) begin : trits
            wire ap = a[2*i]   & ~a[2*i+1];
            wire an = a[2*i+1] & ~a[2*i];
            wire az = ~a[2*i] & ~a[2*i+1];
            wire bp = b[2*i]   & ~b[2*i+1];
            wire bn = b[2*i+1] & ~b[2*i];
            wire bz = ~b[2*i] & ~b[2*i+1];
            assign y[2*i]   = (ap & bp) | (an & bn) | (az & bz);
            assign y[2*i+1] = 1'b0;
        end
    endgenerate
endmodule

module trit_eq_redundant #(
    parameter N = 24
) (
    input  wire [2*N-1:0] a,
    input  wire [2*N-1:0] b,
    output wire [2*N-1:0] y
);
    genvar i;
    generate
        for (i = 0; i < N; i = i + 1) begin : trits
            wire ap = a[2*i]   & ~a[2*i+1];
            wire an = a[2*i+1] & ~a[2*i];
            wire az = ~(a[2*i] ^ a[2*i+1]);
            wire bp = b[2*i]   & ~b[2*i+1];
            wire bn = b[2*i+1] & ~b[2*i];
            wire bz = ~(b[2*i] ^ b[2*i+1]);
            assign y[2*i]   = (ap & bp) | (an & bn) | (az & bz);
            assign y[2*i+1] = 1'b0;
        end
    endgenerate
endmodule

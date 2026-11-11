// Error detection over a canonical 24-trit word, H6 of
// docs/fpga-hypotheses.md. Trit i sits in bits [2i+1:2i] with 00 = 0,
// 01 = +1, 10 = -1 and 11 reserved for error.

// Per-trit detection: any 11 pair raises the error flag.
module trit_err_detect #(
    parameter N = 24
) (
    input  wire [2*N-1:0] a,
    output wire err
);
    wire [N-1:0] bad;
    genvar i;
    generate
        for (i = 0; i < N; i = i + 1) begin : trits
            assign bad[i] = a[2*i] & a[2*i+1];
        end
    endgenerate
    assign err = |bad;
endmodule

// Word parity: odd number of set bits over the 2N bits.
module trit_word_parity #(
    parameter N = 24
) (
    input  wire [2*N-1:0] a,
    output wire err
);
    assign err = ^a;
endmodule

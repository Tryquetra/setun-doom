// H14-A: offset-coded (biased binary) trit adder.
//
// Trit value v = code - 1, with code stored as a 2-bit binary number in
// {0, 1, 2}: 00 -> -1, 01 -> 0, 10 -> +1, 11 -> error (reserved, C3/H6).
// Per stage the three codes are added as plain binary numbers,
// s = a + b + cin in 0..6, and the balanced digit/carry pair is recovered by
//   d_code = smod3(s - 2),   co_code = clamp(s - 2, 0, 2)
// because s - 2 = (value sum) + 1. The carry is an offset code too, so the
// chain is a ripple of 2-bit binary adders plus a 7-entry case.
module trit_add_offset #(
    parameter N = 24
) (
    input  wire [2*N-1:0] a,
    input  wire [2*N-1:0] b,
    input  wire [1:0] cin,
    output wire [2*N-1:0] y,
    output wire [1:0] cout
);
    wire [1:0] c [N:0];
    assign c[0] = cin;
    genvar i;
    generate
        for (i = 0; i < N; i = i + 1) begin : stages
            wire [2:0] s =
                {1'b0, a[2*i+1], a[2*i]} +
                {1'b0, b[2*i+1], b[2*i]} +
                {1'b0, c[i]};
            reg [1:0] d, co;
            always @* begin
                case (s)
                    3'd0: begin d = 2'd1; co = 2'd0; end
                    3'd1: begin d = 2'd2; co = 2'd0; end
                    3'd2: begin d = 2'd0; co = 2'd1; end
                    3'd3: begin d = 2'd1; co = 2'd1; end
                    3'd4: begin d = 2'd2; co = 2'd1; end
                    3'd5: begin d = 2'd0; co = 2'd2; end
                    3'd6: begin d = 2'd1; co = 2'd2; end
                    default: begin d = 2'd1; co = 2'd1; end
                endcase
            end
            assign y[2*i+1] = d[1];
            assign y[2*i]   = d[0];
            assign c[i+1] = co;
        end
    endgenerate
    assign cout = c[N];
endmodule

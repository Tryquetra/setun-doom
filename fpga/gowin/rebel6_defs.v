// REBEL-6 RTL definitions.
// Words are 32 trits packed at 2 bits per trit, lane 0 least significant.
// Trit code: 00 = 0, 01 = +1, 10 = -1, 11 = error.

module rebel6_defs;
    localparam MODULUS = 64'd282429536481;
    localparam HALF    = 64'd141214768240;
endmodule

// Reads one trit from code as a signed 2-bit value in {-1,0,+1} plus an
// error flag for the illegal 11 code.
module trit_read (
    input  wire [1:0] code,
    output reg signed [1:0] val,
    output reg err
);
    always @* begin
        err = 1'b0;
        case (code)
            2'b00: val = 2'sd0;
            2'b01: val = 2'sd1;
            2'b10: val = -2'sd1;
            default: begin val = 2'sd0; err = 1'b1; end
        endcase
    end
endmodule

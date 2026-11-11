// REBEL-6 ALU subset acting on signed 64-bit word values in the balanced
// range [-HALF, HALF], mirroring Word24::raw() semantics from the Rust core.
// Base Ternary arithmetic wraps modulo 3^24; Base Binary reinterprets the low
// 32 bits and wraps modulo 2^32.
//
// op selectors (5-bit):
//   0 TADD   a+b wrap mod 3^24
//   1 TSUB   a-b wrap mod 3^24
//   2 TSLT   a<b (balanced) -> 1/0
//   3 TMIN   min(a,b)
//   4 TMAX   max(a,b)
//   5 TANDW  min(a,b) wordwise (uses same path as TMIN)
//   6 BADD   i32 add (wrapping)
//   7 BSUB   i32 sub
//   8 BSLL   i32 << (b&31)
//   9 BSRL   (u32)a >> (b&31)
//  10 BSRA   i32 >> (b&31)
//  11 BSLTU  (u32)a < (u32)b
//  12 BOR    i32 | i32
//  13 BXOR   i32 ^ i32
//  14 BAND   i32 & i32
//  15 BADDSUB (reserved)

module rebel6_alu #(
    parameter W = 64
) (
    input  wire [4:0] op,
    input  wire signed [W-1:0] a,
    input  wire signed [W-1:0] b,
    output reg signed [W-1:0] res,
    output reg eq,
    output reg lt
);
    localparam HALF = 64'd141214768240;
    localparam MODULUS = 64'd282429536481;

    wire signed [W:0] ta = $signed(a) + $signed(b);
    wire signed [W:0] ts = $signed(a) - $signed(b);

    // balanced wrap helpers
    function automatic signed [W-1:0] wrap_add;
        input signed [W:0] s;
        begin
            if (s > $signed(HALF))
                wrap_add = s - $signed(MODULUS);
            else if (s < -$signed(HALF))
                wrap_add = s + $signed(MODULUS);
            else
                wrap_add = s[W-1:0];
        end
    endfunction

    reg [31:0] t_add, t_sub, t_sll, t_srl, t_sra, t_or, t_xor, t_and;
    reg [31:0] a32, b32;
    always @* begin
        a32 = a[31:0];
        b32 = b[31:0];
        t_add = a32 + b32;
        t_sub = a32 - b32;
        t_sll = a32 << b[4:0];
        t_srl = a32 >> b[4:0];
        t_sra = $signed(a32) >>> b[4:0];
        t_or = a32 | b32;
        t_xor = a32 ^ b32;
        t_and = a32 & b32;
    end
    function automatic signed [W-1:0] sext32;
        input [31:0] v;
        begin
            sext32 = {{32{v[31]}}, v};
        end
    endfunction

    always @* begin
        res = 64'sd0;
        eq = (a == b);
        lt = (a < b);
        case (op)
            5'd0: res = wrap_add(ta);
            5'd1: res = wrap_add(ts);
            5'd2: res = lt ? 64'sd1 : 64'sd0;
            5'd3: res = (a < b) ? a : b;
            5'd4: res = (a > b) ? a : b;
            5'd5: res = (a < b) ? a : b;
            5'd6: res = sext32(t_add);
            5'd7: res = sext32(t_sub);
            5'd8: res = sext32(t_sll);
            5'd9: res = sext32(t_srl);
            5'd10: res = sext32(t_sra);
            5'd11: res = (a32 < b32) ? 64'sd1 : 64'sd0;
            5'd12: res = sext32(t_or);
            5'd13: res = sext32(t_xor);
            5'd14: res = sext32(t_and);
        endcase
    end
endmodule

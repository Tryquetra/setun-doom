// REBEL-6 ALU acting on signed W-bit word values in the balanced range
// [-HALF, HALF], mirroring Word24::raw() semantics from the Rust core.
// Default W=64 keeps compatibility with rebel6_alu_tb; core instantiates W=40.
//
// op selectors (5-bit):
//   0 TADD   a+b wrap mod 3^24
//   1 TSUB   a-b wrap mod 3^24
//   2 TSLT   a<b (balanced) -> 1/0
//   3 TMIN   min(a,b)
//   4 TMAX   max(a,b)
//   5 TCMP   a<=>b -> -1/0/+1 (cmp.t)
//   6 BADD   i32 add (wrapping, sign-extended)
//   7 BSUB   i32 sub
//   8 BSLL   i32 << (b&31)
//   9 BSRL   (u32)a >> (b&31)
//  10 BSRA   i32 >>> (b&31)
//  11 BSLTU  (u32)a < (u32)b
//  12 BOR    i32 | i32
//  13 BXOR   i32 ^ i32
//  14 BAND   i32 & i32
//  15 TSTI   -a (sti.t)
//  16 PASSB  b  (li.t / aipc.t)
module rebel6_alu #(
    parameter W = 64
) (
    input  wire        [4:0] op,
    input  wire signed [W-1:0] a,
    input  wire signed [W-1:0] b,
    output reg  signed [W-1:0] res,
    output wire              eq,
    output wire              lt,
    output wire              ltu32
);
    localparam signed [W:0] HALF    = 41'sd141214768240;
    localparam signed [W:0] MODULUS = 41'sd282429536481;

    wire signed [W:0] ta = $signed(a) + $signed(b);
    wire signed [W:0] ts = $signed(a) - $signed(b);

    function automatic signed [W-1:0] wrap_val(input signed [W:0] s);
        reg signed [W:0] tmp;
        begin
            if (s > HALF)
                tmp = s - MODULUS;
            else if (s < -HALF)
                tmp = s + MODULUS;
            else
                tmp = s;
            wrap_val = tmp[W-1:0];
        end
    endfunction

    wire [31:0] a32   = a[31:0];
    wire [31:0] b32   = b[31:0];
    wire [31:0] t_add = a32 + b32;
    wire [31:0] t_sub = a32 - b32;
    wire [31:0] t_sll = a32 << b32[4:0];
    wire [31:0] t_srl = a32 >> b32[4:0];
    wire [31:0] t_sra = $signed(a32) >>> b32[4:0];
    wire [31:0] t_or  = a32 | b32;
    wire [31:0] t_xor = a32 ^ b32;
    wire [31:0] t_and = a32 & b32;

    function automatic signed [W-1:0] sext32(input [31:0] v);
        begin
            sext32 = {{(W-32){v[31]}}, v};
        end
    endfunction

    assign eq    = (a == b);
    assign lt    = (a < b);
    assign ltu32 = (a32 < b32);

    always @* begin
        res = {W{1'b0}};
        case (op)
            5'd0:  res = wrap_val(ta);
            5'd1:  res = wrap_val(ts);
            5'd2:  res = lt ? {{(W-1){1'b0}}, 1'b1} : {W{1'b0}};
            5'd3:  res = lt ? a : b;
            5'd4:  res = (a > b) ? a : b;
            5'd5:  res = (a > b) ? {{(W-1){1'b0}}, 1'b1} : (lt ? {W{1'b1}} : {W{1'b0}});
            5'd6:  res = sext32(t_add);
            5'd7:  res = sext32(t_sub);
            5'd8:  res = sext32(t_sll);
            5'd9:  res = sext32(t_srl);
            5'd10: res = sext32(t_sra);
            5'd11: res = ltu32 ? {{(W-1){1'b0}}, 1'b1} : {W{1'b0}};
            5'd12: res = sext32(t_or);
            5'd13: res = sext32(t_xor);
            5'd14: res = sext32(t_and);
            5'd15: res = -a;
            5'd16: res = b;
            default: res = {W{1'b0}};
        endcase
    end
endmodule

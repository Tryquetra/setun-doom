module rebel6_alu_tb (
    input wire [4:0] op,
    input wire signed [63:0] a,
    input wire signed [63:0] b,
    output wire [65:0] dbg
);
    wire signed [63:0] res;
    wire eq, lt;
    rebel6_alu u_alu (.op(op), .a(a), .b(b), .res(res), .eq(eq), .lt(lt));
    assign dbg = {res, eq, lt};
endmodule

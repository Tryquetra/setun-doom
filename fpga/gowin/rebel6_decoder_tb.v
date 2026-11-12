module rebel6_decoder_tb (
    input  wire        [63:0] instr,
    output wire       [135:0] dbg
);
    wire               valid;
    wire               err_11;
    wire         [6:0] op;
    wire signed  [9:0] rs1, rs2, rs3, rd1, rd2;
    wire signed [47:0] imm, imm2;
    wire signed  [1:0] fill;
    wire               fmt_long;

    rebel6_decoder u_dec (
        .instr(instr), .valid(valid), .err_11(err_11), .op(op),
        .rs1(rs1), .rs2(rs2), .rs3(rs3), .rd1(rd1), .rd2(rd2),
        .imm(imm), .imm2(imm2), .fill(fill), .fmt_long(fmt_long)
    );

    assign dbg = {8'd0, valid, op, rs1[5:0], rs2[5:0], rs3[5:0], rd1[5:0], rd2[5:0],
                  imm[47:0], imm2[47:0], fill};
endmodule

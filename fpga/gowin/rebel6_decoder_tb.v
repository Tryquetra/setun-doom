module rebel6_decoder_tb (
    input wire [63:0] instr,
    output wire [135:0] dbg
);
    wire valid;
    wire [6:0] op;
    wire [5:0] rs1, rs2, rs3, rd1, rd2;
    wire signed [47:0] imm, imm2;
    wire signed [1:0] fill;
    wire fmt_long;
    rebel6_decoder u_dec (
        .instr(instr), .valid(valid), .op(op),
        .rs1(rs1), .rs2(rs2), .rs3(rs3), .rd1(rd1), .rd2(rd2),
        .imm(imm), .imm2(imm2), .fill(fill), .fmt_long(fmt_long)
    );
    // fixed packing: valid(1) op(7) rs1(6) rs2(6) rs3(6) rd1(6) rd2(6)
    // imm(48) imm2(48) fill(2) = 128 bits
    assign dbg = {valid, op, rs1, rs2, rs3, rd1, rd2,
                  imm[47:0], imm2[47:0], fill};
endmodule

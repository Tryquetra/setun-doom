// REBEL-6 combinational instruction decoder (factored shared-tryte architecture).
//
// Decodes all 73 architectural instructions across the 7 REBEL-6 formats
// (R, I, B, G, X, D, Y) from a 64-bit packed 32-trit word (2 bits/trit:
// 00 = 0, 01 = +1, 10 = -1, 11 = illegal).
//
// Factored field extraction decodes each 6-trit lane group once and composes
// 12-trit and 24-trit immediates via shift-add constants (729 and 531441),
// cutting synthesis area ~10x vs redundant per-format converters.
module rebel6_decoder (
    input  wire        [63:0] instr,
    output wire               valid,
    output wire               err_11,
    output wire         [6:0] op,
    output wire signed  [9:0] rs1,
    output wire signed  [9:0] rs2,
    output wire signed  [9:0] rs3,
    output wire signed  [9:0] rd1,
    output wire signed  [9:0] rd2,
    output wire signed [47:0] imm,
    output wire signed [47:0] imm2,
    output wire signed  [1:0] fill,
    output wire               fmt_long
);
    // Per-trit illegal 11 state detector across all 32 trits.
    wire [31:0] trit_bad;
    genvar gi;
    generate
        for (gi = 0; gi < 32; gi = gi + 1) begin : gen_bad
            assign trit_bad[gi] = instr[gi * 2 + 1] & instr[gi * 2];
        end
    endgenerate
    assign err_11 = |trit_bad;

    function automatic signed [1:0] trit1(input [1:0] c);
        begin
            case (c)
                2'b01:   trit1 = 2'sd1;
                2'b10:   trit1 = -2'sd1;
                default: trit1 = 2'sd0;
            endcase
        end
    endfunction

    function automatic signed [3:0] trit2(input [3:0] b);
        reg signed [3:0] t0, t1;
        begin
            t0 = trit1(b[1:0]);
            t1 = trit1(b[3:2]);
            trit2 = t0 + (t1 <<< 1) + t1;
        end
    endfunction

    function automatic signed [6:0] trit4(input [7:0] b);
        reg signed [6:0] lo, hi;
        begin
            lo = trit2(b[3:0]);
            hi = trit2(b[7:4]);
            // lo + 9 * hi = lo + (hi << 3) + hi
            trit4 = lo + (hi <<< 3) + hi;
        end
    endfunction

    function automatic signed [9:0] trit6(input [11:0] b);
        reg signed [9:0] p0, p1, p2;
        begin
            p0 = trit2(b[3:0]);
            p1 = trit2(b[7:4]);
            p2 = trit2(b[11:8]);
            // p0 + 9*p1 + 81*p2; 81 = 64 + 16 + 1
            trit6 = p0 + (p1 <<< 3) + p1 + (p2 <<< 6) + (p2 <<< 4) + p2;
        end
    endfunction

    // x * 729 where 729 = 512 + 128 + 64 + 16 + 8 + 1
    function automatic signed [19:0] mul729(input signed [9:0] x);
        reg signed [19:0] sx;
        begin
            sx = x;
            mul729 = (sx <<< 9) + (sx <<< 7) + (sx <<< 6) + (sx <<< 4) + (sx <<< 3) + sx;
        end
    endfunction

    // x * 531441 where 531441 = 524288 + 8192 - 1024 - 16 + 1
    function automatic signed [39:0] mul531441(input signed [19:0] x);
        reg signed [39:0] sx;
        begin
            sx = x;
            mul531441 = (sx <<< 19) + (sx <<< 13) - (sx <<< 10) - (sx <<< 4) + sx;
        end
    endfunction

    wire signed [1:0] t0   = trit1(instr[1:0]);
    wire signed [3:0] opc2 = trit2(instr[3:0]);
    wire signed [6:0] opc  = trit4(instr[7:0]);
    wire signed [6:0] func = trit4(instr[15:8]);

    wire signed [9:0] f26  = trit6(instr[63:52]);
    wire signed [9:0] f20  = trit6(instr[51:40]);
    wire signed [9:0] f14  = trit6(instr[39:28]);
    wire signed [9:0] f8   = trit6(instr[27:16]);
    wire signed [9:0] f2   = trit6(instr[15:4]);

    reg [6:0] op_r;
    reg       valid_r;
    reg       fmt_long_r;

    wire signed [7:0] opc8  = opc;
    wire signed [7:0] func8 = func;

    always @* begin
        op_r       = 7'd0;
        valid_r    = 1'b0;
        fmt_long_r = 1'b0;
        if (t0 != 2'sd0) begin
            fmt_long_r = 1'b1;
            case (opc2)
                4'sd4:  begin op_r = 7'd41; valid_r = 1'b1; end // lwa.t
                4'sd1:  begin op_r = 7'd42; valid_r = 1'b1; end // li.t
                -4'sd2: begin op_r = 7'd43; valid_r = 1'b1; end // swa.t
                4'sd2:  begin op_r = 7'd44; valid_r = 1'b1; end // jal.t
                -4'sd1: begin op_r = 7'd45; valid_r = 1'b1; end // aipc.t
                default: valid_r = 1'b0;
            endcase
        end else begin
            case ({opc8, func8})
                {-8'sd36, -8'sd4}: begin op_r = 7'd0;  valid_r = 1'b1; end // add.t
                {-8'sd36, -8'sd3}: begin op_r = 7'd1;  valid_r = 1'b1; end // sub.t
                {-8'sd36, -8'sd2}: begin op_r = 7'd2;  valid_r = 1'b1; end // sh.t
                {-8'sd36, -8'sd1}: begin op_r = 7'd3;  valid_r = 1'b1; end // rotr.t
                {-8'sd36,  8'sd0}: begin op_r = 7'd4;  valid_r = 1'b1; end // slt.t
                {-8'sd36,  8'sd1}: begin op_r = 7'd5;  valid_r = 1'b1; end // or.t
                {-8'sd36,  8'sd2}: begin op_r = 7'd6;  valid_r = 1'b1; end // xor.t
                {-8'sd36,  8'sd3}: begin op_r = 7'd7;  valid_r = 1'b1; end // and.t
                {-8'sd27, -8'sd4}: begin op_r = 7'd8;  valid_r = 1'b1; end // cmp.t
                {-8'sd27, -8'sd3}: begin op_r = 7'd9;  valid_r = 1'b1; end // sti.t
                {-8'sd27, -8'sd2}: begin op_r = 7'd10; valid_r = 1'b1; end // mv2.t
                {-8'sd27, -8'sd1}: begin op_r = 7'd11; valid_r = 1'b1; end // min.t
                {-8'sd27,  8'sd1}: begin op_r = 7'd12; valid_r = 1'b1; end // max.t
                { 8'sd0,   8'sd0}: begin op_r = 7'd13; valid_r = 1'b1; end // addi.t
                { 8'sd0,  -8'sd4}: begin op_r = 7'd14; valid_r = 1'b1; end // shi.t
                { 8'sd0,  -8'sd2}: begin op_r = 7'd15; valid_r = 1'b1; end // rot.t
                { 8'sd0,  -8'sd1}: begin op_r = 7'd16; valid_r = 1'b1; end // slti.t
                { 8'sd0,   8'sd1}: begin op_r = 7'd17; valid_r = 1'b1; end // ori.t
                { 8'sd0,   8'sd2}: begin op_r = 7'd18; valid_r = 1'b1; end // xori.t
                { 8'sd0,   8'sd3}: begin op_r = 7'd19; valid_r = 1'b1; end // andi.t
                {-8'sd18, -8'sd4}: begin op_r = 7'd20; valid_r = 1'b1; end // lw.t
                {-8'sd18, -8'sd3}: begin op_r = 7'd21; valid_r = 1'b1; end // lh.t
                {-8'sd18, -8'sd2}: begin op_r = 7'd22; valid_r = 1'b1; end // lt.t
                {-8'sd18, -8'sd1}: begin op_r = 7'd23; valid_r = 1'b1; end // jalr.t
                {-8'sd9,  -8'sd4}: begin op_r = 7'd24; valid_r = 1'b1; end // bcgs.t
                {-8'sd9,  -8'sd3}: begin op_r = 7'd25; valid_r = 1'b1; end // bceg.t
                {-8'sd9,  -8'sd2}: begin op_r = 7'd26; valid_r = 1'b1; end // beq.t
                {-8'sd9,  -8'sd1}: begin op_r = 7'd27; valid_r = 1'b1; end // bne.t
                {-8'sd9,   8'sd0}: begin op_r = 7'd28; valid_r = 1'b1; end // blt.t
                {-8'sd9,   8'sd1}: begin op_r = 7'd29; valid_r = 1'b1; end // bge.t
                { 8'sd9,  -8'sd4}: begin op_r = 7'd30; valid_r = 1'b1; end // sw.t
                { 8'sd9,  -8'sd3}: begin op_r = 7'd31; valid_r = 1'b1; end // sh.t
                { 8'sd9,  -8'sd2}: begin op_r = 7'd32; valid_r = 1'b1; end // st.t
                { 8'sd18, -8'sd4}: begin op_r = 7'd33; valid_r = 1'b1; end // majv.t
                { 8'sd18, -8'sd3}: begin op_r = 7'd34; valid_r = 1'b1; end // minv.t
                { 8'sd27, -8'sd4}: begin op_r = 7'd35; valid_r = 1'b1; end // li2.t
                { 8'sd36, -8'sd3}: begin op_r = 7'd36; valid_r = 1'b1; end // fence.t
                { 8'sd36, -8'sd2}: begin op_r = 7'd37; valid_r = 1'b1; end // wfi.t
                { 8'sd36,  8'sd2}: begin op_r = 7'd38; valid_r = 1'b1; end // tret.t
                { 8'sd36,  8'sd3}: begin op_r = 7'd39; valid_r = 1'b1; end // ebreak.t
                { 8'sd36,  8'sd4}: begin op_r = 7'd40; valid_r = 1'b1; end // ecall.t
                {-8'sd39, -8'sd4}: begin op_r = 7'd46; valid_r = 1'b1; end // add
                {-8'sd39, -8'sd3}: begin op_r = 7'd47; valid_r = 1'b1; end // sub
                {-8'sd39, -8'sd2}: begin op_r = 7'd48; valid_r = 1'b1; end // sll
                {-8'sd39, -8'sd1}: begin op_r = 7'd49; valid_r = 1'b1; end // srl
                {-8'sd39,  8'sd0}: begin op_r = 7'd50; valid_r = 1'b1; end // sra
                {-8'sd39,  8'sd1}: begin op_r = 7'd51; valid_r = 1'b1; end // sltu
                {-8'sd39,  8'sd2}: begin op_r = 7'd52; valid_r = 1'b1; end // or
                {-8'sd39,  8'sd3}: begin op_r = 7'd53; valid_r = 1'b1; end // xor
                {-8'sd39,  8'sd4}: begin op_r = 7'd54; valid_r = 1'b1; end // and
                {-8'sd3,  -8'sd4}: begin op_r = 7'd55; valid_r = 1'b1; end // addi
                {-8'sd3,  -8'sd3}: begin op_r = 7'd56; valid_r = 1'b1; end // slli
                {-8'sd3,  -8'sd2}: begin op_r = 7'd57; valid_r = 1'b1; end // srli
                {-8'sd3,  -8'sd1}: begin op_r = 7'd58; valid_r = 1'b1; end // srai
                {-8'sd3,   8'sd0}: begin op_r = 7'd59; valid_r = 1'b1; end // sltiu
                {-8'sd3,   8'sd1}: begin op_r = 7'd60; valid_r = 1'b1; end // ori
                {-8'sd3,   8'sd2}: begin op_r = 7'd61; valid_r = 1'b1; end // xori
                {-8'sd3,   8'sd3}: begin op_r = 7'd62; valid_r = 1'b1; end // andi
                {-8'sd21, -8'sd4}: begin op_r = 7'd63; valid_r = 1'b1; end // lw
                {-8'sd21, -8'sd3}: begin op_r = 7'd64; valid_r = 1'b1; end // lh
                {-8'sd21, -8'sd2}: begin op_r = 7'd65; valid_r = 1'b1; end // lb
                {-8'sd21, -8'sd1}: begin op_r = 7'd66; valid_r = 1'b1; end // lhu
                {-8'sd21,  8'sd0}: begin op_r = 7'd67; valid_r = 1'b1; end // lbu
                {-8'sd12, -8'sd4}: begin op_r = 7'd68; valid_r = 1'b1; end // bltu
                {-8'sd12, -8'sd3}: begin op_r = 7'd69; valid_r = 1'b1; end // bgeu
                { 8'sd6,  -8'sd4}: begin op_r = 7'd70; valid_r = 1'b1; end // sw
                { 8'sd6,  -8'sd3}: begin op_r = 7'd71; valid_r = 1'b1; end // sh
                { 8'sd6,  -8'sd2}: begin op_r = 7'd72; valid_r = 1'b1; end // sb
                default:           valid_r = 1'b0;
            endcase
        end
    end

    // Shared 12-trit and 24-trit composites (single mul531441 shared by G and Y).
    wire signed [19:0] hi6_sel  = (op_r == 7'd26 || op_r == 7'd27 || op_r == 7'd28 ||
                                   op_r == 7'd29 || op_r == 7'd30 || op_r == 7'd31 ||
                                   op_r == 7'd32 || op_r == 7'd68 || op_r == 7'd69 ||
                                   op_r == 7'd70 || op_r == 7'd71 || op_r == 7'd72) ? mul729(f14) : mul729(f20);
    wire signed [19:0] imm_12   = $signed({{10{f8[9]}}, f8}) + hi6_sel;

    wire signed [19:0] gy_lo12  = $signed({{10{f2[9]}}, f2}) + mul729(f8);
    wire signed [19:0] gy_hi12  = (op_r == 7'd43)
                                ? ($signed({{10{f14[9]}}, f14}) + mul729(f20))
                                : ($signed({{10{f20[9]}}, f20}) + mul729(f26));
    wire signed [39:0] imm_24   = $signed({{20{gy_lo12[19]}}, gy_lo12}) + mul531441(gy_hi12);

    reg signed  [9:0] rs1_r, rs2_r, rs3_r, rd1_r, rd2_r;
    reg signed [39:0] imm_r, imm2_r;
    reg signed  [1:0] fill_r;

    always @* begin
        rs1_r  = 10'sd0;
        rs2_r  = 10'sd0;
        rs3_r  = 10'sd0;
        rd1_r  = 10'sd0;
        rd2_r  = 10'sd0;
        imm_r  = 40'sd0;
        imm2_r = 40'sd0;
        fill_r = 2'sd0;

        if (fmt_long_r) begin
            imm_r = imm_24;
            if (op_r == 7'd43) begin
                rs1_r = f26;
            end else begin
                rd1_r = f14;
            end
        end else begin
            case (op_r)
                // Format R
                7'd0, 7'd1, 7'd2, 7'd3, 7'd4, 7'd5, 7'd6, 7'd7,
                7'd8, 7'd9, 7'd10, 7'd11, 7'd12,
                7'd36, 7'd37, 7'd38, 7'd39, 7'd40,
                7'd46, 7'd47, 7'd48, 7'd49, 7'd50, 7'd51, 7'd52, 7'd53, 7'd54: begin
                    rs1_r = f26;
                    rs2_r = f20;
                    rd1_r = f14;
                    if (op_r == 7'd2) begin
                        fill_r = f8[1:0];
                        rd2_r  = 10'sd0;
                    end else begin
                        rd2_r  = f8;
                    end
                end
                // Format I
                7'd13, 7'd14, 7'd15, 7'd16, 7'd17, 7'd18, 7'd19,
                7'd20, 7'd21, 7'd22, 7'd23,
                7'd55, 7'd56, 7'd57, 7'd58, 7'd59, 7'd60, 7'd61, 7'd62,
                7'd63, 7'd64, 7'd65, 7'd66, 7'd67: begin
                    rs1_r = f26;
                    rd1_r = f14;
                    if (op_r == 7'd14) begin
                        imm_r  = {{30{f8[9]}}, f8};
                        fill_r = f20[1:0];
                    end else begin
                        imm_r  = {{20{imm_12[19]}}, imm_12};
                    end
                end
                // Format B
                7'd24, 7'd25, 7'd26, 7'd27, 7'd28, 7'd29,
                7'd30, 7'd31, 7'd32,
                7'd68, 7'd69, 7'd70, 7'd71, 7'd72: begin
                    rs1_r = f26;
                    rs2_r = f20;
                    if (op_r == 7'd24 || op_r == 7'd25) begin
                        imm_r  = {{30{f14[9]}}, f14};
                        imm2_r = {{30{f8[9]}}, f8};
                    end else begin
                        imm_r  = {{20{imm_12[19]}}, imm_12};
                    end
                end
                // Format D
                7'd33, 7'd34: begin
                    rs1_r = f26;
                    rs2_r = f20;
                    rd1_r = f14;
                    rs3_r = f8;
                end
                // Format X (li2.t)
                7'd35: begin
                    imm_r  = {{30{f26[9]}}, f26};
                    imm2_r = {{30{f20[9]}}, f20};
                    rd1_r  = f14;
                    rd2_r  = f8;
                end
                default: ;
            endcase
        end
    end

    assign valid    = valid_r;
    assign op       = op_r;
    assign fmt_long = fmt_long_r;
    assign rs1      = rs1_r;
    assign rs2      = rs2_r;
    assign rs3      = rs3_r;
    assign rd1      = rd1_r;
    assign rd2      = rd2_r;
    assign imm      = {{8{imm_r[39]}}, imm_r};
    assign imm2     = {{8{imm2_r[39]}}, imm2_r};
    assign fill     = fill_r;
endmodule

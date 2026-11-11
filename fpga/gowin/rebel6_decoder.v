module rebel6_decoder (
    input wire [63:0] instr,
    output wire valid, output wire [6:0] op,
    output wire [5:0] rs1, rs2, rs3, rd1, rd2,
    output wire signed [47:0] imm, imm2,
    output wire signed [1:0] fill, output wire fmt_long
);
    function automatic signed [47:0] rebel6_field(input [63:0] word, input [4:0] off, input [4:0] width);
        integer i; reg signed [1:0] d; reg signed [47:0] acc; reg signed [47:0] wgt;
        begin
            acc = 48'sd0;
            for (i = 0; i < 24; i = i + 1) begin
                if (i < width) begin
                    case (word[(off + i) * 2 +: 2])
                        2'b01: d = 2'sd1; 2'b10: d = -2'sd1; default: d = 2'sd0;
                    endcase
                    case (i)
                        0: wgt = 48'd1;
                        1: wgt = 48'd3;
                        2: wgt = 48'd9;
                        3: wgt = 48'd27;
                        4: wgt = 48'd81;
                        5: wgt = 48'd243;
                        6: wgt = 48'd729;
                        7: wgt = 48'd2187;
                        8: wgt = 48'd6561;
                        9: wgt = 48'd19683;
                        10: wgt = 48'd59049;
                        11: wgt = 48'd177147;
                        12: wgt = 48'd531441;
                        13: wgt = 48'd1594323;
                        14: wgt = 48'd4782969;
                        15: wgt = 48'd14348907;
                        16: wgt = 48'd43046721;
                        17: wgt = 48'd129140163;
                        18: wgt = 48'd387420489;
                        19: wgt = 48'd1162261467;
                        20: wgt = 48'd3486784401;
                        21: wgt = 48'd10460353203;
                        22: wgt = 48'd31381059609;
                        23: wgt = 48'd94143178827;
                    endcase
                    acc = acc + d * wgt;
                end
            end
            rebel6_field = acc;
        end
    endfunction

    wire signed [1:0] t0;
    trit_read u_t0 (.code(instr[1:0]), .val(t0), .err());
    wire signed [7:0] opc = rebel6_field(instr, 5'd0, 5'd4);
    wire signed [7:0] func = rebel6_field(instr, 5'd4, 5'd4);
    wire signed [7:0] opc2 = rebel6_field(instr, 5'd0, 5'd2);
    reg [6:0] op_r; reg valid_r; reg fmt_long_r;
    always @* begin
        op_r = 7'd0; valid_r = 1'b0; fmt_long_r = 1'b0;
        if (t0 != 2'sd0) begin
            fmt_long_r = 1'b1;
            case (opc2)
                8'sd4: begin op_r = 7'd41; valid_r = 1'b1; end
                8'sd1: begin op_r = 7'd42; valid_r = 1'b1; end
                -8'sd2: begin op_r = 7'd43; valid_r = 1'b1; end
                8'sd2: begin op_r = 7'd44; valid_r = 1'b1; end
                -8'sd1: begin op_r = 7'd45; valid_r = 1'b1; end
                default: valid_r = 1'b0;
            endcase
        end else begin
            case ({opc, func})
                {-8'sd36, -8'sd4}: begin op_r = 7'd0; valid_r = 1'b1; end
                {-8'sd36, -8'sd3}: begin op_r = 7'd1; valid_r = 1'b1; end
                {-8'sd36, -8'sd2}: begin op_r = 7'd2; valid_r = 1'b1; end
                {-8'sd36, -8'sd1}: begin op_r = 7'd3; valid_r = 1'b1; end
                {-8'sd36, 8'sd0}: begin op_r = 7'd4; valid_r = 1'b1; end
                {-8'sd36, 8'sd1}: begin op_r = 7'd5; valid_r = 1'b1; end
                {-8'sd36, 8'sd2}: begin op_r = 7'd6; valid_r = 1'b1; end
                {-8'sd36, 8'sd3}: begin op_r = 7'd7; valid_r = 1'b1; end
                {-8'sd27, -8'sd4}: begin op_r = 7'd8; valid_r = 1'b1; end
                {-8'sd27, -8'sd3}: begin op_r = 7'd9; valid_r = 1'b1; end
                {-8'sd27, -8'sd2}: begin op_r = 7'd10; valid_r = 1'b1; end
                {-8'sd27, -8'sd1}: begin op_r = 7'd11; valid_r = 1'b1; end
                {-8'sd27, 8'sd1}: begin op_r = 7'd12; valid_r = 1'b1; end
                {8'sd0, 8'sd0}: begin op_r = 7'd13; valid_r = 1'b1; end
                {8'sd0, -8'sd4}: begin op_r = 7'd14; valid_r = 1'b1; end
                {8'sd0, -8'sd2}: begin op_r = 7'd15; valid_r = 1'b1; end
                {8'sd0, -8'sd1}: begin op_r = 7'd16; valid_r = 1'b1; end
                {8'sd0, 8'sd1}: begin op_r = 7'd17; valid_r = 1'b1; end
                {8'sd0, 8'sd2}: begin op_r = 7'd18; valid_r = 1'b1; end
                {8'sd0, 8'sd3}: begin op_r = 7'd19; valid_r = 1'b1; end
                {-8'sd18, -8'sd4}: begin op_r = 7'd20; valid_r = 1'b1; end
                {-8'sd18, -8'sd3}: begin op_r = 7'd21; valid_r = 1'b1; end
                {-8'sd18, -8'sd2}: begin op_r = 7'd22; valid_r = 1'b1; end
                {-8'sd18, -8'sd1}: begin op_r = 7'd23; valid_r = 1'b1; end
                {-8'sd9, -8'sd4}: begin op_r = 7'd24; valid_r = 1'b1; end
                {-8'sd9, -8'sd3}: begin op_r = 7'd25; valid_r = 1'b1; end
                {-8'sd9, -8'sd2}: begin op_r = 7'd26; valid_r = 1'b1; end
                {-8'sd9, -8'sd1}: begin op_r = 7'd27; valid_r = 1'b1; end
                {-8'sd9, 8'sd0}: begin op_r = 7'd28; valid_r = 1'b1; end
                {-8'sd9, 8'sd1}: begin op_r = 7'd29; valid_r = 1'b1; end
                {8'sd9, -8'sd4}: begin op_r = 7'd30; valid_r = 1'b1; end
                {8'sd9, -8'sd3}: begin op_r = 7'd31; valid_r = 1'b1; end
                {8'sd9, -8'sd2}: begin op_r = 7'd32; valid_r = 1'b1; end
                {8'sd18, -8'sd4}: begin op_r = 7'd33; valid_r = 1'b1; end
                {8'sd18, -8'sd3}: begin op_r = 7'd34; valid_r = 1'b1; end
                {8'sd27, -8'sd4}: begin op_r = 7'd35; valid_r = 1'b1; end
                {8'sd36, -8'sd3}: begin op_r = 7'd36; valid_r = 1'b1; end
                {8'sd36, -8'sd2}: begin op_r = 7'd37; valid_r = 1'b1; end
                {8'sd36, 8'sd2}: begin op_r = 7'd38; valid_r = 1'b1; end
                {8'sd36, 8'sd3}: begin op_r = 7'd39; valid_r = 1'b1; end
                {8'sd36, 8'sd4}: begin op_r = 7'd40; valid_r = 1'b1; end
                {-8'sd39, -8'sd4}: begin op_r = 7'd46; valid_r = 1'b1; end
                {-8'sd39, -8'sd3}: begin op_r = 7'd47; valid_r = 1'b1; end
                {-8'sd39, -8'sd2}: begin op_r = 7'd48; valid_r = 1'b1; end
                {-8'sd39, -8'sd1}: begin op_r = 7'd49; valid_r = 1'b1; end
                {-8'sd39, 8'sd0}: begin op_r = 7'd50; valid_r = 1'b1; end
                {-8'sd39, 8'sd1}: begin op_r = 7'd51; valid_r = 1'b1; end
                {-8'sd39, 8'sd2}: begin op_r = 7'd52; valid_r = 1'b1; end
                {-8'sd39, 8'sd3}: begin op_r = 7'd53; valid_r = 1'b1; end
                {-8'sd39, 8'sd4}: begin op_r = 7'd54; valid_r = 1'b1; end
                {-8'sd3, -8'sd4}: begin op_r = 7'd55; valid_r = 1'b1; end
                {-8'sd3, -8'sd3}: begin op_r = 7'd56; valid_r = 1'b1; end
                {-8'sd3, -8'sd2}: begin op_r = 7'd57; valid_r = 1'b1; end
                {-8'sd3, -8'sd1}: begin op_r = 7'd58; valid_r = 1'b1; end
                {-8'sd3, 8'sd0}: begin op_r = 7'd59; valid_r = 1'b1; end
                {-8'sd3, 8'sd1}: begin op_r = 7'd60; valid_r = 1'b1; end
                {-8'sd3, 8'sd2}: begin op_r = 7'd61; valid_r = 1'b1; end
                {-8'sd3, 8'sd3}: begin op_r = 7'd62; valid_r = 1'b1; end
                {-8'sd21, -8'sd4}: begin op_r = 7'd63; valid_r = 1'b1; end
                {-8'sd21, -8'sd3}: begin op_r = 7'd64; valid_r = 1'b1; end
                {-8'sd21, -8'sd2}: begin op_r = 7'd65; valid_r = 1'b1; end
                {-8'sd21, -8'sd1}: begin op_r = 7'd66; valid_r = 1'b1; end
                {-8'sd21, 8'sd0}: begin op_r = 7'd67; valid_r = 1'b1; end
                {-8'sd12, -8'sd4}: begin op_r = 7'd68; valid_r = 1'b1; end
                {-8'sd12, -8'sd3}: begin op_r = 7'd69; valid_r = 1'b1; end
                {8'sd6, -8'sd4}: begin op_r = 7'd70; valid_r = 1'b1; end
                {8'sd6, -8'sd3}: begin op_r = 7'd71; valid_r = 1'b1; end
                {8'sd6, -8'sd2}: begin op_r = 7'd72; valid_r = 1'b1; end
                default: valid_r = 1'b0;
            endcase
        end
    end
    assign op = op_r; assign valid = valid_r; assign fmt_long = fmt_long_r;

    wire signed [47:0] r1 = rebel6_field(instr, 5'd26, 5'd6);
    wire signed [47:0] r2 = rebel6_field(instr, 5'd20, 5'd6);
    wire signed [47:0] rd1f = rebel6_field(instr, 5'd14, 5'd6);
    wire signed [47:0] rd2f = rebel6_field(instr, 5'd8, 5'd6);
    wire signed [47:0] lo6 = rebel6_field(instr, 5'd8, 5'd6);
    wire signed [47:0] hi6 = rebel6_field(instr, 5'd20, 5'd6);
    wire signed [47:0] imm12lo = rebel6_field(instr, 5'd8, 5'd12);
    wire signed [47:0] imm12hi = rebel6_field(instr, 5'd20, 5'd12);
    wire signed [47:0] imm24 = rebel6_field(instr, 5'd2, 5'd24);
    wire signed [47:0] imm_i = lo6 + hi6 * 48'd729;
    wire signed [47:0] immGlo = rebel6_field(instr, 5'd2, 5'd12);
    wire signed [47:0] immGhi = rebel6_field(instr, 5'd20, 5'd12);
    wire signed [47:0] imm_g = immGlo + immGhi * 48'd531441;

    reg [5:0] rs1_r, rs2_r, rs3_r, rd1_r, rd2_r;
    reg signed [47:0] imm_r, imm2_r; reg signed [1:0] fill_r;
    always @* begin
        rs1_r = r1[5:0]; rs2_r = r2[5:0]; rs3_r = 6'd0;
        rd1_r = rd1f[5:0]; rd2_r = rd2f[5:0];
        imm_r = 48'sd0; imm2_r = 48'sd0; fill_r = 2'sd0;
        if (fmt_long_r) begin
            if (op_r == 7'd43) begin
                imm_r = imm24; rs1_r = r1[5:0];
                rd2_r = 6'd0; rs2_r = 6'd0; rs3_r = 6'd0;
            end else begin
                imm_r = imm_g; rd1_r = rd1f[5:0];
                rs2_r = 6'd0; rs3_r = 6'd0; rd2_r = 6'd0;
            end
        end else begin
            case (op_r)
                7'd0, 7'd1, 7'd2, 7'd3, 7'd4, 7'd5, 7'd6, 7'd7, 7'd8, 7'd9, 7'd10, 7'd11, 7'd12, 7'd46, 7'd47, 7'd48, 7'd49, 7'd50, 7'd51, 7'd52, 7'd53, 7'd54: begin
                    rs1_r = r1[5:0]; rs2_r = r2[5:0];
                    rd1_r = rd1f[5:0]; rd2_r = rd2f[5:0];
                    if (op_r == 7'd2) begin
                        fill_r = rd2f[47] ? 2'b10 : ((rd2f != 0) ? 2'b01 : 2'b00);
                        rd2_r = 6'd0;
                    end
                end
                7'd13, 7'd14, 7'd15, 7'd16, 7'd17, 7'd18, 7'd19, 7'd20, 7'd21, 7'd22, 7'd23, 7'd55, 7'd56, 7'd57, 7'd58, 7'd59, 7'd60, 7'd61, 7'd62, 7'd63, 7'd64, 7'd65, 7'd66, 7'd67: begin
                    rs1_r = r1[5:0]; rd1_r = rd1f[5:0];
                    rs2_r = 6'd0; rd2_r = 6'd0; rs3_r = 6'd0;
                    if (op_r == 7'd14) begin
                        imm_r = lo6; fill_r = hi6[47] ? 2'b10 : ((hi6 != 0) ? 2'b01 : 2'b00);
                    end else
                        imm_r = imm_i;
                end
                7'd24, 7'd25, 7'd26, 7'd27, 7'd28, 7'd29, 7'd30, 7'd31, 7'd32, 7'd68, 7'd69, 7'd70, 7'd71, 7'd72: begin
                    rs1_r = r1[5:0]; rs2_r = r2[5:0];
                    rd1_r = 6'd0; rd2_r = 6'd0; rs3_r = 6'd0;
                    if (op_r == 7'd24 || op_r == 7'd25) begin
                        imm2_r = rd2f; imm_r = rd1f;
                    end else
                        imm_r = imm12lo;
                end
                7'd33, 7'd34: begin
                    rs1_r = r1[5:0]; rs2_r = r2[5:0];
                    rs3_r = rd2f[5:0]; rd1_r = rd1f[5:0]; rd2_r = 6'd0;
                end
                7'd35: begin
                    imm_r = r1; imm2_r = r2;
                    rd1_r = rd1f[5:0]; rd2_r = rd2f[5:0];
                    rs1_r = 6'd0; rs2_r = 6'd0; rs3_r = 6'd0;
                end
            endcase
        end
    end
    assign rs1 = rs1_r; assign rs2 = rs2_r; assign rs3 = rs3_r;
    assign rd1 = rd1_r; assign rd2 = rd2_r;
    assign imm = imm_r; assign imm2 = imm2_r; assign fill = fill_r;
endmodule

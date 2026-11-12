// Multiplier-free sequential balanced-ternary unit for REBEL-6 tritwise,
// shift, rotate, majority and minority instructions:
//   mode 0: OR.T  / ORI.T   per-trit max(a, b)
//   mode 1: XOR.T / XORI.T  per-trit -(a * b)
//   mode 2: AND.T / ANDI.T  per-trit min(a, b)
//   mode 3: MAJV.T          per-trit median(a, b, c)
//   mode 4: MINV.T          per-trit -median(a, b, c)
//   mode 5: SH.T            rs1.shift(rs2.low(4), fill)
//   mode 6: ROTR.T          rs1.rotate(rs2.low(4))
//   mode 7: SHI.T           rs1.shift(shamt_imm, fill)
//   mode 8: ROT.T           rs1.rotate(shamt_imm)
//
// Decomposition identity: for V in [-HALF, +HALF], U = V + HALF is in
// [0, 3^24 - 1] with base-3 digits d_k = t_k + 1 in {0, 1, 2}. Comparing U
// against 3^k and 2*3^k from k=23 down to 0 extracts all 24 balanced trits
// without division, and recomposition uses acc <= acc*3 + t_k.
module rebel6_trit_unit (
    input  wire               clk,
    input  wire               rst,
    input  wire               start,
    input  wire         [3:0] mode,
    input  wire signed [39:0] op_a,
    input  wire signed [39:0] op_b,
    input  wire signed [39:0] op_c,
    input  wire signed [39:0] shamt_imm,
    input  wire signed  [1:0] fill,
    output reg  signed [39:0] result,
    output reg                done
);
    localparam [39:0] HALF = 40'd141214768240;

    function automatic [38:0] pow3(input [4:0] idx);
        begin
            case (idx)
                5'd0:  pow3 = 39'd1;
                5'd1:  pow3 = 39'd3;
                5'd2:  pow3 = 39'd9;
                5'd3:  pow3 = 39'd27;
                5'd4:  pow3 = 39'd81;
                5'd5:  pow3 = 39'd243;
                5'd6:  pow3 = 39'd729;
                5'd7:  pow3 = 39'd2187;
                5'd8:  pow3 = 39'd6561;
                5'd9:  pow3 = 39'd19683;
                5'd10: pow3 = 39'd59049;
                5'd11: pow3 = 39'd177147;
                5'd12: pow3 = 39'd531441;
                5'd13: pow3 = 39'd1594323;
                5'd14: pow3 = 39'd4782969;
                5'd15: pow3 = 39'd14348907;
                5'd16: pow3 = 39'd43046721;
                5'd17: pow3 = 39'd129140163;
                5'd18: pow3 = 39'd387420489;
                5'd19: pow3 = 39'd1162261467;
                5'd20: pow3 = 39'd3486784401;
                5'd21: pow3 = 39'd10460353203;
                5'd22: pow3 = 39'd31381059609;
                default: pow3 = 39'd94143178827;
            endcase
        end
    endfunction

    reg [1:0]  state;
    localparam ST_IDLE   = 2'd0;
    localparam ST_UNPACK = 2'd1;
    localparam ST_PREP   = 2'd2;
    localparam ST_PACK   = 2'd3;
    reg [4:0]  idx;
    reg [38:0] ua, ub, uc;
    reg [47:0] ta_vec, tb_vec, tc_vec; // 2 bits/trit: 00=0, 01=+1, 11=-1 (signed 2-bit)
    reg [3:0]  mode_r;
    reg signed [39:0] shamt_imm_r;
    reg signed [1:0]  fill_r;
    reg signed [39:0] acc;

    wire [38:0] p1 = pow3(idx);
    wire [38:0] p2 = {p1[37:0], 1'b0}; // 2 * 3^idx (since 2*3^23 = 188286357654 < 2^38)

    wire signed [1:0] da = (ua >= p2) ? 2'sd1 : ((ua >= p1) ? 2'sd0 : -2'sd1);
    wire [38:0]       na = (ua >= p2) ? (ua - p2) : ((ua >= p1) ? (ua - p1) : ua);

    wire signed [1:0] db = (ub >= p2) ? 2'sd1 : ((ub >= p1) ? 2'sd0 : -2'sd1);
    wire [38:0]       nb = (ub >= p2) ? (ub - p2) : ((ub >= p1) ? (ub - p1) : ub);

    wire signed [1:0] dc = (uc >= p2) ? 2'sd1 : ((uc >= p1) ? 2'sd0 : -2'sd1);
    wire [38:0]       nc = (uc >= p2) ? (uc - p2) : ((uc >= p1) ? (uc - p1) : uc);

    // Helper to read signed 2-bit trit at position k (0..23)
    function automatic signed [1:0] get_t(input [47:0] vec, input [4:0] k);
        begin
            get_t = vec[k * 2 +: 2];
        end
    endfunction

    // Compute rs2.low(4) from tb_vec[0..3]: in [-40, +40]
    wire signed [7:0] b_t0 = get_t(tb_vec, 5'd0);
    wire signed [7:0] b_t1 = get_t(tb_vec, 5'd1);
    wire signed [7:0] b_t2 = get_t(tb_vec, 5'd2);
    wire signed [7:0] b_t3 = get_t(tb_vec, 5'd3);
    wire signed [7:0] rs2_low4 = b_t0 + (b_t1 * 8'sd3) + (b_t2 * 8'sd9) + (b_t3 * 8'sd27);
    wire signed [31:0] sh_amt = (mode_r == 4'd5 || mode_r == 4'd6)
                              ? {{24{rs2_low4[7]}}, rs2_low4}
                              : shamt_imm_r[31:0];

    // Divider-free mod 24 from tb_vec trits (for mode 6: op_b=rs2; mode 8: op_b=shamt_imm).
    // Since 3^0 = 1, 3^(odd) == 3 (mod 24), 3^(even>=2) == 9 (mod 24):
    wire signed [9:0] rot_odd_sum =
        $signed(get_t(tb_vec, 5'd1))  + $signed(get_t(tb_vec, 5'd3))
      + ((mode_r == 4'd8) ? (
          $signed(get_t(tb_vec, 5'd5))  + $signed(get_t(tb_vec, 5'd7))
        + $signed(get_t(tb_vec, 5'd9))  + $signed(get_t(tb_vec, 5'd11))
        + $signed(get_t(tb_vec, 5'd13)) + $signed(get_t(tb_vec, 5'd15))
        + $signed(get_t(tb_vec, 5'd17)) + $signed(get_t(tb_vec, 5'd19))
        + $signed(get_t(tb_vec, 5'd21)) + $signed(get_t(tb_vec, 5'd23))
        ) : 10'sd0);
    wire signed [9:0] rot_even_sum =
        $signed(get_t(tb_vec, 5'd2))
      + ((mode_r == 4'd8) ? (
          $signed(get_t(tb_vec, 5'd4))  + $signed(get_t(tb_vec, 5'd6))
        + $signed(get_t(tb_vec, 5'd8))  + $signed(get_t(tb_vec, 5'd10))
        + $signed(get_t(tb_vec, 5'd12)) + $signed(get_t(tb_vec, 5'd14))
        + $signed(get_t(tb_vec, 5'd16)) + $signed(get_t(tb_vec, 5'd18))
        + $signed(get_t(tb_vec, 5'd20)) + $signed(get_t(tb_vec, 5'd22))
        ) : 10'sd0);
    wire signed [9:0] rot_raw = $signed(get_t(tb_vec, 5'd0))
                              + (rot_odd_sum * 10'sd3)
                              + (rot_even_sum * 10'sd9); // in [-145, +145]

    function automatic [4:0] mod24_small(input signed [9:0] x);
        reg signed [9:0] r;
        begin
            r = x;
            if (r >= 10'sd96)  r = r - 10'sd96;
            if (r >= 10'sd96)  r = r - 10'sd96;
            if (r >= 10'sd48)  r = r - 10'sd48;
            if (r >= 10'sd24)  r = r - 10'sd24;
            if (r <= -10'sd96) r = r + 10'sd96;
            if (r <= -10'sd96) r = r + 10'sd96;
            if (r <= -10'sd48) r = r + 10'sd48;
            if (r <= -10'sd24) r = r + 10'sd24;
            if (r < 10'sd0)    r = r + 10'sd24;
            mod24_small = r[4:0];
        end
    endfunction

    wire [4:0]        rot_k_comb = mod24_small(rot_raw);
    reg  [4:0]        rot_k_r;
    reg signed [31:0] sh_amt_r;
    // Output trit at position idx during ST_PACK (idx counts 23 down to 0)
    reg signed [1:0] out_t;
    reg signed [1:0] cur_a, cur_b, cur_c, med;
    reg signed [31:0] src_pos;
    reg [4:0] rot_src;

    always @* begin
        cur_a = get_t(ta_vec, idx);
        cur_b = get_t(tb_vec, idx);
        cur_c = get_t(tc_vec, idx);

        // Median of (cur_a, cur_b, cur_c)
        if ((cur_a >= cur_b && cur_a <= cur_c) || (cur_a <= cur_b && cur_a >= cur_c))
            med = cur_a;
        else if ((cur_b >= cur_a && cur_b <= cur_c) || (cur_b <= cur_a && cur_b >= cur_c))
            med = cur_b;
        else
            med = cur_c;

        src_pos = $signed({27'd0, idx}) - sh_amt_r;
        rot_src = (idx >= rot_k_r) ? (idx - rot_k_r) : (idx + 5'd24 - rot_k_r);

        case (mode_r)
            4'd0: out_t = (cur_a > cur_b) ? cur_a : cur_b; // OR.T
            4'd1: begin // XOR.T = -(a * b)
                if (cur_a == 2'sd0 || cur_b == 2'sd0)
                    out_t = 2'sd0;
                else if (cur_a == cur_b)
                    out_t = -2'sd1;
                else
                    out_t = 2'sd1;
            end
            4'd2: out_t = (cur_a < cur_b) ? cur_a : cur_b; // AND.T
            4'd3: out_t = med;                             // MAJV.T
            4'd4: out_t = -med;                            // MINV.T
            4'd5, 4'd7: begin                              // SH.T / SHI.T
                if (sh_amt_r == 32'sd0)
                    out_t = cur_a;
                else if (src_pos >= 32'sd0 && src_pos < 32'sd24)
                    out_t = get_t(ta_vec, src_pos[4:0]);
                else
                    out_t = fill_r;
            end
            4'd6, 4'd8: out_t = get_t(ta_vec, rot_src);    // ROTR.T / ROT.T
            default: out_t = 2'sd0;
        endcase
    end

    wire signed [39:0] next_acc = (acc <<< 1) + acc + {{38{out_t[1]}}, out_t};

    always @(posedge clk) begin
        if (rst) begin
            state  <= ST_IDLE;
            done   <= 1'b0;
            result <= 40'sd0;
            idx    <= 5'd23;
        end else begin
            done <= 1'b0;
            case (state)
                ST_IDLE: begin
                    if (start) begin
                        ua          <= $unsigned($signed(op_a) + $signed(HALF));
                        ub          <= $unsigned($signed(op_b) + $signed(HALF));
                        uc          <= $unsigned($signed(op_c) + $signed(HALF));
                        mode_r      <= mode;
                        shamt_imm_r <= shamt_imm;
                        fill_r      <= fill;
                        idx         <= 5'd23;
                        state       <= ST_UNPACK;
                    end
                end
                ST_UNPACK: begin
                    ta_vec[idx * 2 +: 2] <= da;
                    tb_vec[idx * 2 +: 2] <= db;
                    tc_vec[idx * 2 +: 2] <= dc;
                    ua <= na;
                    ub <= nb;
                    uc <= nc;
                    if (idx == 5'd0) begin
                        state <= ST_PREP;
                    end else begin
                        idx <= idx - 5'd1;
                    end
                end
                ST_PREP: begin
                    rot_k_r  <= rot_k_comb;
                    sh_amt_r <= sh_amt;
                    idx      <= 5'd23;
                    acc      <= 40'sd0;
                    state    <= ST_PACK;
                end
                ST_PACK: begin
                    if (idx == 5'd0) begin
                        result <= next_acc;
                        done   <= 1'b1;
                        state  <= ST_IDLE;
                    end else begin
                        acc <= next_acc;
                        idx <= idx - 5'd1;
                    end
                end
                default: state <= ST_IDLE;
            endcase
        end
    end
endmodule

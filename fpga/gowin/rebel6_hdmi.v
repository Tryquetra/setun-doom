// HDMI / DVI-D Video Pipeline for Sipeed Tang Nano 20K (640x480p @ 60.00 Hz).
//
// Architecture:
//   - Dual-Port 64 KiB Framebuffer BRAM (320x200x8 = 64,000 bytes, 32 DPX9B)
//     written on clk_sys by SYS_BLIT DMA, scanned out tear-free on clk_pix.
//   - Dual-Port 256x24-bit Palette BRAM (1 DPX9B) written on clk_sys by
//     SYS_PALETTE DMA.
//   - Exact 4:3 CRT aspect scaler (320 -> 640 2x horizontal, 200 -> 480 12:5
//     integer cadence vertical, zero multipliers/dividers).
//   - Instant-on Balanced Ternary boot screen while SDRAM/WAD loads before the
//     first rendered Doom frame arrives, switching automatically on has_frame.
//   - 3x DC-balanced 8b/10b TMDS encoders + 4x Gowin OSER10 + 4x TLVDS_OBUF
//     (in simulation/Verilator, SIM_ONLY=1 omits vendor SERDES primitives).
module tmds_encoder (
    input  wire       clk,
    input  wire       rst,
    input  wire [7:0] vd,
    input  wire [1:0] cd,
    input  wire       vde,
    output reg  [9:0] tmds
);
    wire [3:0] n1d = vd[0] + vd[1] + vd[2] + vd[3] + vd[4] + vd[5] + vd[6] + vd[7];
    wire       xnor_mode = (n1d > 4'd4) || (n1d == 4'd4 && vd[0] == 1'b0);

    wire [8:0] q_m;
    assign q_m[0] = vd[0];
    assign q_m[1] = xnor_mode ? ~(q_m[0] ^ vd[1]) : (q_m[0] ^ vd[1]);
    assign q_m[2] = xnor_mode ? ~(q_m[1] ^ vd[2]) : (q_m[1] ^ vd[2]);
    assign q_m[3] = xnor_mode ? ~(q_m[2] ^ vd[3]) : (q_m[2] ^ vd[3]);
    assign q_m[4] = xnor_mode ? ~(q_m[3] ^ vd[4]) : (q_m[3] ^ vd[4]);
    assign q_m[5] = xnor_mode ? ~(q_m[4] ^ vd[5]) : (q_m[4] ^ vd[5]);
    assign q_m[6] = xnor_mode ? ~(q_m[5] ^ vd[6]) : (q_m[5] ^ vd[6]);
    assign q_m[7] = xnor_mode ? ~(q_m[6] ^ vd[7]) : (q_m[6] ^ vd[7]);
    assign q_m[8] = ~xnor_mode;

    wire [3:0] n1q = q_m[0] + q_m[1] + q_m[2] + q_m[3] + q_m[4] + q_m[5] + q_m[6] + q_m[7];
    wire signed [4:0] diff_q = $signed({1'b0, n1q, 1'b0}) - 5'sd8; // 2*n1q - 8

    reg signed [4:0] dc_bias;

    always @(posedge clk) begin
        if (rst) begin
            tmds    <= 10'b1101010100;
            dc_bias <= 5'sd0;
        end else if (!vde) begin
            dc_bias <= 5'sd0;
            case (cd)
                2'b00: tmds <= 10'b1101010100;
                2'b01: tmds <= 10'b0010101011;
                2'b10: tmds <= 10'b0101010100;
                2'b11: tmds <= 10'b1010101011;
            endcase
        end else begin
            if (dc_bias == 5'sd0 || diff_q == 5'sd0) begin
                tmds[9]   <= ~q_m[8];
                tmds[8]   <= q_m[8];
                tmds[7:0] <= q_m[8] ? q_m[7:0] : ~q_m[7:0];
                dc_bias   <= q_m[8] ? (dc_bias + diff_q) : (dc_bias - diff_q);
            end else if ((dc_bias > 5'sd0 && diff_q > 5'sd0) ||
                         (dc_bias < 5'sd0 && diff_q < 5'sd0)) begin
                tmds[9]   <= 1'b1;
                tmds[8]   <= q_m[8];
                tmds[7:0] <= ~q_m[7:0];
                dc_bias   <= dc_bias + (q_m[8] ? 5'sd2 : 5'sd0) - diff_q;
            end else begin
                tmds[9]   <= 1'b0;
                tmds[8]   <= q_m[8];
                tmds[7:0] <= q_m[7:0];
                dc_bias   <= dc_bias - (q_m[8] ? 5'sd0 : 5'sd2) + diff_q;
            end
        end
    end
endmodule

module rebel6_hdmi #(
    parameter SIM_ONLY = 0
) (
    // System domain (writes framebuffer & palette)
    input  wire        clk_sys,
    input  wire        fb_we,
    input  wire [15:0] fb_waddr,
    input  wire  [7:0] fb_wdata,
    input  wire [15:0] fb_sys_raddr,
    output reg   [7:0] fb_sys_rdata,
    input  wire        pal_we,
    input  wire  [7:0] pal_waddr,
    input  wire [23:0] pal_wdata,
    input  wire        has_frame,
    input  wire  [9:0] boot_progress,

    // Video pixel (25.2 MHz) and 5x DDR serial (126.0 MHz) domains
    input  wire        clk_pix,
    input  wire        clk_tmds,
    input  wire        rst_pix,

    // Simulation inspection outputs
    output wire  [7:0] sim_r,
    output wire  [7:0] sim_g,
    output wire  [7:0] sim_b,
    output wire        sim_de,
    output wire        sim_hs,
    output wire        sim_vs,

    // Physical HDMI differential pairs
    output wire        O_tmds_clk_p,
    output wire        O_tmds_clk_n,
    output wire  [2:0] O_tmds_data_p,
    output wire  [2:0] O_tmds_data_n
);
    // Dual-port 64 KiB Framebuffer BRAM (32 x DPX9B)
    // Port A (clk_sys) is shared: writes from SYS_BLIT/palette-mirror go to
    // fb_waddr when fb_we=1; the live video streamer (rebel6_soc_host.v) reads
    // fb_raddr when fb_we=0. Port B (clk_pix) is the tear-free HDMI scanout.
    (* ram_style = "block" *)
    reg [7:0] fb_mem [0:65535];
    reg [7:0] fb_pix_idx;
    wire [15:0] fb_aaddr = fb_we ? fb_waddr : fb_sys_raddr;

    always @(posedge clk_sys) begin
        if (fb_we)
            fb_mem[fb_aaddr] <= fb_wdata;
        else
            fb_sys_rdata <= fb_mem[fb_aaddr];
    end

    // Dual-port 256 x 24-bit Palette BRAM (1 x DPX9B)
    (* ram_style = "block" *)
    reg [23:0] pal_mem [0:255];
    reg [23:0] pal_rgb;

    always @(posedge clk_sys) begin
        if (pal_we)
            pal_mem[pal_waddr] <= pal_wdata;
    end

    // Synchronize status flags into clk_pix domain
    reg has_frame_s0, has_frame_pix;
    reg [9:0] prog_s0, prog_pix;
    always @(posedge clk_pix) begin
        has_frame_s0  <= has_frame;
        has_frame_pix <= has_frame_s0;
        prog_s0       <= boot_progress;
        prog_pix      <= prog_s0;
    end

    // 640x480 @ 60 Hz timing generator (800 x 525)
    reg [9:0] h_cnt;
    reg [9:0] v_cnt;
    reg [3:0] v_mod12;
    reg [7:0] src_y;

    always @(posedge clk_pix) begin
        if (rst_pix) begin
            h_cnt   <= 10'd0;
            v_cnt   <= 10'd0;
            v_mod12 <= 4'd0;
            src_y   <= 8'd0;
        end else begin
            if (h_cnt == 10'd799) begin
                h_cnt <= 10'd0;
                if (v_cnt == 10'd524) begin
                    v_cnt   <= 10'd0;
                    v_mod12 <= 4'd0;
                    src_y   <= 8'd0;
                end else begin
                    v_cnt <= v_cnt + 10'd1;
                    if (v_cnt < 10'd479) begin
                        // 12:5 cadence maps 480 output lines -> 200 Doom lines (exact 4:3 CRT)
                        if (v_mod12 == 4'd11)
                            v_mod12 <= 4'd0;
                        else
                            v_mod12 <= v_mod12 + 4'd1;

                        if (v_mod12 == 4'd1 || v_mod12 == 4'd3 || v_mod12 == 4'd6 ||
                            v_mod12 == 4'd8 || v_mod12 == 4'd11) begin
                            src_y <= src_y + 8'd1;
                        end
                    end
                end
            end else begin
                h_cnt <= h_cnt + 10'd1;
            end
        end
    end

    // Stage 0 -> Stage 1: Framebuffer read (src_y * 320 + src_x)
    wire [8:0]  src_x    = h_cnt[9:1];
    wire [15:0] fb_raddr = ({8'd0, src_y} << 8) + ({8'd0, src_y} << 6) + {7'd0, src_x};

    reg        de_s1, hs_s1, vs_s1;
    reg  [9:0] hx_s1, vy_s1;
    always @(posedge clk_pix) begin
        fb_pix_idx <= fb_mem[fb_raddr];
        de_s1      <= (h_cnt < 10'd640) && (v_cnt < 10'd480);
        hs_s1      <= ~((h_cnt >= 10'd656) && (h_cnt < 10'd752)); // negative polarity
        vs_s1      <= ~((v_cnt >= 10'd490) && (v_cnt < 10'd492)); // negative polarity
        hx_s1      <= h_cnt;
        vy_s1      <= v_cnt;
    end

    // Stage 1 -> Stage 2: Palette lookup + boot screen overlay when !has_frame_pix
    reg        de_s2, hs_s2, vs_s2;
    reg  [9:0] hx_s2, vy_s2;
    always @(posedge clk_pix) begin
        pal_rgb <= pal_mem[fb_pix_idx];
        de_s2   <= de_s1;
        hs_s2   <= hs_s1;
        vs_s2   <= vs_s1;
        hx_s2   <= hx_s1;
        vy_s2   <= vy_s1;
    end

    reg [7:0] out_r, out_g, out_b;
    always @* begin
        if (has_frame_pix) begin
            out_r = pal_rgb[23:16];
            out_g = pal_rgb[15:8];
            out_b = pal_rgb[7:0];
        end else begin
            // Setun/REBEL-6 ternary boot screen:
            // Top third: +1 Green, Middle third: 0 Slate, Bottom third: -1 Amber,
            // with live SDRAM/Flash boot progress bar across y = 228..252.
            if (vy_s2 >= 10'd228 && vy_s2 < 10'd252 && hx_s2 >= 10'd64 && hx_s2 < 10'd576) begin
                if ((hx_s2 - 10'd64) < {prog_pix[8:0], 1'b0}) begin
                    out_r = 8'h30; out_g = 8'hFF; out_b = 8'h60;
                end else begin
                    out_r = 8'h18; out_g = 8'h18; out_b = 8'h20;
                end
            end else if (vy_s2 < 10'd160) begin
                out_r = 8'h10; out_g = 8'h58; out_b = 8'h28;
            end else if (vy_s2 < 10'd320) begin
                out_r = 8'h22; out_g = 8'h26; out_b = 8'h30;
            end else begin
                out_r = 8'h68; out_g = 8'h3C; out_b = 8'h0C;
            end
        end
    end

    assign sim_r  = out_r;
    assign sim_g  = out_g;
    assign sim_b  = out_b;
    assign sim_de = de_s2;
    assign sim_hs = hs_s2;
    assign sim_vs = vs_s2;

    wire [9:0] tmds_b, tmds_g, tmds_r;
    tmds_encoder enc_b (.clk(clk_pix), .rst(rst_pix), .vd(out_b), .cd({vs_s2, hs_s2}), .vde(de_s2), .tmds(tmds_b));
    tmds_encoder enc_g (.clk(clk_pix), .rst(rst_pix), .vd(out_g), .cd(2'b00),          .vde(de_s2), .tmds(tmds_g));
    tmds_encoder enc_r (.clk(clk_pix), .rst(rst_pix), .vd(out_r), .cd(2'b00),          .vde(de_s2), .tmds(tmds_r));

    generate
        if (SIM_ONLY == 0) begin : gen_gowin_phy
            wire ser_b, ser_g, ser_r, ser_c;

            OSER10 #(.GSREN("false"), .LSREN("true")) u_ser_b (
                .Q(ser_b), .D0(tmds_b[0]), .D1(tmds_b[1]), .D2(tmds_b[2]), .D3(tmds_b[3]), .D4(tmds_b[4]),
                .D5(tmds_b[5]), .D6(tmds_b[6]), .D7(tmds_b[7]), .D8(tmds_b[8]), .D9(tmds_b[9]),
                .PCLK(clk_pix), .FCLK(clk_tmds), .RESET(rst_pix)
            );
            OSER10 #(.GSREN("false"), .LSREN("true")) u_ser_g (
                .Q(ser_g), .D0(tmds_g[0]), .D1(tmds_g[1]), .D2(tmds_g[2]), .D3(tmds_g[3]), .D4(tmds_g[4]),
                .D5(tmds_g[5]), .D6(tmds_g[6]), .D7(tmds_g[7]), .D8(tmds_g[8]), .D9(tmds_g[9]),
                .PCLK(clk_pix), .FCLK(clk_tmds), .RESET(rst_pix)
            );
            OSER10 #(.GSREN("false"), .LSREN("true")) u_ser_r (
                .Q(ser_r), .D0(tmds_r[0]), .D1(tmds_r[1]), .D2(tmds_r[2]), .D3(tmds_r[3]), .D4(tmds_r[4]),
                .D5(tmds_r[5]), .D6(tmds_r[6]), .D7(tmds_r[7]), .D8(tmds_r[8]), .D9(tmds_r[9]),
                .PCLK(clk_pix), .FCLK(clk_tmds), .RESET(rst_pix)
            );
            OSER10 #(.GSREN("false"), .LSREN("true")) u_ser_c (
                .Q(ser_c), .D0(1'b1), .D1(1'b1), .D2(1'b1), .D3(1'b1), .D4(1'b1),
                .D5(1'b0), .D6(1'b0), .D7(1'b0), .D8(1'b0), .D9(1'b0),
                .PCLK(clk_pix), .FCLK(clk_tmds), .RESET(rst_pix)
            );

            TLVDS_OBUF u_obuf_c (.I(ser_c), .O(O_tmds_clk_p),     .OB(O_tmds_clk_n));
            TLVDS_OBUF u_obuf_0 (.I(ser_b), .O(O_tmds_data_p[0]), .OB(O_tmds_data_n[0]));
            TLVDS_OBUF u_obuf_1 (.I(ser_g), .O(O_tmds_data_p[1]), .OB(O_tmds_data_n[1]));
            TLVDS_OBUF u_obuf_2 (.I(ser_r), .O(O_tmds_data_p[2]), .OB(O_tmds_data_n[2]));
        end else begin : gen_sim_phy
            assign O_tmds_clk_p  = clk_pix;
            assign O_tmds_clk_n  = ~clk_pix;
            assign O_tmds_data_p = 3'b000;
            assign O_tmds_data_n = 3'b111;
        end
    endgenerate
endmodule

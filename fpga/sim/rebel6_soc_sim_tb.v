// Full-SoC Simulation Wrapper for Verilator:
// Instantiates rebel6_soc_top(SIM_ONLY=1) alongside cycle-accurate behavioral
// models of the onboard 8 MiB SPI NOR Flash (XT25F64B) and the embedded
// 64 Mbit (8 MiB = 2M x 32-bit) SDR SDRAM, exposing UART TX and HDMI RGB scanout.
module sim_sdram_model (
    input  wire        clk,
    input  wire        cke,
    input  wire        cs_n,
    input  wire        ras_n,
    input  wire        cas_n,
    input  wire        wen_n,
    input  wire  [3:0] dqm,
    input  wire [10:0] addr,
    input  wire  [1:0] ba,
    input  wire [31:0] dq_in,
    output reg  [31:0] dq_out,
    output reg         dq_oe
);
    // 4 banks x 2048 rows x 256 cols = 2,097,152 x 32-bit words (8 MiB)
    reg [31:0] mem [0:2097151];
    reg [10:0] active_row [0:3];
    reg        row_open   [0:3];

    // CAS=2 read pipeline
    reg        rd_p1;
    reg [31:0] rd_data_p1;

    wire [2:0] cmd = cs_n ? 3'b111 : {ras_n, cas_n, wen_n};
    wire [20:0] full_waddr = {ba, active_row[ba], addr[7:0]};

    always @(posedge clk) begin
        dq_oe <= rd_p1;
        dq_out <= rd_data_p1;
        rd_p1 <= 1'b0;

        if (cke) begin
            case (cmd)
                3'b011: begin // ACTIVATE
                    active_row[ba] <= addr;
                    row_open[ba]   <= 1'b1;
                end
                3'b101: begin // READ
                    rd_p1      <= 1'b1;
                    rd_data_p1 <= mem[full_waddr];
                    if (addr[10]) row_open[ba] <= 1'b0;
                end
                3'b100: begin // WRITE
                    if (!dqm[0]) mem[full_waddr][7:0]   <= dq_in[7:0];
                    if (!dqm[1]) mem[full_waddr][15:8]  <= dq_in[15:8];
                    if (!dqm[2]) mem[full_waddr][23:16] <= dq_in[23:16];
                    if (!dqm[3]) mem[full_waddr][31:24] <= dq_in[31:24];
                    if (addr[10]) row_open[ba] <= 1'b0;
                end
                3'b010: begin // PRECHARGE
                    if (addr[10]) begin
                        row_open[0] <= 1'b0; row_open[1] <= 1'b0;
                        row_open[2] <= 1'b0; row_open[3] <= 1'b0;
                    end else begin
                        row_open[ba] <= 1'b0;
                    end
                end
                default: ;
            endcase
        end
    end
endmodule

module rebel6_soc_sim_tb (
    input  wire       clk,
    input  wire [1:0] btn,
    output wire [5:0] led_n,
    output wire       uart_tx,
    input  wire       uart_rx,

    // Exposed SPI pins driven by C++ 8 MiB flash array
    output wire       flash_cs_n,
    output wire       flash_clk,
    output wire       flash_mosi,
    input  wire       flash_miso,

    // Exposed HDMI 640x480 scanout tap + frame counter for fast Verification
    output wire [7:0] hdmi_r,
    output wire [7:0] hdmi_g,
    output wire [7:0] hdmi_b,
    output wire       hdmi_de,
    output wire       hdmi_hs,
    output wire       hdmi_vs,
    output wire [15:0] dbg_frames,
    output wire       dbg_has_frame,
    output wire       dbg_trapped,
    output wire       dbg_boot_err
);
    wire        sdram_clk, sdram_cke, sdram_cs_n, sdram_cas_n, sdram_ras_n, sdram_wen_n;
    wire  [3:0] sdram_dqm;
    wire [10:0] sdram_addr;
    wire  [1:0] sdram_ba;
    wire [31:0] sdram_dq;
    wire [31:0] mdl_dq_out;
    wire        mdl_dq_oe;

    assign sdram_dq = mdl_dq_oe ? mdl_dq_out : 32'hZZZZ_ZZZZ;

    rebel6_soc_top #(
        .SIM_ONLY(1),
        .SYS_FREQ(27_000_000)
    ) u_soc (
        .I_clk(clk),
        .I_btn(btn),
        .O_led_n(led_n),
        .O_uart_tx(uart_tx),
        .I_uart_rx(uart_rx),
        .O_flash_cs_n(flash_cs_n),
        .O_flash_clk(flash_clk),
        .O_flash_mosi(flash_mosi),
        .I_flash_miso(flash_miso),
        .O_tmds_clk_p(),
        .O_tmds_clk_n(),
        .O_tmds_data_p(),
        .O_tmds_data_n(),
        .O_sdram_clk(sdram_clk),
        .O_sdram_cke(sdram_cke),
        .O_sdram_cs_n(sdram_cs_n),
        .O_sdram_cas_n(sdram_cas_n),
        .O_sdram_ras_n(sdram_ras_n),
        .O_sdram_wen_n(sdram_wen_n),
        .O_sdram_dqm(sdram_dqm),
        .O_sdram_addr(sdram_addr),
        .O_sdram_ba(sdram_ba),
        .IO_sdram_dq(sdram_dq)
    );

    sim_sdram_model u_sdram_mdl (
        .clk(sdram_clk),
        .cke(sdram_cke),
        .cs_n(sdram_cs_n),
        .ras_n(sdram_ras_n),
        .cas_n(sdram_cas_n),
        .wen_n(sdram_wen_n),
        .dqm(sdram_dqm),
        .addr(sdram_addr),
        .ba(sdram_ba),
        .dq_in(u_soc.u_sdram.dq_out),
        .dq_out(mdl_dq_out),
        .dq_oe(mdl_dq_oe)
    );

    assign hdmi_r        = u_soc.u_hdmi.sim_r;
    assign hdmi_g        = u_soc.u_hdmi.sim_g;
    assign hdmi_b        = u_soc.u_hdmi.sim_b;
    assign hdmi_de       = u_soc.u_hdmi.sim_de;
    assign hdmi_hs       = u_soc.u_hdmi.sim_hs;
    assign hdmi_vs       = u_soc.u_hdmi.sim_vs;
    assign dbg_frames    = u_soc.frame_count;
    assign dbg_has_frame = u_soc.has_frame;
    assign dbg_trapped   = u_soc.cpu_trapped;
    assign dbg_boot_err  = u_soc.boot_err;
endmodule

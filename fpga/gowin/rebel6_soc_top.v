// Top-Level REBEL-6 Doom System-on-Chip for Sipeed Tang Nano 20K (GW2AR-LV18QN88C8/I7).
//
// Integrates:
//   - Video rPLL (27.0 MHz -> 126.0 MHz TMDS serial + CLKDIV /5 -> 25.2 MHz pixel)
//   - System clock (27.0 MHz crystal + inverted 180-deg SDRAM clock for rock-solid
//     18.5 ns setup/hold margin and single-PLL placement)
//   - 73-instruction REBEL-6 ternary CPU core + 8 KiB I-Cache + 2 KiB D-Cache
//   - 32-bit 8 MiB embedded SDRAM controller
//   - Autonomous SPI NOR Flash bootloader & DOOM1.WAD DMA engine + 115200 UART
//   - 640x480p@60Hz HDMI/TMDS video engine with 64 KiB dual-port framebuffer BRAM
module rebel6_soc_top #(
    parameter SIM_ONLY = 0,
    parameter integer SYS_FREQ = 21_000_000
) (
    input  wire        I_clk,       // 27 MHz crystal (pin 4)
    input  wire  [1:0] I_btn,       // Onboard buttons S1 (88), S2 (87) - active high on Tang Nano 20K
    output wire  [5:0] O_led_n,     // 6 active-low onboard LEDs (pins 15..20)

    // Onboard BL616 USB-UART (pins 69 TX, 70 RX)
    output wire        O_uart_tx,
    input  wire        I_uart_rx,

    // Onboard 8 MiB SPI NOR Flash XT25F64B (pins 59..62, enabled via --mspi_as_gpio)
    output wire        O_flash_cs_n,
    output wire        O_flash_clk,
    output wire        O_flash_mosi,
    input  wire        I_flash_miso,

    // HDMI TMDS output (pins 33..40)
    output wire        O_tmds_clk_p,
    output wire        O_tmds_clk_n,
    output wire  [2:0] O_tmds_data_p,
    output wire  [2:0] O_tmds_data_n,

    // Embedded 64 Mbit (8 MiB) SiP SDR SDRAM magic ports
    output wire        O_sdram_clk,
    output wire        O_sdram_cke,
    output wire        O_sdram_cs_n,
    output wire        O_sdram_cas_n,
    output wire        O_sdram_ras_n,
    output wire        O_sdram_wen_n,
    output wire  [3:0] O_sdram_dqm,
    output wire [10:0] O_sdram_addr,
    output wire  [1:0] O_sdram_ba,
    inout  wire [31:0] IO_sdram_dq
);
    // -------------------------------------------------------------------------
    // Clocks & Power-on Reset
    // -------------------------------------------------------------------------
    wire clk_sys;
    wire clk_sdram = ~clk_sys;

    wire clk_tmds;
    wire clk_pix;
    wire pll_lock;

    generate
        if (SIM_ONLY == 0) begin : gen_clks
            rPLL #(
                .FCLKIN("27.0"),
                .IDIV_SEL(2),
                .FBDIV_SEL(13),
                .ODIV_SEL(4),
                .CLKOUTD_BYPASS("false"),
                .CLKOUTD_SRC("CLKOUT"),
                .DYN_SDIV_SEL(6)
            ) u_pll_hdmi (
                .CLKIN(I_clk),
                .CLKFB(1'b0),
                .RESET(1'b0),
                .RESET_P(1'b0),
                .FBDSEL(6'b0),
                .IDSEL(6'b0),
                .ODSEL(6'b0),
                .PSDA(4'b0),
                .DUTYDA(4'b0),
                .FDLY(4'b0),
                .CLKOUT(clk_tmds),
                .LOCK(pll_lock),
                .CLKOUTP(),
                .CLKOUTD(clk_sys),
                .CLKOUTD3()
            );

            CLKDIV #(
                .DIV_MODE("5"),
                .GSREN("false")
            ) u_div5 (
                .HCLKIN(clk_tmds),
                .RESETN(pll_lock),
                .CALIB(1'b0),
                .CLKOUT(clk_pix)
            );
        end else begin : gen_sim_clks
            assign clk_sys  = I_clk;
            assign clk_tmds = I_clk;
            assign clk_pix  = I_clk;
            assign pll_lock = 1'b1;
        end
    endgenerate

    // Clean power-on reset stretcher (~2.4 ms at 27 MHz; short in SIM_ONLY)
    reg [15:0] por_cnt = 16'd0;
    wire       por_done = (SIM_ONLY != 0) ? (por_cnt >= 16'd8) : (&por_cnt);
    always @(posedge clk_sys) begin
        if (!por_done)
            por_cnt <= por_cnt + 16'd1;
    end
    wire rst_sys = ~por_done;

    reg rst_pix_s0 = 1'b1, rst_pix = 1'b1;
    always @(posedge clk_pix) begin
        rst_pix_s0 <= (~por_done) | (~pll_lock);
        rst_pix    <= rst_pix_s0;
    end

    // -------------------------------------------------------------------------
    // Interconnect Wires
    // -------------------------------------------------------------------------
    wire               cpu_start_pulse;
    wire signed [39:0] cpu_start_pc;
    wire        [31:0] cpu_instr_count;
    wire               host_rf_we;
    wire         [9:0] host_rf_waddr;
    wire signed [39:0] host_rf_wdata;

    wire               imem_req;
    wire        [19:0] imem_addr;
    wire        [63:0] imem_rdata;
    wire               imem_ready;

    wire               dmem_req;
    wire               dmem_we;
    wire        [23:0] dmem_addr;
    wire         [3:0] dmem_wstrb;
    wire        [31:0] dmem_wdata;
    wire        [31:0] dmem_rdata;
    wire               dmem_ready;

    wire               ecall_req;
    wire signed [39:0] ecall_a7;
    wire        [31:0] ecall_a0, ecall_a1, ecall_a2;
    wire               ecall_done;
    wire               ecall_write_a0;
    wire signed [39:0] ecall_ret_a0;
    wire               ecall_halt;
    wire signed [39:0] ecall_exit_code;

    wire               cpu_running, cpu_halted, cpu_trapped;
    wire         [2:0] cpu_trap_cause;
    wire signed [39:0] cpu_pc;
    wire        [63:0] cpu_cycles;
    wire signed [39:0] cpu_exit_code;

    wire               cache_flush_req;
    wire               icache_flush_busy, dcache_flush_busy;
    wire               dma_inv_we;
    wire         [8:0] dma_inv_idx;

    wire               ic_sdram_req;
    wire        [20:0] ic_sdram_addr;
    wire               ic_sdram_ack;

    wire               dc_sdram_req, dc_sdram_we;
    wire        [20:0] dc_sdram_addr;
    wire         [3:0] dc_sdram_wstrb;
    wire        [31:0] dc_sdram_wdata;
    wire               dc_sdram_ack;

    wire               sdram_req, sdram_we;
    wire        [20:0] sdram_word_addr;
    wire         [3:0] sdram_wstrb;
    wire        [31:0] sdram_wdata, sdram_rdata;
    wire               sdram_ack, sdram_init_done;

    wire               spi_ready, spi_start_read, spi_next_byte, spi_stop_read, spi_byte_valid;
    wire        [23:0] spi_read_addr;
    wire         [7:0] spi_byte_data;

    wire               uart_tx_req, uart_tx_ready, uart_rx_valid, uart_rx_ack;
    wire         [7:0] uart_tx_data, uart_rx_data;

    wire               fb_we, pal_we, has_frame, boot_err;
    wire        [15:0] fb_waddr, frame_count;
    wire         [7:0] fb_wdata, pal_waddr;
    wire        [23:0] pal_wdata;
    wire         [9:0] boot_progress;
    wire        [15:0] fb_sys_raddr;
    wire         [7:0] fb_sys_rdata;

    // -------------------------------------------------------------------------
    // Submodules
    // -------------------------------------------------------------------------
    rebel6_core u_core (
        .clk(clk_sys),
        .rst(rst_sys),
        .start_pulse(cpu_start_pulse),
        .start_pc(cpu_start_pc),
        .instr_count(cpu_instr_count),
        .dmem_limit(24'h680000),
        .host_rf_we(host_rf_we),
        .host_rf_waddr(host_rf_waddr),
        .host_rf_wdata(host_rf_wdata),
        .imem_req(imem_req),
        .imem_addr(imem_addr),
        .imem_rdata(imem_rdata),
        .imem_ready(imem_ready),
        .dmem_req(dmem_req),
        .dmem_we(dmem_we),
        .dmem_addr(dmem_addr),
        .dmem_wstrb(dmem_wstrb),
        .dmem_wdata(dmem_wdata),
        .dmem_rdata(dmem_rdata),
        .dmem_ready(dmem_ready),
        .ecall_req(ecall_req),
        .ecall_a7(ecall_a7),
        .ecall_a0(ecall_a0),
        .ecall_a1(ecall_a1),
        .ecall_a2(ecall_a2),
        .ecall_done(ecall_done),
        .ecall_write_a0(ecall_write_a0),
        .ecall_ret_a0(ecall_ret_a0),
        .ecall_halt(ecall_halt),
        .ecall_exit_code(ecall_exit_code),
        .running(cpu_running),
        .halted(cpu_halted),
        .trapped(cpu_trapped),
        .trap_cause(cpu_trap_cause),
        .pc_out(cpu_pc),
        .cycles_out(cpu_cycles),
        .exit_code_out(cpu_exit_code)
    );

    rebel6_icache #(.SDRAM_IMEM_WORD_BASE(21'h1A0000)) u_icache (
        .clk(clk_sys),
        .rst(rst_sys),
        .flush_req(cache_flush_req),
        .flush_busy(icache_flush_busy),
        .imem_req(imem_req),
        .imem_addr(imem_addr),
        .imem_rdata(imem_rdata),
        .imem_ready(imem_ready),
        .sdram_req(ic_sdram_req),
        .sdram_word_addr(ic_sdram_addr),
        .sdram_rdata(sdram_rdata),
        .sdram_ack(ic_sdram_ack)
    );

    rebel6_dcache u_dcache (
        .clk(clk_sys),
        .rst(rst_sys),
        .flush_req(cache_flush_req),
        .flush_busy(dcache_flush_busy),
        .dma_inv_we(dma_inv_we),
        .dma_inv_idx(dma_inv_idx),
        .dmem_req(dmem_req),
        .dmem_we(dmem_we),
        .dmem_addr(dmem_addr),
        .dmem_wstrb(dmem_wstrb),
        .dmem_wdata(dmem_wdata),
        .dmem_rdata(dmem_rdata),
        .dmem_ready(dmem_ready),
        .sdram_req(dc_sdram_req),
        .sdram_we(dc_sdram_we),
        .sdram_word_addr(dc_sdram_addr),
        .sdram_wstrb(dc_sdram_wstrb),
        .sdram_wdata(dc_sdram_wdata),
        .sdram_rdata(sdram_rdata),
        .sdram_ack(dc_sdram_ack)
    );

    rebel6_sdram32 #(
        .FREQ((SIM_ONLY != 0) ? 100_000 : SYS_FREQ)
    ) u_sdram (
        .clk(clk_sys),
        .clk_sdram(clk_sdram),
        .rst(rst_sys),
        .req(sdram_req),
        .we(sdram_we),
        .word_addr(sdram_word_addr),
        .wstrb(sdram_wstrb),
        .wdata(sdram_wdata),
        .rdata(sdram_rdata),
        .ack(sdram_ack),
        .init_done(sdram_init_done),
        .O_sdram_clk(O_sdram_clk),
        .O_sdram_cke(O_sdram_cke),
        .O_sdram_cs_n(O_sdram_cs_n),
        .O_sdram_cas_n(O_sdram_cas_n),
        .O_sdram_ras_n(O_sdram_ras_n),
        .O_sdram_wen_n(O_sdram_wen_n),
        .O_sdram_dqm(O_sdram_dqm),
        .O_sdram_addr(O_sdram_addr),
        .O_sdram_ba(O_sdram_ba),
        .IO_sdram_dq(IO_sdram_dq)
    );

    rebel6_spiflash #(
        .FREQ((SIM_ONLY != 0) ? 1_000_000 : SYS_FREQ)
    ) u_spiflash (
        .clk(clk_sys),
        .rst(rst_sys),
        .ready(spi_ready),
        .start_read(spi_start_read),
        .read_addr(spi_read_addr),
        .next_byte(spi_next_byte),
        .stop_read(spi_stop_read),
        .byte_valid(spi_byte_valid),
        .byte_data(spi_byte_data),
        .spi_cs_n(O_flash_cs_n),
        .spi_clk(O_flash_clk),
        .spi_mosi(O_flash_mosi),
        .spi_miso(I_flash_miso)
    );

    rebel6_uart #(
        .FREQ(SYS_FREQ),
        .BAUD((SIM_ONLY != 0) ? (SYS_FREQ / 2) : 3_000_000),
        .MIN_TX_CYCLES((SIM_ONLY != 0) ? 0 : 264)
    ) u_uart (
        .clk(clk_sys),
        .rst(rst_sys),
        .tx_req(uart_tx_req),
        .tx_data(uart_tx_data),
        .tx_ready(uart_tx_ready),
        .uart_tx(O_uart_tx),
        .uart_rx(I_uart_rx),
        .rx_valid(uart_rx_valid),
        .rx_data(uart_rx_data),
        .rx_ack(uart_rx_ack)
    );

    rebel6_soc_host #(
        .BOOT_FLASH_ADDR(24'h100000),
        .SDRAM_IMEM_WORD_BASE(21'h1A0000)
    ) u_host (
        .clk(clk_sys),
        .rst(rst_sys),
        .sdram_init_done(sdram_init_done),
        .spi_ready(spi_ready),
        .cache_flush_req(cache_flush_req),
        .icache_flush_busy(icache_flush_busy),
        .dcache_flush_busy(dcache_flush_busy),
        .dma_inv_we(dma_inv_we),
        .dma_inv_idx(dma_inv_idx),
        .ic_sdram_req(ic_sdram_req),
        .ic_sdram_addr(ic_sdram_addr),
        .ic_sdram_ack(ic_sdram_ack),
        .dc_sdram_req(dc_sdram_req),
        .dc_sdram_we(dc_sdram_we),
        .dc_sdram_addr(dc_sdram_addr),
        .dc_sdram_wstrb(dc_sdram_wstrb),
        .dc_sdram_wdata(dc_sdram_wdata),
        .dc_sdram_ack(dc_sdram_ack),
        .sdram_req(sdram_req),
        .sdram_we(sdram_we),
        .sdram_word_addr(sdram_word_addr),
        .sdram_wstrb(sdram_wstrb),
        .sdram_wdata(sdram_wdata),
        .sdram_rdata(sdram_rdata),
        .sdram_ack(sdram_ack),
        .spi_start_read(spi_start_read),
        .spi_read_addr(spi_read_addr),
        .spi_next_byte(spi_next_byte),
        .spi_stop_read(spi_stop_read),
        .spi_byte_valid(spi_byte_valid),
        .spi_byte_data(spi_byte_data),
        .uart_tx_req(uart_tx_req),
        .uart_tx_data(uart_tx_data),
        .uart_tx_ready(uart_tx_ready),
        .uart_rx_valid(uart_rx_valid),
        .uart_rx_ack(uart_rx_ack),
        .fb_raddr(fb_sys_raddr),
        .fb_rdata(fb_sys_rdata),
        .btn_pressed(I_btn),
        .fb_we(fb_we),
        .fb_waddr(fb_waddr),
        .fb_wdata(fb_wdata),
        .pal_we(pal_we),
        .pal_waddr(pal_waddr),
        .pal_wdata(pal_wdata),
        .has_frame(has_frame),
        .boot_progress(boot_progress),
        .frame_count(frame_count),
        .boot_err(boot_err),
        .cpu_start_pulse(cpu_start_pulse),
        .cpu_start_pc(cpu_start_pc),
        .cpu_instr_count(cpu_instr_count),
        .host_rf_we(host_rf_we),
        .host_rf_waddr(host_rf_waddr),
        .host_rf_wdata(host_rf_wdata),
        .ecall_req(ecall_req),
        .ecall_a7(ecall_a7),
        .ecall_a0(ecall_a0),
        .ecall_a1(ecall_a1),
        .cpu_cycles(cpu_cycles),
        .ecall_done(ecall_done),
        .ecall_write_a0(ecall_write_a0),
        .ecall_ret_a0(ecall_ret_a0),
        .ecall_halt(ecall_halt),
        .ecall_exit_code(ecall_exit_code)
    );

    rebel6_hdmi #(.SIM_ONLY(SIM_ONLY)) u_hdmi (
        .clk_sys(clk_sys),
        .fb_we(fb_we),
        .fb_waddr(fb_waddr),
        .fb_wdata(fb_wdata),
        .pal_we(pal_we),
        .pal_waddr(pal_waddr),
        .pal_wdata(pal_wdata),
        .has_frame(has_frame),
        .boot_progress(boot_progress),
        .fb_sys_raddr(fb_sys_raddr),
        .fb_sys_rdata(fb_sys_rdata),
        .clk_pix(clk_pix),
        .clk_tmds(clk_tmds),
        .rst_pix(rst_pix),
        .sim_r(),
        .sim_g(),
        .sim_b(),
        .sim_de(),
        .sim_hs(),
        .sim_vs(),
        .O_tmds_clk_p(O_tmds_clk_p),
        .O_tmds_clk_n(O_tmds_clk_n),
        .O_tmds_data_p(O_tmds_data_p),
        .O_tmds_data_n(O_tmds_data_n)
    );

    // Active-low diagnostic LEDs:
    //   LED0: SDRAM initialized & PLL locked
    //   LED1: CPU actively executing REBEL-6 instructions
    //   LED2: First Doom frame rendered to HDMI
    //   LED3: Toggles on every rendered Doom frame
    //   LED4: Toggles every ~2^20 retired REBEL-6 instructions (heartbeat)
    //   LED5: Fault / trap / boot error indicator
    assign O_led_n = ~{
        (cpu_trapped | boot_err),
        cpu_cycles[20],
        frame_count[0],
        has_frame,
        cpu_running,
        (sdram_init_done & pll_lock)
    };
endmodule

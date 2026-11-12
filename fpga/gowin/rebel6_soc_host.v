// Hardware Bootloader, SDRAM Arbiter and ECALL DMA Service Engine for REBEL-6 Doom SoC.
//
// Autonomous hardware state machine (no helper MCU required):
//   1. Waits for SDRAM init + SPI Flash wake + I/D-Cache initial flush
//   2. Clears guest SDRAM [0 .. clear_words-1] to zero while updating HDMI boot bar
//   3. Streams R6BT header + instruction image + initialized data from SPI Flash
//      (offset 0x100000) into SDRAM, flushes caches, initializes X2 (sp) and X1 (ra),
//      emits "[BOOT OK]" on UART and starts rebel6_core
//   4. Services all 11 ABI ecalls directly in hardware:
//        0 SYS_EXIT      -> halts core, prints "[EXIT]"
//        1 SYS_WRITE     -> streams guest console buffer out UART TX (115200 8N1)
//        2 SYS_TICKS_MS  -> deterministic cycle timer during startup, 1-tic/frame pace in play
//        3 SYS_BLIT      -> 32-bit SDRAM burst -> 64 KiB Framebuffer BRAM + UART frame log
//        4 SYS_GETKEY    -> maps UART RX keys (WASD/Space/F/Enter/Esc) & buttons S1/S2
//        5 SYS_WAD_OPEN  -> opens DOOM1.WAD in SPI Flash (0x200000)
//        6 SYS_WAD_SEEK  -> updates WAD read cursor
//        7 SYS_WAD_READ  -> streams WAD lump from SPI Flash -> SDRAM with D-Cache invalidation
//        8 SYS_WAD_SIZE  -> returns WAD byte length
//        9 SYS_SLEEP_MS  -> returns 0
//       10 SYS_PALETTE   -> loads 256 x RGB24 entries from SDRAM -> HDMI Palette BRAM
module rebel6_soc_host #(
    parameter [23:0] BOOT_FLASH_ADDR      = 24'h100000,
    parameter [20:0] SDRAM_IMEM_WORD_BASE = 21'h1A0000
) (
    input  wire               clk,
    input  wire               rst,

    // Cache & SDRAM readiness / flush control
    input  wire               sdram_init_done,
    input  wire               spi_ready,
    output reg                cache_flush_req,
    input  wire               icache_flush_busy,
    input  wire               dcache_flush_busy,
    output reg                dma_inv_we,
    output reg          [8:0] dma_inv_idx,

    // Cache SDRAM master ports (active only during HS_RUN_CPU)
    input  wire               ic_sdram_req,
    input  wire        [20:0] ic_sdram_addr,
    output wire               ic_sdram_ack,

    input  wire               dc_sdram_req,
    input  wire               dc_sdram_we,
    input  wire        [20:0] dc_sdram_addr,
    input  wire         [3:0] dc_sdram_wstrb,
    input  wire        [31:0] dc_sdram_wdata,
    output wire               dc_sdram_ack,

    // Shared SDRAM controller port
    output wire               sdram_req,
    output wire               sdram_we,
    output wire        [20:0] sdram_word_addr,
    output wire         [3:0] sdram_wstrb,
    output wire        [31:0] sdram_wdata,
    input  wire        [31:0] sdram_rdata,
    input  wire               sdram_ack,

    // SPI Flash reader port
    output reg                spi_start_read,
    output reg         [23:0] spi_read_addr,
    output reg                spi_next_byte,
    output reg                spi_stop_read,
    input  wire               spi_byte_valid,
    input  wire         [7:0] spi_byte_data,

    output wire               uart_tx_req,
    output wire         [7:0] uart_tx_data,
    input  wire               uart_tx_ready,
    input  wire               uart_rx_valid,
    input  wire         [7:0] uart_rx_data,
    output reg                uart_rx_ack,
    input  wire         [1:0] btn_pressed,

    // Live framebuffer video streamer read port (Port A of HDMI fb BRAM)
    output reg         [15:0] fb_raddr,
    input  wire         [7:0] fb_rdata,

    // HDMI Framebuffer & Palette write ports
    output reg                fb_we,
    output reg         [15:0] fb_waddr,
    output reg          [7:0] fb_wdata,
    output reg                pal_we,
    output reg          [7:0] pal_waddr,
    output reg         [23:0] pal_wdata,
    output reg                has_frame,
    output reg          [9:0] boot_progress,
    output reg         [15:0] frame_count,
    output reg                boot_err,

    // REBEL-6 CPU control & ECALL interface
    output reg                cpu_start_pulse,
    output reg  signed [39:0] cpu_start_pc,
    output reg         [31:0] cpu_instr_count,
    output reg                host_rf_we,
    output reg          [9:0] host_rf_waddr,
    output reg  signed [39:0] host_rf_wdata,

    input  wire               ecall_req,
    input  wire signed [39:0] ecall_a7,
    input  wire        [31:0] ecall_a0,
    input  wire        [31:0] ecall_a1,
    input  wire        [63:0] cpu_cycles,
    output reg                ecall_done,
    output reg                ecall_write_a0,
    output reg  signed [39:0] ecall_ret_a0,
    output reg                ecall_halt,
    output reg  signed [39:0] ecall_exit_code
);
    // -------------------------------------------------------------------------
    // SDRAM Arbiter: Host/DMA when state != HS_RUN_CPU, else D-Cache > I-Cache
    // -------------------------------------------------------------------------
    localparam [4:0]
        HS_WAIT_INIT   = 5'd0,
        HS_CLEAR_MEM   = 5'd1,
        HS_OPEN_HDR    = 5'd2,
        HS_READ_HDR    = 5'd3,
        HS_PARSE_HDR   = 5'd4,
        HS_LOAD_IMEM_B = 5'd5,
        HS_LOAD_IMEM_W = 5'd6,
        HS_LOAD_DMEM_B = 5'd7,
        HS_LOAD_DMEM_W = 5'd8,
        HS_BOOT_FLUSH  = 5'd9,
        HS_BOOT_REGS   = 5'd10,
        HS_BOOT_MSG    = 5'd11,
        HS_RUN_CPU     = 5'd12,
        HS_WRITE_RD    = 5'd13,
        HS_WRITE_TX    = 5'd14,
        HS_BLIT_RD     = 5'd15,
        HS_BLIT_WR     = 5'd16,
        HS_BLIT_MSG    = 5'd17,
        HS_PAL_RD      = 5'd18,
        HS_WAD_OPEN_SPI= 5'd19,
        HS_WAD_RD_B    = 5'd20,
        HS_WAD_WR_MEM  = 5'd21,
        HS_KEY_WR0     = 5'd22,
        HS_KEY_WR1     = 5'd23,
        HS_KEY_WR2     = 5'd24,
        HS_ERR_SPIN    = 5'd25;

    reg [4:0] state;

    reg        h_sdram_req;
    reg        h_sdram_we;
    reg [20:0] h_sdram_addr;
    reg  [3:0] h_sdram_wstrb;
    reg [31:0] h_sdram_wdata;

    // 2-way cache arbiter during HS_RUN_CPU (0=none, 1=dcache, 2=icache)
    reg [1:0] cache_owner;
    always @(posedge clk) begin
        if (rst || state != HS_RUN_CPU) begin
            cache_owner <= 2'd0;
        end else if (cache_owner == 2'd0) begin
            if (dc_sdram_req)
                cache_owner <= 2'd1;
            else if (ic_sdram_req)
                cache_owner <= 2'd2;
        end else if (sdram_ack) begin
            cache_owner <= 2'd0;
        end
    end

    wire use_host = (state != HS_RUN_CPU);
    wire use_dc   = (!use_host) && ((cache_owner == 2'd1) || (cache_owner == 2'd0 && dc_sdram_req));
    wire use_ic   = (!use_host) && ((cache_owner == 2'd2) || (cache_owner == 2'd0 && !dc_sdram_req && ic_sdram_req));

    assign sdram_req       = use_host ? h_sdram_req   : (use_dc ? dc_sdram_req   : (use_ic ? ic_sdram_req  : 1'b0));
    assign sdram_we        = use_host ? h_sdram_we    : (use_dc ? dc_sdram_we    : 1'b0);
    assign sdram_word_addr = use_host ? h_sdram_addr  : (use_dc ? dc_sdram_addr  : ic_sdram_addr);
    assign sdram_wstrb     = use_host ? h_sdram_wstrb : (use_dc ? dc_sdram_wstrb : 4'b0000);
    assign sdram_wdata     = use_host ? h_sdram_wdata : dc_sdram_wdata;

    assign dc_sdram_ack    = (!use_host) && (cache_owner == 2'd1 || (cache_owner == 2'd0 && dc_sdram_req)) && sdram_ack;
    assign ic_sdram_ack    = (!use_host) && (cache_owner == 2'd2 || (cache_owner == 2'd0 && !dc_sdram_req && ic_sdram_req)) && sdram_ack;

    // -------------------------------------------------------------------------
    // Input Event Queue (16-slot FIFO for press/release from Buttons + UART RX)
    // -------------------------------------------------------------------------
    reg  [7:0] key_fifo_code [0:15];
    reg        key_fifo_down [0:15];
    reg  [3:0] kf_head, kf_tail;
    wire       kf_empty = (kf_head == kf_tail);
    wire       kf_full  = ((kf_tail + 4'd1) & 4'hF) == kf_head;

    reg  [1:0] btn_sync0, btn_sync1, btn_prev;
    reg  [7:0] auto_rel_key;
    reg [21:0] auto_rel_timer; // ~50 ms hold for UART single-char keypresses

    // Binary keyboard packets from the Linux viewer: 0xFD down / 0xFC up + code.
    reg        uart_pkt_stage;
    reg        uart_pkt_down;

    // Doom key-down deferral: deliver each press for at least one game tic.
    reg  [7:0] last_down_code;
    reg        last_down_valid;
    reg        last_up_seen;

    // Console TX FIFO: drained only while the video streamer is idle.
    reg        c_uart_tx_req;
    reg  [7:0] c_uart_tx_data;
    reg  [7:0] con_fifo [0:255];
    reg  [7:0] con_head, con_tail;
    wire       con_empty = (con_head == con_tail);
    wire       con_full  = ((con_tail + 8'd1) & 8'hFF) == con_head;

    // Live video streamer handshake pulse (set by HS_BLIT_WR, read by streamer FSM).
    reg        v_start_capture;
    // Palette -> fb_mem[64000..] RGB mirror sub-step.
    reg  [1:0] mirror_step;


    function automatic [7:0] map_uart_key(input [7:0] ch);
        begin
            case (ch)
                "w", "W": map_uart_key = 8'hAD; // KEY_UPARROW
                "s", "S": map_uart_key = 8'hAF; // KEY_DOWNARROW
                "a", "A": map_uart_key = 8'hAC; // KEY_LEFTARROW
                "d", "D": map_uart_key = 8'hAE; // KEY_RIGHTARROW
                " ", "e", "E": map_uart_key = 8'hA2; // KEY_USE
                "f", "F": map_uart_key = 8'hA3; // KEY_FIRE
                8'd13, 8'd10: map_uart_key = 8'd13; // KEY_ENTER
                8'd27:    map_uart_key = 8'd27; // KEY_ESCAPE
                8'd9:     map_uart_key = 8'd9;  // KEY_TAB
                default:  map_uart_key = 8'd0;
            endcase
        end
    endfunction

    // -------------------------------------------------------------------------
    // R6BT Boot Header storage (36 bytes = 9 x u32 little-endian)
    //   [0] magic = "R6BT" (0x54423652)
    //   [1] instr_count
    //   [2] data_base
    //   [3] init_data_len
    //   [4] clear_words   (e.g. 1638400 = 6.25 MiB)
    //   [5] stack_top     (e.g. 6500000)
    //   [6] entry_pc
    //   [7] wad_flash_addr (e.g. 0x200000)
    //   [8] wad_size       (e.g. 4196020)
    // -------------------------------------------------------------------------
    reg [31:0] hdr_words [0:8];
    reg  [5:0] hdr_byte_idx;
    reg [31:0] sh_word;
    reg  [1:0] sub_byte;

    reg [20:0] xfer_idx;
    reg [23:0] xfer_rem;
    reg [23:0] dst_baddr;

    reg        wad_is_open;
    reg [23:0] wad_pos;
    reg [63:0] frame4_cycles;

    // Cached 32-bit SDRAM word reader for fast sequential byte reads (BLIT, PALETTE, WRITE)
    reg        rd_cache_valid;
    reg [20:0] rd_cache_waddr;
    reg [31:0] rd_cache_data;
    reg  [7:0] pal_rgb_buf [0:1];
    reg  [1:0] pal_comp;

    wire [7:0] cur_cached_byte =
        (dst_baddr[1:0] == 2'd0) ? rd_cache_data[7:0]   :
        (dst_baddr[1:0] == 2'd1) ? rd_cache_data[15:8]  :
        (dst_baddr[1:0] == 2'd2) ? rd_cache_data[23:16] : rd_cache_data[31:24];

    // Banner strings: 0 = "[REBEL6 BOOT OK]\r\n" (18B), 1 = "[DOOM FRAME]\r\n" (14B), 2 = "[ERR NO R6BT]\r\n" (15B)
    reg  [1:0] msg_sel;
    reg  [4:0] msg_idx;
    reg  [4:0] msg_len;
    function automatic [7:0] msg_char(input [1:0] sel, input [4:0] idx);
        begin
            msg_char = 8'h20;
            case (sel)
                2'd0: case (idx)
                    5'd0: msg_char="["; 5'd1: msg_char="R"; 5'd2: msg_char="E"; 5'd3: msg_char="B";
                    5'd4: msg_char="E"; 5'd5: msg_char="L"; 5'd6: msg_char="6"; 5'd7: msg_char=" ";
                    5'd8: msg_char="B"; 5'd9: msg_char="O"; 5'd10:msg_char="O"; 5'd11:msg_char="T";
                    5'd12:msg_char=" "; 5'd13:msg_char="O"; 5'd14:msg_char="K"; 5'd15:msg_char="]";
                    5'd16:msg_char=8'd13; 5'd17:msg_char=8'd10; default: msg_char=8'd0;
                endcase
                2'd1: case (idx)
                    5'd0: msg_char=8'd13; 5'd1: msg_char=8'd10;
                    5'd2: msg_char="["; 5'd3: msg_char="D"; 5'd4: msg_char="O"; 5'd5: msg_char="O";
                    5'd6: msg_char="M"; 5'd7: msg_char=" "; 5'd8: msg_char="F"; 5'd9: msg_char="R";
                    5'd10:msg_char="A"; 5'd11:msg_char="M"; 5'd12:msg_char="E"; 5'd13:msg_char="]";
                    5'd14:msg_char=8'd13; 5'd15:msg_char=8'd10; default: msg_char=8'd0;
                endcase
                default: case (idx)
                    5'd0: msg_char="["; 5'd1: msg_char="E"; 5'd2: msg_char="R"; 5'd3: msg_char="R";
                    5'd4: msg_char=" "; 5'd5: msg_char="N"; 5'd6: msg_char="O"; 5'd7: msg_char=" ";
                    5'd8: msg_char="R"; 5'd9: msg_char="6"; 5'd10:msg_char="B"; 5'd11:msg_char="T";
                    5'd12:msg_char="]"; 5'd13:msg_char=8'd13; 5'd14:msg_char=8'd10; default: msg_char=8'd0;
                endcase
            endcase
        end
    endfunction

    reg [31:0] key_val_down;
    reg  [7:0] key_val_code;
    reg  [3:0] reg_step;

    always @(posedge clk) begin
        if (rst) begin
            state           <= HS_WAIT_INIT;
            h_sdram_req     <= 1'b0;
            h_sdram_we      <= 1'b0;
            h_sdram_addr    <= 21'd0;
            h_sdram_wstrb   <= 4'b0000;
            h_sdram_wdata   <= 32'd0;
            cache_flush_req <= 1'b0;
            dma_inv_we      <= 1'b0;
            dma_inv_idx     <= 9'd0;
            spi_start_read  <= 1'b0;
            spi_read_addr   <= 24'd0;
            spi_next_byte   <= 1'b0;
            spi_stop_read   <= 1'b0;
            uart_rx_ack     <= 1'b0;
            fb_we           <= 1'b0;
            fb_waddr        <= 16'd0;
            fb_wdata        <= 8'd0;
            pal_we          <= 1'b0;
            pal_waddr       <= 8'd0;
            pal_wdata       <= 24'd0;
            has_frame       <= 1'b0;
            boot_progress   <= 10'd0;
            frame_count     <= 16'd0;
            boot_err        <= 1'b0;
            cpu_start_pulse <= 1'b0;
            cpu_start_pc    <= 40'sd0;
            cpu_instr_count <= 32'd0;
            host_rf_we      <= 1'b0;
            host_rf_waddr   <= 10'd0;
            host_rf_wdata   <= 40'sd0;
            ecall_done      <= 1'b0;
            ecall_write_a0  <= 1'b0;
            ecall_ret_a0    <= 40'sd0;
            ecall_halt      <= 1'b0;
            ecall_exit_code <= 40'sd0;
            wad_is_open     <= 1'b0;
            wad_pos         <= 24'd0;
            frame4_cycles   <= 64'd0;
            rd_cache_valid  <= 1'b0;
            kf_head         <= 4'd0;
            kf_tail         <= 4'd0;
            uart_pkt_stage <= 1'b0;
            uart_pkt_down  <= 1'b0;
            last_down_valid <= 1'b0;
            last_up_seen   <= 1'b0;
            last_down_code  <= 8'd0;
            v_start_capture <= 1'b0;
            mirror_step     <= 2'd0;
            con_head        <= 8'd0;
            con_tail        <= 8'd0;
            btn_sync0       <= 2'b00;
            btn_sync1       <= 2'b00;
            btn_prev        <= 2'b00;
            auto_rel_key    <= 8'd0;
            auto_rel_timer  <= 22'd0;
        end else begin
            // Default single-cycle pulses
            cache_flush_req <= 1'b0;
            dma_inv_we      <= 1'b0;
            spi_start_read  <= 1'b0;
            spi_next_byte   <= 1'b0;
            spi_stop_read   <= 1'b0;
            v_start_capture <= 1'b0;
            uart_rx_ack     <= 1'b0;
            fb_we           <= 1'b0;
            pal_we          <= 1'b0;
            cpu_start_pulse <= 1'b0;
            host_rf_we      <= 1'b0;
            ecall_done      <= 1'b0;
            ecall_write_a0  <= 1'b0;
            ecall_halt      <= 1'b0;

            // Button & UART key capture
            // Binary key packets: 0xFD = key down, 0xFC = key up, followed by the
            // Doom keycode. Legacy single ASCII chars still map to down + auto-release.
            btn_sync0 <= btn_pressed;
            btn_sync1 <= btn_sync0;
            if (btn_sync1[0] != btn_prev[0]) begin
                btn_prev[0]             <= btn_sync1[0];
                if (!kf_full) begin
                    key_fifo_code[kf_tail]  <= 8'hAD; // S1 = Forward (KEY_UPARROW)
                    key_fifo_down[kf_tail]  <= btn_sync1[0];
                    kf_tail                 <= kf_tail + 4'd1;
                end
            end else if (btn_sync1[1] != btn_prev[1]) begin
                btn_prev[1]             <= btn_sync1[1];
                if (!kf_full) begin
                    key_fifo_code[kf_tail]  <= 8'hA3; // S2 = Shoot (KEY_FIRE)
                    key_fifo_down[kf_tail]  <= btn_sync1[1];
                    kf_tail                 <= kf_tail + 4'd1;
                end
            end else if (uart_rx_valid) begin
                uart_rx_ack <= 1'b1;
                if (uart_pkt_stage) begin
                    if (!kf_full) begin
                        key_fifo_code[kf_tail] <= uart_rx_data;
                        key_fifo_down[kf_tail] <= uart_pkt_down;
                        kf_tail                <= kf_tail + 4'd1;
                    end
                    uart_pkt_stage <= 1'b0;
                end else if (uart_rx_data == 8'hFD) begin
                    uart_pkt_down   <= 1'b1;
                    uart_pkt_stage <= 1'b1;
                end else if (uart_rx_data == 8'hFC) begin
                    uart_pkt_down   <= 1'b0;
                    uart_pkt_stage <= 1'b1;
                end else if (map_uart_key(uart_rx_data) != 8'd0) begin
                    if (!kf_full) begin
                        key_fifo_code[kf_tail] <= map_uart_key(uart_rx_data);
                        key_fifo_down[kf_tail] <= 1'b1;
                        kf_tail                <= kf_tail + 4'd1;
                    end
                    auto_rel_key   <= map_uart_key(uart_rx_data);
                    auto_rel_timer <= 22'd3_000_000;
                end
            end else if (auto_rel_key != 8'd0) begin
                if (auto_rel_timer == 22'd0) begin
                    if (!kf_full) begin
                        key_fifo_code[kf_tail] <= auto_rel_key;
                        key_fifo_down[kf_tail] <= 1'b0;
                        kf_tail                <= kf_tail + 4'd1;
                    end
                    auto_rel_key <= 8'd0;
                end else begin
                    auto_rel_timer <= auto_rel_timer - 22'd1;
                end
            end

            case (state)
                // -------------------------------------------------------------
                HS_WAIT_INIT: begin
                    if (sdram_init_done && spi_ready && !icache_flush_busy && !dcache_flush_busy) begin
                        spi_start_read <= 1'b1;
                        spi_read_addr  <= BOOT_FLASH_ADDR;
                        hdr_byte_idx   <= 6'd0;
                        state          <= HS_OPEN_HDR;
                    end
                end

                HS_OPEN_HDR: begin
                    spi_next_byte <= 1'b1;
                    state         <= HS_READ_HDR;
                end

                HS_READ_HDR: begin
                    if (spi_byte_valid) begin
                        sh_word <= {spi_byte_data, sh_word[31:8]};
                        if (hdr_byte_idx[1:0] == 2'd3) begin
                            hdr_words[hdr_byte_idx[5:2]] <= {spi_byte_data, sh_word[31:8]};
                        end
                        if (hdr_byte_idx == 6'd35) begin
                            state <= HS_PARSE_HDR;
                        end else begin
                            hdr_byte_idx  <= hdr_byte_idx + 6'd1;
                            spi_next_byte <= 1'b1;
                        end
                    end
                end

                HS_PARSE_HDR: begin
                    if (hdr_words[0] != 32'h54423652) begin // "R6BT"
                        spi_stop_read <= 1'b1;
                        boot_err      <= 1'b1;
                        msg_sel       <= 2'd2;
                        msg_idx       <= 5'd0;
                        msg_len       <= 5'd15;
                        state         <= HS_ERR_SPIN;
                    end else begin
                        cpu_instr_count <= hdr_words[1];
                        xfer_idx        <= 21'd0;
                        boot_progress   <= 10'd16;
                        state           <= HS_CLEAR_MEM;
                    end
                end

                // Clear guest dmem [0 .. clear_words-1] before loading data
                HS_CLEAR_MEM: begin
                    if (!h_sdram_req) begin
                        if (xfer_idx >= hdr_words[4][20:0]) begin
                            // Now stream instr_count * 2 32-bit words from SPI into SDRAM imem
                            xfer_idx      <= 21'd0;
                            xfer_rem      <= {hdr_words[1][22:0], 1'b0}; // 2 words per 64-bit instr
                            sub_byte      <= 2'd0;
                            spi_next_byte <= 1'b1;
                            state         <= HS_LOAD_IMEM_B;
                        end else begin
                            h_sdram_req   <= 1'b1;
                            h_sdram_we    <= 1'b1;
                            h_sdram_addr  <= xfer_idx;
                            h_sdram_wstrb <= 4'b1111;
                            h_sdram_wdata <= 32'd0;
                        end
                    end else if (sdram_ack) begin
                        h_sdram_req   <= 1'b0;
                        h_sdram_we    <= 1'b0;
                        xfer_idx      <= xfer_idx + 21'd1;
                        boot_progress <= 10'd16 + xfer_idx[20:14]; // 16..118
                    end
                end

                HS_LOAD_IMEM_B: begin
                    if (xfer_rem == 24'd0) begin
                        dst_baddr <= hdr_words[2][23:0]; // data_base
                        xfer_rem  <= hdr_words[3][23:0]; // init_data_len
                        if (hdr_words[3][23:0] == 24'd0) begin
                            spi_stop_read   <= 1'b1;
                            cache_flush_req <= 1'b1;
                            state           <= HS_BOOT_FLUSH;
                        end else begin
                            spi_next_byte <= 1'b1;
                            state         <= HS_LOAD_DMEM_B;
                        end
                    end else if (spi_byte_valid) begin
                        sh_word <= {spi_byte_data, sh_word[31:8]};
                        if (sub_byte == 2'd3) begin
                            h_sdram_req   <= 1'b1;
                            h_sdram_we    <= 1'b1;
                            h_sdram_addr  <= SDRAM_IMEM_WORD_BASE + xfer_idx;
                            h_sdram_wstrb <= 4'b1111;
                            h_sdram_wdata <= {spi_byte_data, sh_word[31:8]};
                            state         <= HS_LOAD_IMEM_W;
                        end else begin
                            sub_byte      <= sub_byte + 2'd1;
                            spi_next_byte <= 1'b1;
                        end
                    end
                end

                HS_LOAD_IMEM_W: begin
                    if (sdram_ack) begin
                        h_sdram_req   <= 1'b0;
                        h_sdram_we    <= 1'b0;
                        xfer_idx      <= xfer_idx + 21'd1;
                        xfer_rem      <= xfer_rem - 24'd1;
                        sub_byte      <= 2'd0;
                        boot_progress <= 10'd120 + xfer_idx[18:10];
                        if (xfer_rem > 24'd1)
                            spi_next_byte <= 1'b1;
                        state <= HS_LOAD_IMEM_B;
                    end
                end

                HS_LOAD_DMEM_B: begin
                    if (spi_byte_valid) begin
                        h_sdram_req   <= 1'b1;
                        h_sdram_we    <= 1'b1;
                        h_sdram_addr  <= dst_baddr[22:2];
                        h_sdram_wstrb <= (4'b0001 << dst_baddr[1:0]);
                        h_sdram_wdata <= {4{spi_byte_data}};
                        state         <= HS_LOAD_DMEM_W;
                    end
                end

                HS_LOAD_DMEM_W: begin
                    if (sdram_ack) begin
                        h_sdram_req   <= 1'b0;
                        h_sdram_we    <= 1'b0;
                        dst_baddr     <= dst_baddr + 24'd1;
                        xfer_rem      <= xfer_rem - 24'd1;
                        boot_progress <= 10'd255 - xfer_rem[19:12];
                        if (xfer_rem == 24'd1) begin
                            spi_stop_read   <= 1'b1;
                            cache_flush_req <= 1'b1;
                            state           <= HS_BOOT_FLUSH;
                        end else begin
                            spi_next_byte <= 1'b1;
                            state         <= HS_LOAD_DMEM_B;
                        end
                    end
                end

                HS_BOOT_FLUSH: begin
                    if (!cache_flush_req && !icache_flush_busy && !dcache_flush_busy) begin
                        reg_step      <= 4'd0;
                        boot_progress <= 10'd256;
                        state         <= HS_BOOT_REGS;
                    end
                end

                HS_BOOT_REGS: begin
                    reg_step <= reg_step + 4'd1;
                    if (reg_step == 4'd0) begin
                        host_rf_we    <= 1'b1;
                        host_rf_waddr <= 10'd2; // sp = stack_top
                        host_rf_wdata <= {8'd0, hdr_words[5]};
                    end else if (reg_step == 4'd1) begin
                        host_rf_we    <= 1'b1;
                        host_rf_waddr <= 10'd1; // ra = -1
                        host_rf_wdata <= -40'sd1;
                    end else begin
                        msg_sel <= 2'd0;
                        msg_idx <= 5'd0;
                        msg_len <= 5'd18;
                        state   <= HS_BOOT_MSG;
                    end
                end

                HS_BOOT_MSG: begin
                    if (msg_idx >= msg_len) begin
                        cpu_start_pc    <= {8'd0, hdr_words[6]};
                        cpu_start_pulse <= 1'b1;
                        state           <= HS_RUN_CPU;
                    end else if (!con_full) begin
                        con_fifo[con_tail] <= msg_char(msg_sel, msg_idx);
                        con_tail           <= con_tail + 8'd1;
                        msg_idx            <= msg_idx + 5'd1;
                    end
                end

                // -------------------------------------------------------------
                // Live CPU execution & ECALL dispatch
                // -------------------------------------------------------------
                HS_RUN_CPU: begin
                    if (ecall_req && !ecall_done) begin
                        rd_cache_valid <= 1'b0;
                        case (ecall_a7[7:0])
                            8'd0: begin // SYS_EXIT
                                ecall_halt      <= 1'b1;
                                ecall_exit_code <= {{8{ecall_a0[31]}}, ecall_a0};
                                ecall_done      <= 1'b1;
                            end

                            8'd1: begin // SYS_WRITE(buf=a0, len=a1)
                                if ($signed(ecall_a1) <= 32'sd0) begin
                                    ecall_write_a0 <= 1'b1;
                                    ecall_ret_a0   <= 40'sd0;
                                    ecall_done     <= 1'b1;
                                end else begin
                                    dst_baddr <= ecall_a0[23:0];
                                    xfer_rem  <= ecall_a1[23:0];
                                    state     <= HS_WRITE_RD;
                                end
                            end

                            8'd2: begin // SYS_TICKS_MS
                                ecall_write_a0 <= 1'b1;
                                if (frame_count < 16'd4) begin
                                    // Startup parity: ~1 ms per 1024 retired instructions
                                    ecall_ret_a0 <= {{8{1'b0}}, cpu_cycles[41:10]};
                                end else begin
                                    // Live gameplay: ~1 Doom tic (29 ms) per ~3.8M instructions
                                    ecall_ret_a0 <= {{8{1'b0}}, (frame4_cycles[41:10] + ((cpu_cycles - frame4_cycles) >> 17))};
                                end
                                ecall_done <= 1'b1;
                            end

                            8'd3: begin // SYS_BLIT(fb=a0)
                                if (!v_stream_active) begin
                                    dst_baddr <= ecall_a0[23:0];
                                    xfer_idx  <= 21'd0;
                                    state     <= HS_BLIT_RD;
                                end
                            end

                            8'd4: begin // SYS_GETKEY(pressed_ptr=a0, key_ptr=a1)
                                if (kf_empty) begin
                                    ecall_write_a0 <= 1'b1;
                                    ecall_ret_a0   <= 40'sd0;
                                    ecall_done     <= 1'b1;
                                end else if (key_fifo_down[kf_head] && last_down_valid &&
                                             key_fifo_code[kf_head] == last_down_code && !last_up_seen) begin
                                    // Same key still held down within this frame: return 0 so
                                    // Doom sees each press for at least one tic.
                                    ecall_write_a0 <= 1'b1;
                                    ecall_ret_a0   <= 40'sd0;
                                    ecall_done     <= 1'b1;
                                end else begin
                                    key_val_code <= key_fifo_code[kf_head];
                                    key_val_down <= {31'd0, key_fifo_down[kf_head]};
                                    if (key_fifo_down[kf_head]) begin
                                        last_down_code  <= key_fifo_code[kf_head];
                                        last_down_valid <= 1'b1;
                                        last_up_seen   <= 1'b0;
                                    end else begin
                                        last_down_valid <= 1'b0;
                                        last_up_seen   <= 1'b1;
                                    end
                                    kf_head      <= kf_head + 4'd1;
                                    dst_baddr    <= ecall_a0[23:0];
                                    sub_byte     <= 2'd0;
                                    state        <= HS_KEY_WR0;
                                end
                            end

                            8'd5: begin // SYS_WAD_OPEN
                                wad_is_open    <= (hdr_words[8] != 32'd0);
                                wad_pos        <= 24'd0;
                                ecall_write_a0 <= 1'b1;
                                ecall_ret_a0   <= (hdr_words[8] != 32'd0) ? 40'sd0 : -40'sd2;
                                ecall_done     <= 1'b1;
                            end

                            8'd6: begin // SYS_WAD_SEEK
                                if (!wad_is_open) begin
                                    ecall_write_a0 <= 1'b1;
                                    ecall_ret_a0   <= -40'sd9;
                                end else begin
                                    wad_pos        <= ecall_a0[23:0];
                                    ecall_write_a0 <= 1'b1;
                                    ecall_ret_a0   <= 40'sd0;
                                end
                                ecall_done <= 1'b1;
                            end

                            8'd7: begin // SYS_WAD_READ(buf=a0, len=a1)
                                if (!wad_is_open) begin
                                    ecall_write_a0 <= 1'b1;
                                    ecall_ret_a0   <= -40'sd9;
                                    ecall_done     <= 1'b1;
                                end else if ($signed(ecall_a1) <= 32'sd0 || wad_pos >= hdr_words[8][23:0]) begin
                                    ecall_write_a0 <= 1'b1;
                                    ecall_ret_a0   <= 40'sd0;
                                    ecall_done     <= 1'b1;
                                end else begin
                                    dst_baddr      <= ecall_a0[23:0];
                                    xfer_rem       <= ((wad_pos + ecall_a1[23:0]) > hdr_words[8][23:0])
                                                    ? (hdr_words[8][23:0] - wad_pos)
                                                    : ecall_a1[23:0];
                                    spi_start_read <= 1'b1;
                                    spi_read_addr  <= hdr_words[7][23:0] + wad_pos;
                                    state          <= HS_WAD_OPEN_SPI;
                                end
                            end

                            8'd8: begin // SYS_WAD_SIZE
                                ecall_write_a0 <= 1'b1;
                                ecall_ret_a0   <= wad_is_open ? {8'd0, hdr_words[8]} : -40'sd9;
                                ecall_done     <= 1'b1;
                            end

                            8'd9: begin // SYS_SLEEP_MS
                                ecall_write_a0 <= 1'b1;
                                ecall_ret_a0   <= 40'sd0;
                                ecall_done     <= 1'b1;
                            end

                            8'd10: begin // SYS_PALETTE(rgb768=a0)
                                if (!v_stream_active) begin
                                    dst_baddr <= ecall_a0[23:0];
                                    xfer_idx  <= 21'd0;
                                    pal_comp  <= 2'd0;
                                    mirror_step <= 2'd0;
                                    state     <= HS_PAL_RD;
                                end
                            end

                            default: begin
                                ecall_write_a0 <= 1'b1;
                                ecall_ret_a0   <= -40'sd22;
                                ecall_done     <= 1'b1;
                            end
                        endcase
                    end
                end

                // -------------------------------------------------------------
                // SYS_WRITE: cached 32-bit SDRAM read -> UART TX
                // -------------------------------------------------------------
                HS_WRITE_RD: begin
                    if (rd_cache_valid && rd_cache_waddr == dst_baddr[22:2]) begin
                        state <= HS_WRITE_TX;
                    end else if (!h_sdram_req) begin
                        h_sdram_req   <= 1'b1;
                        h_sdram_we    <= 1'b0;
                        h_sdram_addr  <= dst_baddr[22:2];
                        h_sdram_wstrb <= 4'b0000;
                    end else if (sdram_ack) begin
                        h_sdram_req    <= 1'b0;
                        rd_cache_valid <= 1'b1;
                        rd_cache_waddr <= dst_baddr[22:2];
                        rd_cache_data  <= sdram_rdata;
                        state          <= HS_WRITE_TX;
                    end
                end

                HS_WRITE_TX: begin
                    if (!con_full) begin
                        con_fifo[con_tail] <= cur_cached_byte;
                        con_tail           <= con_tail + 8'd1;
                        dst_baddr          <= dst_baddr + 24'd1;
                        if (xfer_rem == 24'd1) begin
                            ecall_write_a0 <= 1'b1;
                            ecall_ret_a0   <= {8'd0, ecall_a1};
                            ecall_done     <= 1'b1;
                            state          <= HS_RUN_CPU;
                        end else begin
                            xfer_rem <= xfer_rem - 24'd1;
                            state    <= HS_WRITE_RD;
                        end
                    end
                end

                // -------------------------------------------------------------
                // SYS_BLIT: 64,000 bytes SDRAM -> Dual-port Framebuffer BRAM
                // -------------------------------------------------------------
                HS_BLIT_RD: begin
                    if (rd_cache_valid && rd_cache_waddr == dst_baddr[22:2]) begin
                        state <= HS_BLIT_WR;
                    end else if (!h_sdram_req) begin
                        h_sdram_req   <= 1'b1;
                        h_sdram_we    <= 1'b0;
                        h_sdram_addr  <= dst_baddr[22:2];
                        h_sdram_wstrb <= 4'b0000;
                    end else if (sdram_ack) begin
                        h_sdram_req    <= 1'b0;
                        rd_cache_valid <= 1'b1;
                        rd_cache_waddr <= dst_baddr[22:2];
                        rd_cache_data  <= sdram_rdata;
                        state          <= HS_BLIT_WR;
                    end
                end

                HS_BLIT_WR: begin
                    fb_we     <= 1'b1;
                    fb_waddr  <= xfer_idx[15:0];
                    fb_wdata  <= cur_cached_byte;
                    dst_baddr <= dst_baddr + 24'd1;
                    if (xfer_idx == 21'd63999) begin
                        has_frame   <= 1'b1;
                        frame_count <= frame_count + 16'd1;
                        v_start_capture <= 1'b1;
                        if (frame_count == 16'd3)
                            frame4_cycles <= cpu_cycles;
                        state   <= HS_BLIT_MSG;
                    end else begin
                        xfer_idx <= xfer_idx + 21'd1;
                        state    <= HS_BLIT_RD;
                    end
                end

                HS_BLIT_MSG: begin
                    // The stream marker and frame bytes are emitted by the background
                    // video streamer; nothing else to wait for here.
                    ecall_write_a0 <= 1'b1;
                    ecall_ret_a0   <= 40'sd0;
                    ecall_done     <= 1'b1;
                    state          <= HS_RUN_CPU;
                end

                // -------------------------------------------------------------
                // SYS_PALETTE: 768 bytes (256 x R,G,B) SDRAM -> Palette BRAM
                // -------------------------------------------------------------
                HS_PAL_RD: begin
                    if (rd_cache_valid && rd_cache_waddr == dst_baddr[22:2]) begin
                        dst_baddr <= dst_baddr + 24'd1;
                        if (pal_comp == 2'd0) begin
                            pal_rgb_buf[0] <= cur_cached_byte;
                            pal_comp       <= 2'd1;
                        end else if (pal_comp == 2'd1) begin
                            pal_rgb_buf[1] <= cur_cached_byte;
                            pal_comp       <= 2'd2;
                        end else begin
                            // Mirror the palette RGB bytes into fb_mem[64000 + e*3 .. +2]
                            // so every live stream frame carries the active palette.
                            if (mirror_step == 2'd0) begin
                                fb_we    <= 1'b1;
                                fb_waddr <= 16'd64000 + {8'd0, xfer_idx[7:0]} + ((xfer_idx[7:0] << 1));
                                fb_wdata <= pal_rgb_buf[0];
                                mirror_step <= 2'd1;
                            end else if (mirror_step == 2'd1) begin
                                fb_we    <= 1'b1;
                                fb_waddr <= 16'd64001 + {8'd0, xfer_idx[7:0]} + ((xfer_idx[7:0] << 1));
                                fb_wdata <= pal_rgb_buf[1];
                                mirror_step <= 2'd2;
                            end else begin
                                fb_we    <= 1'b1;
                                fb_waddr <= 16'd64002 + {8'd0, xfer_idx[7:0]} + ((xfer_idx[7:0] << 1));
                                fb_wdata <= cur_cached_byte;
                                mirror_step <= 2'd0;
                                pal_we    <= 1'b1;
                                pal_waddr <= xfer_idx[7:0];
                                pal_wdata <= {pal_rgb_buf[0], pal_rgb_buf[1], cur_cached_byte};
                                pal_comp  <= 2'd0;
                                if (xfer_idx == 21'd255) begin
                                    ecall_write_a0 <= 1'b1;
                                    ecall_ret_a0   <= 40'sd0;
                                    ecall_done     <= 1'b1;
                                    state          <= HS_RUN_CPU;
                                end else begin
                                    xfer_idx <= xfer_idx + 21'd1;
                                end
                            end
                        end
                    end else if (!h_sdram_req) begin
                        h_sdram_req   <= 1'b1;
                        h_sdram_we    <= 1'b0;
                        h_sdram_addr  <= dst_baddr[22:2];
                        h_sdram_wstrb <= 4'b0000;
                    end else if (sdram_ack) begin
                        h_sdram_req    <= 1'b0;
                        rd_cache_valid <= 1'b1;
                        rd_cache_waddr <= dst_baddr[22:2];
                        rd_cache_data  <= sdram_rdata;
                    end
                end

                // -------------------------------------------------------------
                // SYS_WAD_READ: SPI Flash -> SDRAM + live D-Cache invalidation
                // -------------------------------------------------------------
                HS_WAD_OPEN_SPI: begin
                    xfer_idx      <= xfer_rem[20:0]; // save total count for return value
                    boot_progress <= 10'd256 + wad_pos[21:14];
                    spi_next_byte <= 1'b1;
                    state         <= HS_WAD_RD_B;
                end

                HS_WAD_RD_B: begin
                    if (spi_byte_valid) begin
                        h_sdram_req   <= 1'b1;
                        h_sdram_we    <= 1'b1;
                        h_sdram_addr  <= dst_baddr[22:2];
                        h_sdram_wstrb <= (4'b0001 << dst_baddr[1:0]);
                        h_sdram_wdata <= {4{spi_byte_data}};
                        dma_inv_we    <= 1'b1;
                        dma_inv_idx   <= dst_baddr[10:2];
                        state         <= HS_WAD_WR_MEM;
                    end
                end

                HS_WAD_WR_MEM: begin
                    if (sdram_ack) begin
                        h_sdram_req <= 1'b0;
                        h_sdram_we  <= 1'b0;
                        dst_baddr   <= dst_baddr + 24'd1;
                        wad_pos     <= wad_pos + 24'd1;
                        if (xfer_rem == 24'd1) begin
                            spi_stop_read  <= 1'b1;
                            ecall_write_a0 <= 1'b1;
                            ecall_ret_a0   <= {19'd0, xfer_idx};
                            ecall_done     <= 1'b1;
                            state          <= HS_RUN_CPU;
                        end else begin
                            xfer_rem      <= xfer_rem - 24'd1;
                            spi_next_byte <= 1'b1;
                            state         <= HS_WAD_RD_B;
                        end
                    end
                end

                // -------------------------------------------------------------
                // SYS_GETKEY: write 4-byte *pressed (a0) and 1-byte *key (a1)
                // -------------------------------------------------------------
                HS_KEY_WR0: begin
                    if (!h_sdram_req) begin
                        h_sdram_req   <= 1'b1;
                        h_sdram_we    <= 1'b1;
                        h_sdram_addr  <= dst_baddr[22:2];
                        h_sdram_wstrb <= (4'b0001 << dst_baddr[1:0]);
                        h_sdram_wdata <= {4{(sub_byte == 2'd0) ? key_val_down[7:0] : 8'd0}};
                        dma_inv_we    <= 1'b1;
                        dma_inv_idx   <= dst_baddr[10:2];
                    end else if (sdram_ack) begin
                        h_sdram_req <= 1'b0;
                        h_sdram_we  <= 1'b0;
                        if (sub_byte == 2'd3) begin
                            dst_baddr <= ecall_a1[23:0];
                            state     <= HS_KEY_WR1;
                        end else begin
                            sub_byte  <= sub_byte + 2'd1;
                            dst_baddr <= dst_baddr + 24'd1;
                        end
                    end
                end

                HS_KEY_WR1: begin
                    h_sdram_req   <= 1'b1;
                    h_sdram_we    <= 1'b1;
                    h_sdram_addr  <= dst_baddr[22:2];
                    h_sdram_wstrb <= (4'b0001 << dst_baddr[1:0]);
                    h_sdram_wdata <= {4{key_val_code}};
                    dma_inv_we    <= 1'b1;
                    dma_inv_idx   <= dst_baddr[10:2];
                    state         <= HS_KEY_WR2;
                end

                HS_KEY_WR2: begin
                    if (sdram_ack) begin
                        h_sdram_req    <= 1'b0;
                        h_sdram_we     <= 1'b0;
                        ecall_write_a0 <= 1'b1;
                        ecall_ret_a0   <= 40'sd1;
                        ecall_done     <= 1'b1;
                        state          <= HS_RUN_CPU;
                    end
                end

                // -------------------------------------------------------------
                HS_ERR_SPIN: begin
                    if (msg_idx < msg_len) begin
                        if (!con_full) begin
                            con_fifo[con_tail] <= msg_char(msg_sel, msg_idx);
                            con_tail           <= con_tail + 8'd1;
                            msg_idx            <= msg_idx + 5'd1;
                        end
                    end
                end

                default: state <= HS_ERR_SPIN;
            endcase
        end
    end
    // -------------------------------------------------------------------------
    // Live Video Streamer (background, from the first Doom frame on)
    //
    // Frame packet over UART @ 3 Mbaud 8N1 (paced by MIN_TX_CYCLES so the BL616
    // USB-UART bridge never overflows):
    //   FE ED 52 36 | frame#lo | frame#hi | 768 palette bytes (from fb_mem[64000..])
    //   | escape-byte RLE pixels until 64000 decoded | checksum XOR
    //   | \r\n[DOOM FRAME]\r\n
    //
    // The streamer reads the shared Port A of the HDMI framebuffer BRAM; the CPU
    // never stalls on it. SYS_BLIT / SYS_PALETTE wait for v_stream_active to drop
    // before rewriting the framebuffer so a frame is never torn mid-stream.
    // -------------------------------------------------------------------------
    localparam [4:0]
        VID_IDLE      = 5'd0,
        VID_MAGIC     = 5'd1,
        VID_FRAME_LO  = 5'd2,
        VID_FRAME_HI  = 5'd3,
        VID_PAL       = 5'd4,
        VID_FB        = 5'd5,
        VID_CKSUM     = 5'd6,
        VID_TRAILER   = 5'd7;
    // (VID_LIT_*/VID_EMIT removed: payload is raw 64000 pixel bytes)

    reg [4:0]  v_state;
    reg        v_stream_active;
    reg  [1:0] v_magic_i;
    reg  [9:0] v_pal_i;
    reg [16:0] v_pix;
    reg  [7:0] v_run_val, v_run_cnt, v_next_val;
    reg  [7:0] v_cksum;
    reg  [2:0] v_emit_step;
    reg  [8:0] v_emit_left;
    reg        v_have_data;
    reg        v_emit_new;
    reg        v_ck_en;
    reg        v_tx_req;
    reg  [7:0] v_tx_data;
    reg  [7:0] v_trail_i;

    wire v_tx_idle = uart_tx_ready && !v_tx_req;

    always @(posedge clk) begin
        if (rst) begin
            v_state        <= VID_IDLE;
            v_stream_active <= 1'b0;
            v_tx_req      <= 1'b0;
            v_tx_data     <= 8'd0;
            v_magic_i     <= 2'd0;
            v_pal_i       <= 10'd0;
            v_pix         <= 17'd0;
            v_run_val     <= 8'd0;
            v_run_cnt     <= 8'd0;
            v_next_val    <= 8'd0;
            v_cksum      <= 8'd0;
            v_emit_step   <= 3'd0;
            v_emit_left   <= 9'd0;
            v_have_data   <= 1'b0;
            v_emit_new    <= 1'b0;
            v_ck_en      <= 1'b0;
            v_trail_i     <= 8'd0;
            fb_raddr      <= 16'd0;
        end else begin
            v_tx_req <= 1'b0;
            case (v_state)
                VID_IDLE: begin
                    if (v_start_capture) begin
                        v_stream_active <= 1'b1;
                        v_magic_i      <= 2'd0;
                        v_cksum        <= 8'd0;
                        v_state         <= VID_MAGIC;
                    end
                end

                VID_MAGIC: begin
                    if (v_tx_idle) begin
                        v_tx_req <= 1'b1;
                        case (v_magic_i)
                            2'd0: v_tx_data <= 8'hFE;
                            2'd1: v_tx_data <= 8'hED;
                            2'd2: v_tx_data <= 8'h52;
                            default: v_tx_data <= 8'h36;
                        endcase
                        if (v_magic_i == 2'd3) begin
                            v_ck_en <= 1'b1;
                            v_state  <= VID_FRAME_LO;
                        end else begin
                            v_magic_i <= v_magic_i + 2'd1;
                        end
                    end
                end

                VID_FRAME_LO: begin
                    if (v_tx_idle) begin
                        v_tx_req <= 1'b1;
                        v_tx_data <= frame_count[7:0];
                        v_cksum  <= v_cksum ^ frame_count[7:0];
                        v_state   <= VID_FRAME_HI;
                    end
                end

                VID_FRAME_HI: begin
                    if (v_tx_idle) begin
                        v_tx_req <= 1'b1;
                        v_tx_data <= frame_count[15:8];
                        v_cksum  <= v_cksum ^ frame_count[15:8];
                        v_pal_i  <= 10'd0;
                        v_have_data <= 1'b0;
                        v_state   <= VID_PAL;
                    end
                end

                VID_PAL: begin
                    if (v_pal_i >= 10'd768) begin
                        v_have_data <= 1'b0;
                        v_pix      <= 17'd0;
                        v_state     <= VID_FB;
                    end else if (v_tx_idle) begin
                        if (!v_have_data) begin
                            fb_raddr    <= 16'd64000 + {6'd0, v_pal_i};
                            v_have_data <= 1'b1;
                        end else begin
                            v_tx_req <= 1'b1;
                            v_tx_data <= fb_rdata;
                            v_cksum  <= v_cksum ^ fb_rdata;
                            v_pal_i  <= v_pal_i + 10'd1;
                            v_have_data <= 1'b0;
                        end
                    end
                end

                VID_FB: begin
                    // Raw 320x200 framebuffer: 64000 index bytes, end-to-end.
                    if (v_pix >= 17'd64000) begin
                        v_ck_en <= 1'b0;
                        v_state <= VID_CKSUM;
                    end else if (v_tx_idle) begin
                        if (!v_have_data) begin
                            fb_raddr    <= v_pix[15:0];
                            v_have_data <= 1'b1;
                        end else begin
                            v_tx_req <= 1'b1;
                            v_tx_data <= fb_rdata;
                            v_cksum  <= v_cksum ^ fb_rdata;
                            v_pix    <= v_pix + 17'd1;
                            v_have_data <= 1'b0;
                        end
                    end
                end

                VID_CKSUM: begin
                    if (v_tx_idle) begin
                        v_tx_req <= 1'b1;
                        v_tx_data <= v_cksum;
                        v_trail_i <= 8'd0;
                        v_state <= VID_TRAILER;
                    end
                end

                VID_TRAILER: begin
                    if (v_trail_i < 8'd16) begin
                        if (v_tx_idle) begin
                            v_tx_req <= 1'b1;
                            v_tx_data <= msg_char(2'd1, v_trail_i[4:0]);
                            v_trail_i <= v_trail_i + 8'd1;
                        end
                    end else if (v_trail_i == 8'd16) begin
                        if (v_tx_idle) begin
                            v_tx_req <= 1'b1;
                            v_tx_data <= v_pix[7:0]; // pixel count LSB (debug/verify)
                            v_trail_i <= v_trail_i + 8'd1;
                        end
                    end else if (v_trail_i == 8'd17) begin
                        if (v_tx_idle) begin
                            v_tx_req <= 1'b1;
                            v_tx_data <= v_pix[15:8]; // pixel count MSB
                            v_trail_i <= v_trail_i + 8'd1;
                        end
                    end else begin
                        v_stream_active <= 1'b0;
                        v_state <= VID_IDLE;
                    end
                end
            endcase
        end
    end

    assign uart_tx_req  = v_stream_active ? v_tx_req : c_uart_tx_req;
    assign uart_tx_data = v_stream_active ? v_tx_data : c_uart_tx_data;

    // Console TX FIFO drain: only while the video streamer is idle.
    always @(posedge clk) begin
        if (rst) begin
            c_uart_tx_req  <= 1'b0;
            c_uart_tx_data <= 8'd0;
        end else begin
            c_uart_tx_req <= 1'b0;
            if (!v_stream_active && !con_empty && uart_tx_ready && !c_uart_tx_req) begin
                c_uart_tx_req  <= 1'b1;
                c_uart_tx_data <= con_fifo[con_head];
                con_head <= con_head + 8'd1;
            end
        end
    end
endmodule

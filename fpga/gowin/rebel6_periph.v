// Streaming SPI NOR Flash Reader (XT25F64B) and 8N1 UART Transceiver for Tang Nano 20K.
//
// SPI Flash Reader:
//   - Issues 0xAB (Release Power-Down) + 40 us wake delay at reset so flash is
//     awake after FPGA bitstream configuration.
//   - Supports continuous sequential burst reads (0x03 + 24-bit address) at
//     clk / 2 (27 MHz -> 3.375 MB/s) for bootloading SDRAM and WAD DMA.
module rebel6_spiflash #(
    parameter integer FREQ = 54_000_000
) (
    input  wire        clk,
    input  wire        rst,

    output reg         ready,
    input  wire        start_read,
    input  wire [23:0] read_addr,
    input  wire        next_byte,
    input  wire        stop_read,
    output reg         byte_valid,
    output reg   [7:0] byte_data,

    output reg         spi_cs_n,
    output reg         spi_clk,
    output reg         spi_mosi,
    input  wire        spi_miso
);
    localparam integer WAKE_CYCLES = (FREQ / 1000000) * 40;

    localparam [2:0]
        ST_WAKE_CMD  = 3'd0,
        ST_WAKE_WAIT = 3'd1,
        ST_IDLE      = 3'd2,
        ST_CMD_ADDR  = 3'd3,
        ST_STREAM    = 3'd4,
        ST_SHIFT_IN  = 3'd5;

    reg  [2:0] state;
    reg [15:0] wait_cnt;
    reg [31:0] sh_out;
    reg  [5:0] bit_cnt;
    reg  [7:0] sh_in;
    reg        phase;
    reg        pend_next;

    always @(posedge clk) begin
        if (rst) begin
            state      <= ST_WAKE_CMD;
            ready      <= 1'b0;
            byte_valid <= 1'b0;
            byte_data  <= 8'd0;
            spi_cs_n   <= 1'b1;
            spi_clk    <= 1'b0;
            spi_mosi   <= 1'b0;
            sh_out     <= {8'hAB, 24'd0};
            bit_cnt    <= 6'd8;
            phase      <= 1'b0;
            pend_next  <= 1'b0;
            wait_cnt   <= 16'd0;
        end else begin
            byte_valid <= 1'b0;
            if (start_read || stop_read)
                pend_next <= 1'b0;
            else if (next_byte && state != ST_STREAM)
                pend_next <= 1'b1;
            case (state)
                ST_WAKE_CMD: begin
                    spi_cs_n <= 1'b0;
                    if (!phase) begin
                        spi_clk  <= 1'b0;
                        spi_mosi <= sh_out[31];
                        phase    <= 1'b1;
                    end else begin
                        spi_clk <= 1'b1;
                        sh_out  <= {sh_out[30:0], 1'b0};
                        phase   <= 1'b0;
                        if (bit_cnt == 6'd1) begin
                            state    <= ST_WAKE_WAIT;
                            wait_cnt <= 16'd0;
                        end else begin
                            bit_cnt <= bit_cnt - 6'd1;
                        end
                    end
                end

                ST_WAKE_WAIT: begin
                    spi_clk  <= 1'b0;
                    spi_cs_n <= 1'b1;
                    if (wait_cnt >= WAKE_CYCLES[15:0]) begin
                        ready <= 1'b1;
                        state <= ST_IDLE;
                    end else begin
                        wait_cnt <= wait_cnt + 16'd1;
                    end
                end

                ST_IDLE: begin
                    spi_cs_n <= 1'b1;
                    spi_clk  <= 1'b0;
                    ready    <= 1'b1;
                    if (start_read) begin
                        ready    <= 1'b0;
                        spi_cs_n <= 1'b0;
                        sh_out   <= {8'h03, read_addr};
                        bit_cnt  <= 6'd32;
                        phase    <= 1'b0;
                        state    <= ST_CMD_ADDR;
                    end
                end

                ST_CMD_ADDR: begin
                    if (!phase) begin
                        spi_clk  <= 1'b0;
                        spi_mosi <= sh_out[31];
                        phase    <= 1'b1;
                    end else begin
                        spi_clk <= 1'b1;
                        sh_out  <= {sh_out[30:0], 1'b0};
                        phase   <= 1'b0;
                        if (bit_cnt == 6'd1) begin
                            spi_clk <= 1'b1;
                            state   <= ST_STREAM;
                        end else begin
                            bit_cnt <= bit_cnt - 6'd1;
                        end
                    end
                end

                ST_STREAM: begin
                    spi_clk <= 1'b0;
                    if (stop_read) begin
                        spi_cs_n <= 1'b1;
                        ready    <= 1'b1;
                        state    <= ST_IDLE;
                    end else if (next_byte || pend_next) begin
                        pend_next <= 1'b0;
                        bit_cnt   <= 6'd8;
                        phase     <= 1'b0;
                        state     <= ST_SHIFT_IN;
                    end
                end

                ST_SHIFT_IN: begin
                    if (!phase) begin
                        spi_clk  <= 1'b0;
                        spi_mosi <= 1'b0;
                        phase    <= 1'b1;
                    end else begin
                        spi_clk <= 1'b1;
                        sh_in   <= {sh_in[6:0], spi_miso};
                        phase   <= 1'b0;
                        if (bit_cnt == 6'd1) begin
                            byte_data  <= {sh_in[6:0], spi_miso};
                            byte_valid <= 1'b1;
                            state      <= ST_STREAM;
                        end else begin
                            bit_cnt <= bit_cnt - 6'd1;
                        end
                    end
                end

                default: state <= ST_IDLE;
            endcase
        end
    end
endmodule

// Simple 8N1 UART TX + RX for console output (SYS_WRITE), telemetry and keyboard input (SYS_GETKEY).
module rebel6_uart #(
    parameter integer FREQ = 54_000_000,
    parameter integer BAUD = 115200,
    parameter integer MIN_TX_CYCLES = 0
) (
    input  wire       clk,
    input  wire       rst,

    // TX
    input  wire       tx_req,
    input  wire [7:0] tx_data,
    output wire       tx_ready,
    output reg        uart_tx,

    // RX
    input  wire       uart_rx,
    output reg        rx_valid,
    output reg  [7:0] rx_data,
    input  wire       rx_ack
);
    localparam integer DIV = FREQ / BAUD;

    // --- Transmitter ---
    reg [15:0] tx_div;
    reg  [3:0] tx_bits;
    reg  [9:0] tx_sh;
    reg [15:0] tx_guard;
    assign tx_ready = (tx_bits == 4'd0) && (tx_guard == 16'd0);

    always @(posedge clk) begin
        if (rst) begin
            tx_div  <= 16'd0;
            tx_bits <= 4'd0;
            tx_sh   <= 10'h3FF;
            tx_guard <= 16'd0;
            uart_tx <= 1'b1;
        end else if (tx_bits == 4'd0) begin
            uart_tx <= 1'b1;
            if (tx_req) begin
                tx_sh   <= {1'b1, tx_data, 1'b0};
                tx_bits <= 4'd10;
                tx_div  <= 16'd0;
                tx_guard <= MIN_TX_CYCLES[15:0];
            end else if (tx_guard != 16'd0) begin
                tx_guard <= tx_guard - 16'd1;
            end
        end else begin
            uart_tx <= tx_sh[0];
            if (tx_div >= (DIV - 1)) begin
                tx_div  <= 16'd0;
                tx_sh   <= {1'b1, tx_sh[9:1]};
                tx_bits <= tx_bits - 4'd1;
            end else begin
                tx_div <= tx_div + 16'd1;
            end
        end
    end

    // --- Receiver ---
    reg        rx_sync0, rx_sync1;
    reg [15:0] rx_div;
    reg  [3:0] rx_bits;
    reg  [7:0] rx_sh;

    always @(posedge clk) begin
        if (rst) begin
            rx_sync0 <= 1'b1;
            rx_sync1 <= 1'b1;
            rx_div   <= 16'd0;
            rx_bits  <= 4'd0;
            rx_valid <= 1'b0;
            rx_data  <= 8'd0;
        end else begin
            rx_sync0 <= uart_rx;
            rx_sync1 <= rx_sync0;

            if (rx_ack)
                rx_valid <= 1'b0;

            if (rx_bits == 4'd0) begin
                if (!rx_sync1) begin
                    rx_div  <= DIV[15:0] >> 1; // sample mid-start-bit
                    rx_bits <= 4'd1;
                end
            end else if (rx_div >= (DIV - 1)) begin
                rx_div <= 16'd0;
                if (rx_bits == 4'd1) begin
                    // Verify start bit still low
                    if (!rx_sync1)
                        rx_bits <= 4'd2;
                    else
                        rx_bits <= 4'd0;
                end else if (rx_bits <= 4'd9) begin
                    rx_sh   <= {rx_sync1, rx_sh[7:1]};
                    rx_bits <= rx_bits + 4'd1;
                end else begin
                    if (rx_sync1) begin
                        rx_data  <= rx_sh;
                        rx_valid <= 1'b1;
                    end
                    rx_bits <= 4'd0;
                end
            end else begin
                rx_div <= rx_div + 16'd1;
            end
        end
    end
endmodule

// Direct-mapped 512-entry x 32-bit Write-Through Data Cache (2 KiB) in 2 DPX9B B-SRAMs.
//
// Speeds up stack, globals and Doom lookup tables to 2-cycle hits while keeping
// SDRAM continuously coherent via write-through and per-word DMA invalidation.
// Line layout (45 bits): {valid[1], tag[11:0], data[31:0]}.
module rebel6_dcache (
    input  wire        clk,
    input  wire        rst,
    input  wire        flush_req,
    output wire        flush_busy,

    // Single-cycle line invalidation driven by SoC WAD DMA writes
    input  wire        dma_inv_we,
    input  wire  [8:0] dma_inv_idx,

    // CPU dmem port
    input  wire        dmem_req,
    input  wire        dmem_we,
    input  wire [23:0] dmem_addr,
    input  wire  [3:0] dmem_wstrb,
    input  wire [31:0] dmem_wdata,
    output reg  [31:0] dmem_rdata,
    output reg         dmem_ready,

    // Backing SDRAM 32-bit R/W port
    output reg         sdram_req,
    output reg         sdram_we,
    output reg  [20:0] sdram_word_addr,
    output reg   [3:0] sdram_wstrb,
    output reg  [31:0] sdram_wdata,
    input  wire [31:0] sdram_rdata,
    input  wire        sdram_ack
);
    (* ram_style = "block" *)
    reg [44:0] mem [0:511];

    reg        bram_we;
    reg  [8:0] bram_waddr;
    reg [44:0] bram_din;
    reg  [8:0] bram_raddr;
    reg [44:0] bram_dout;

    always @(posedge clk) begin
        if (bram_we)
            mem[bram_waddr] <= bram_din;
        bram_dout <= mem[bram_raddr];
    end

    localparam [2:0]
        ST_FLUSH = 3'd0,
        ST_IDLE  = 3'd1,
        ST_CHECK = 3'd2,
        ST_READ  = 3'd3,
        ST_WRITE = 3'd4;

    reg  [2:0] state;
    reg  [8:0] flush_idx;
    reg [20:0] req_waddr;
    reg        req_we;
    reg  [3:0] req_wstrb;
    reg [31:0] req_wdata;

    assign flush_busy = (state == ST_FLUSH);

    always @* begin
        bram_raddr = dmem_addr[10:2];
        if (state != ST_IDLE)
            bram_raddr = req_waddr[8:0];
    end

    wire        line_valid = bram_dout[44];
    wire [11:0] line_tag   = bram_dout[43:32];
    wire [31:0] line_data  = bram_dout[31:0];

    wire [31:0] merged_wdata = {
        req_wstrb[3] ? req_wdata[31:24] : line_data[31:24],
        req_wstrb[2] ? req_wdata[23:16] : line_data[23:16],
        req_wstrb[1] ? req_wdata[15:8]  : line_data[15:8],
        req_wstrb[0] ? req_wdata[7:0]   : line_data[7:0]
    };

    always @(posedge clk) begin
        if (rst) begin
            state           <= ST_FLUSH;
            flush_idx       <= 9'd0;
            dmem_ready      <= 1'b0;
            dmem_rdata      <= 32'd0;
            sdram_req       <= 1'b0;
            sdram_we        <= 1'b0;
            sdram_word_addr <= 21'd0;
            sdram_wstrb     <= 4'd0;
            sdram_wdata     <= 32'd0;
            bram_we         <= 1'b0;
            bram_waddr      <= 9'd0;
            bram_din        <= 45'd0;
        end else begin
            dmem_ready <= 1'b0;
            bram_we    <= 1'b0;

            if (flush_req && state != ST_FLUSH) begin
                state     <= ST_FLUSH;
                flush_idx <= 9'd0;
                sdram_req <= 1'b0;
            end else if (dma_inv_we && state == ST_IDLE) begin
                bram_we    <= 1'b1;
                bram_waddr <= dma_inv_idx;
                bram_din   <= 45'd0;
            end else begin
                case (state)
                    ST_FLUSH: begin
                        bram_we    <= 1'b1;
                        bram_waddr <= flush_idx;
                        bram_din   <= 45'd0;
                        if (flush_idx == 9'd511)
                            state <= ST_IDLE;
                        else
                            flush_idx <= flush_idx + 9'd1;
                    end

                    ST_IDLE: begin
                        if (dmem_req && !dmem_ready) begin
                            req_waddr <= dmem_addr[22:2];
                            req_we    <= dmem_we;
                            req_wstrb <= dmem_wstrb;
                            req_wdata <= dmem_wdata;
                            state     <= ST_CHECK;
                        end
                    end

                    ST_CHECK: begin
                        if (!req_we) begin
                            if (line_valid && (line_tag == req_waddr[20:9])) begin
                                dmem_rdata <= line_data;
                                dmem_ready <= 1'b1;
                                state      <= ST_IDLE;
                            end else begin
                                sdram_req       <= 1'b1;
                                sdram_we        <= 1'b0;
                                sdram_word_addr <= req_waddr;
                                sdram_wstrb     <= 4'b0000;
                                state           <= ST_READ;
                            end
                        end else begin
                            // Write-through to SDRAM; update cache on full-word write or tag hit
                            if (req_wstrb == 4'b1111) begin
                                bram_we    <= 1'b1;
                                bram_waddr <= req_waddr[8:0];
                                bram_din   <= {1'b1, req_waddr[20:9], req_wdata};
                            end else if (line_valid && (line_tag == req_waddr[20:9])) begin
                                bram_we    <= 1'b1;
                                bram_waddr <= req_waddr[8:0];
                                bram_din   <= {1'b1, req_waddr[20:9], merged_wdata};
                            end
                            sdram_req       <= 1'b1;
                            sdram_we        <= 1'b1;
                            sdram_word_addr <= req_waddr;
                            sdram_wstrb     <= req_wstrb;
                            sdram_wdata     <= req_wdata;
                            state           <= ST_WRITE;
                        end
                    end

                    ST_READ: begin
                        if (sdram_ack) begin
                            sdram_req  <= 1'b0;
                            bram_we    <= 1'b1;
                            bram_waddr <= req_waddr[8:0];
                            bram_din   <= {1'b1, req_waddr[20:9], sdram_rdata};
                            dmem_rdata <= sdram_rdata;
                            dmem_ready <= 1'b1;
                            state      <= ST_IDLE;
                        end
                    end

                    ST_WRITE: begin
                        if (sdram_ack) begin
                            sdram_req  <= 1'b0;
                            sdram_we   <= 1'b0;
                            dmem_ready <= 1'b1;
                            state      <= ST_IDLE;
                        end
                    end

                    default: state <= ST_IDLE;
                endcase
            end
        end
    end
endmodule

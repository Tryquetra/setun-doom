// Direct-mapped 1,024-entry x 64-bit Instruction Cache (8 KiB) in 4 DPX9B B-SRAMs.
//
// Read-only Harvard instruction cache backed by 32-bit SDRAM words starting at
// SDRAM_IMEM_WORD_BASE (default 21'h1A0000 = byte offset 6.50 MiB).
// Line layout (72 bits): {valid[1], tag[6:0], instr64[63:0]}.
module rebel6_icache #(
    parameter [20:0] SDRAM_IMEM_WORD_BASE = 21'h1A0000
) (
    input  wire        clk,
    input  wire        rst,
    input  wire        flush_req,
    output wire        flush_busy,

    // CPU imem port
    input  wire        imem_req,
    input  wire [19:0] imem_addr,
    output reg  [63:0] imem_rdata,
    output reg         imem_ready,

    // Backing SDRAM 32-bit read port
    output reg         sdram_req,
    output reg  [20:0] sdram_word_addr,
    input  wire [31:0] sdram_rdata,
    input  wire        sdram_ack
);
    (* ram_style = "block" *)
    reg [71:0] mem [0:1023];

    reg        bram_we;
    reg  [9:0] bram_waddr;
    reg [71:0] bram_din;
    reg  [9:0] bram_raddr;
    reg [71:0] bram_dout;

    always @(posedge clk) begin
        if (bram_we)
            mem[bram_waddr] <= bram_din;
        bram_dout <= mem[bram_raddr];
    end

    localparam [2:0]
        ST_FLUSH = 3'd0,
        ST_IDLE  = 3'd1,
        ST_CHECK = 3'd2,
        ST_FILL0 = 3'd3,
        ST_FILL1 = 3'd4;

    reg  [2:0] state;
    reg  [9:0] flush_idx;
    reg [16:0] req_pc;
    reg [31:0] lo32;

    assign flush_busy = (state == ST_FLUSH);

    always @* begin
        bram_raddr = imem_addr[9:0];
        if (state != ST_IDLE)
            bram_raddr = req_pc[9:0];
    end

    wire        line_valid = bram_dout[71];
    wire  [6:0] line_tag   = bram_dout[70:64];
    wire [63:0] line_data  = bram_dout[63:0];

    always @(posedge clk) begin
        if (rst) begin
            state           <= ST_FLUSH;
            flush_idx       <= 10'd0;
            imem_ready      <= 1'b0;
            imem_rdata      <= 64'd0;
            sdram_req       <= 1'b0;
            sdram_word_addr <= 21'd0;
            bram_we         <= 1'b0;
            bram_waddr      <= 10'd0;
            bram_din        <= 72'd0;
        end else begin
            imem_ready <= 1'b0;
            bram_we    <= 1'b0;

            if (flush_req && state != ST_FLUSH) begin
                state     <= ST_FLUSH;
                flush_idx <= 10'd0;
                sdram_req <= 1'b0;
            end else begin
                case (state)
                    ST_FLUSH: begin
                        bram_we    <= 1'b1;
                        bram_waddr <= flush_idx;
                        bram_din   <= 72'd0;
                        if (flush_idx == 10'd1023)
                            state <= ST_IDLE;
                        else
                            flush_idx <= flush_idx + 10'd1;
                    end

                    ST_IDLE: begin
                        if (imem_req && !imem_ready) begin
                            req_pc <= imem_addr[16:0];
                            state  <= ST_CHECK;
                        end
                    end

                    ST_CHECK: begin
                        if (line_valid && (line_tag == req_pc[16:10])) begin
                            imem_rdata <= line_data;
                            imem_ready <= 1'b1;
                            state      <= ST_IDLE;
                        end else begin
                            sdram_req       <= 1'b1;
                            sdram_word_addr <= SDRAM_IMEM_WORD_BASE + {3'd0, req_pc, 1'b0};
                            state           <= ST_FILL0;
                        end
                    end

                    ST_FILL0: begin
                        if (sdram_ack) begin
                            lo32      <= sdram_rdata;
                            sdram_req <= 1'b0;
                            state     <= ST_FILL1;
                        end
                    end

                    ST_FILL1: begin
                        if (!sdram_req) begin
                            sdram_req       <= 1'b1;
                            sdram_word_addr <= SDRAM_IMEM_WORD_BASE + {3'd0, req_pc, 1'b1};
                        end else if (sdram_ack) begin
                            sdram_req  <= 1'b0;
                            bram_we    <= 1'b1;
                            bram_waddr <= req_pc[9:0];
                            bram_din   <= {1'b1, req_pc[16:10], sdram_rdata, lo32};
                            imem_rdata <= {sdram_rdata, lo32};
                            imem_ready <= 1'b1;
                            state      <= ST_IDLE;
                        end
                    end

                    default: state <= ST_IDLE;
                endcase
            end
        end
    end
endmodule

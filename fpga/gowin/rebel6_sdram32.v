// 32-bit Synchronous Controller for the GW2AR-18 Embedded 64 Mbit (8 MiB) SDRAM.
//
// Organization: 4 banks x 2048 rows x 256 columns x 32 bits = 8,388,608 bytes.
// Features:
//   - Automatic 200 us power-on initialization (Precharge All, 2x Auto-Refresh,
//     Mode Register Set CAS=2, Burst=1)
//   - Automatic 15 us background Auto-Refresh timer arbitrated cleanly in IDLE
//   - Full 32-bit word reads and byte-masked writes (wstrb[3:0] -> ~SDRAM_DQM)
//   - Registered rdata and 1-cycle ack pulse for high Fmax
module rebel6_sdram32 #(
    parameter integer FREQ  = 54_000_000,
    parameter [3:0]   CAS   = 4'd2,
    parameter [3:0]   T_WR  = 4'd2,
    parameter [3:0]   T_MRD = 4'd2,
    parameter [3:0]   T_RP  = 4'd1,
    parameter [3:0]   T_RCD = 4'd1,
    parameter [3:0]   T_RC  = 4'd4
) (
    input  wire        clk,
    input  wire        clk_sdram,
    input  wire        rst,

    // System bus (word_addr = byte_addr[22:2])
    input  wire        req,
    input  wire        we,
    input  wire [20:0] word_addr,
    input  wire  [3:0] wstrb,
    input  wire [31:0] wdata,
    output reg  [31:0] rdata,
    output reg         ack,
    output reg         init_done,

    // Physical embedded SiP SDRAM pins
    output wire        O_sdram_clk,
    output wire        O_sdram_cke,
    output wire        O_sdram_cs_n,
    output reg         O_sdram_cas_n,
    output reg         O_sdram_ras_n,
    output reg         O_sdram_wen_n,
    output reg   [3:0] O_sdram_dqm,
    output reg  [10:0] O_sdram_addr,
    output reg   [1:0] O_sdram_ba,
    inout  wire [31:0] IO_sdram_dq
);
    assign O_sdram_clk  = clk_sdram;
    assign O_sdram_cke  = 1'b1;
    assign O_sdram_cs_n = 1'b0;

    reg        dq_oen;
    reg [31:0] dq_out;
    assign IO_sdram_dq = dq_oen ? 32'hZZZZ_ZZZZ : dq_out;
    wire [31:0] dq_in  = IO_sdram_dq;

    localparam [2:0]
        CMD_MRS     = 3'b000,
        CMD_REFRESH = 3'b001,
        CMD_PRECHG  = 3'b010,
        CMD_ACTIVE  = 3'b011,
        CMD_WRITE   = 3'b100,
        CMD_READ    = 3'b101,
        CMD_NOP     = 3'b111;

    localparam [10:0] MODE_REG = {4'b0000, CAS[2:0], 1'b0, 3'b000};

    localparam [2:0]
        ST_INIT    = 3'd0,
        ST_CONFIG  = 3'd1,
        ST_IDLE    = 3'd2,
        ST_READ    = 3'd3,
        ST_WRITE   = 3'd4,
        ST_REFRESH = 3'd5;

    localparam integer INIT_CYCLES    = (FREQ / 1000) * 200 / 1000;
    localparam integer REFRESH_CYCLES = (FREQ >= 5_000_000) ? ((FREQ / 1000000) * 14) : 512;

    reg  [2:0] state;
    reg  [3:0] cycle;
    reg [15:0] init_cnt;
    reg [11:0] ref_cnt;
    reg        ref_pending;

    reg  [7:0] col_q;
    reg  [3:0] wstrb_q;
    reg [31:0] wdata_q;

    always @(posedge clk) begin
        if (rst) begin
            state         <= ST_INIT;
            cycle         <= 4'd0;
            init_cnt      <= 16'd0;
            ref_cnt       <= 12'd0;
            ref_pending   <= 1'b0;
            init_done     <= 1'b0;
            ack           <= 1'b0;
            rdata         <= 32'd0;
            dq_oen        <= 1'b1;
            dq_out        <= 32'd0;
            O_sdram_dqm   <= 4'b1111;
            O_sdram_addr  <= 11'd0;
            O_sdram_ba    <= 2'd0;
            {O_sdram_ras_n, O_sdram_cas_n, O_sdram_wen_n} <= CMD_NOP;
        end else begin
            ack <= 1'b0;
            {O_sdram_ras_n, O_sdram_cas_n, O_sdram_wen_n} <= CMD_NOP;

            // Periodic auto-refresh timer once initialized
            if (init_done) begin
                if (ref_cnt >= REFRESH_CYCLES[11:0]) begin
                    ref_cnt     <= 12'd0;
                    ref_pending <= 1'b1;
                end else begin
                    ref_cnt <= ref_cnt + 12'd1;
                end
            end

            case (state)
                ST_INIT: begin
                    if (init_cnt >= INIT_CYCLES[15:0]) begin
                        state <= ST_CONFIG;
                        cycle <= 4'd0;
                    end else begin
                        init_cnt <= init_cnt + 16'd1;
                    end
                end

                ST_CONFIG: begin
                    cycle <= cycle + 4'd1;
                    if (cycle == 4'd0) begin
                        {O_sdram_ras_n, O_sdram_cas_n, O_sdram_wen_n} <= CMD_PRECHG;
                        O_sdram_addr[10] <= 1'b1;
                    end else if (cycle == T_RP) begin
                        {O_sdram_ras_n, O_sdram_cas_n, O_sdram_wen_n} <= CMD_REFRESH;
                    end else if (cycle == (T_RP + T_RC)) begin
                        {O_sdram_ras_n, O_sdram_cas_n, O_sdram_wen_n} <= CMD_REFRESH;
                    end else if (cycle == (T_RP + T_RC + T_RC)) begin
                        {O_sdram_ras_n, O_sdram_cas_n, O_sdram_wen_n} <= CMD_MRS;
                        O_sdram_addr <= MODE_REG;
                    end else if (cycle == (T_RP + T_RC + T_RC + T_MRD)) begin
                        init_done <= 1'b1;
                        state     <= ST_IDLE;
                    end
                end

                ST_IDLE: begin
                    dq_oen <= 1'b1;
                    if (ref_pending) begin
                        ref_pending <= 1'b0;
                        {O_sdram_ras_n, O_sdram_cas_n, O_sdram_wen_n} <= CMD_REFRESH;
                        cycle <= 4'd1;
                        state <= ST_REFRESH;
                    end else if (req && !ack) begin
                        {O_sdram_ras_n, O_sdram_cas_n, O_sdram_wen_n} <= CMD_ACTIVE;
                        O_sdram_ba   <= word_addr[20:19];
                        O_sdram_addr <= word_addr[18:8];
                        col_q        <= word_addr[7:0];
                        wstrb_q      <= wstrb;
                        wdata_q      <= wdata;
                        cycle        <= 4'd1;
                        state        <= we ? ST_WRITE : ST_READ;
                    end
                end

                ST_READ: begin
                    cycle <= cycle + 4'd1;
                    if (cycle == T_RCD) begin
                        {O_sdram_ras_n, O_sdram_cas_n, O_sdram_wen_n} <= CMD_READ;
                        O_sdram_addr <= {1'b1, 2'b00, col_q}; // auto-precharge
                        O_sdram_dqm  <= 4'b0000;
                    end else if (cycle == (T_RCD + CAS)) begin
                        rdata <= dq_in;
                    end else if (cycle == (T_RCD + CAS + 4'd1)) begin
                        ack        <= 1'b1;
                        state      <= ST_IDLE;
                    end
                end

                ST_WRITE: begin
                    cycle <= cycle + 4'd1;
                    if (cycle == T_RCD) begin
                        {O_sdram_ras_n, O_sdram_cas_n, O_sdram_wen_n} <= CMD_WRITE;
                        O_sdram_addr <= {1'b1, 2'b00, col_q}; // auto-precharge
                        O_sdram_dqm  <= ~wstrb_q;
                        dq_out       <= wdata_q;
                        dq_oen       <= 1'b0;
                    end else if (cycle == (T_RCD + 4'd1)) begin
                        dq_oen      <= 1'b1;
                        O_sdram_dqm <= 4'b1111;
                    end else if (cycle == (T_RCD + T_WR + T_RP)) begin
                        ack        <= 1'b1;
                        state      <= ST_IDLE;
                    end
                end

                ST_REFRESH: begin
                    cycle <= cycle + 4'd1;
                    if (cycle == T_RC) begin
                        state <= ST_IDLE;
                    end
                end

                default: state <= ST_IDLE;
            endcase
        end
    end
endmodule

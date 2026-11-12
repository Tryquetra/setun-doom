// Synthesizable REBEL-6 CPU core implementing all 73 architectural instructions.
//
// Execution model matches src/rebel6/cpu.rs:
//   - 729 registers of 24 balanced trits (represented as signed 40-bit integers
//     in [-HALF, +HALF] inside dual-bank B-SRAM, X0 hardwired to zero)
//   - 32-trit instructions fetched over the 64-bit Harvard imem bus
//   - Byte-addressed little-endian data memory over the 32-bit dmem bus (with
//     transparent support for both aligned and cross-word unaligned accesses)
//   - Hardware ecall handshake exposing a7/a0/a1/a2 and accepting return a0
//   - Hardware trap causes 0..5 matching docs/abi.md section 4.4
module rebel6_core (
    input  wire               clk,
    input  wire               rst,

    // Boot / host control
    input  wire               start_pulse,
    input  wire signed [39:0] start_pc,
    input  wire        [31:0] instr_count,
    input  wire        [23:0] dmem_limit,
    input  wire               host_rf_we,
    input  wire         [9:0] host_rf_waddr,
    input  wire signed [39:0] host_rf_wdata,

    // Harvard instruction memory port (64-bit packed 32-trit slot)
    output reg                imem_req,
    output wire        [19:0] imem_addr,
    input  wire        [63:0] imem_rdata,
    input  wire               imem_ready,

    // Byte-addressed data memory port (32-bit aligned word bus)
    output reg                dmem_req,
    output reg                dmem_we,
    output reg         [23:0] dmem_addr,
    output reg          [3:0] dmem_wstrb,
    output reg         [31:0] dmem_wdata,
    input  wire        [31:0] dmem_rdata,
    input  wire               dmem_ready,

    // ECALL host/SoC service interface
    output reg                ecall_req,
    output wire signed [39:0] ecall_a7,
    output wire        [31:0] ecall_a0,
    output wire        [31:0] ecall_a1,
    output wire        [31:0] ecall_a2,
    input  wire               ecall_done,
    input  wire               ecall_write_a0,
    input  wire signed [39:0] ecall_ret_a0,
    input  wire               ecall_halt,
    input  wire signed [39:0] ecall_exit_code,

    // Status and telemetry
    output reg                running,
    output reg                halted,
    output reg                trapped,
    output reg          [2:0] trap_cause,
    output wire signed [39:0] pc_out,
    output reg         [63:0] cycles_out,
    output reg  signed [39:0] exit_code_out
);
    localparam [3:0]
        S_IDLE      = 4'd0,
        S_FETCH     = 4'd1,
        S_DEC       = 4'd2,
        S_EXEC      = 4'd3,
        S_MEM1      = 4'd4,
        S_MEM2      = 4'd5,
        S_TRIT_RS3  = 4'd6,
        S_TRIT_WAIT = 4'd7,
        S_WB2       = 4'd8,
        S_ECALL     = 4'd9;

    reg [3:0]          stage;
    reg signed [39:0]  pc;
    reg        [63:0]  instr;

    assign pc_out    = pc;
    assign imem_addr = pc[19:0];

    // Combinational instruction decoder over latched instr
    wire               dec_valid;
    wire               dec_err11;
    wire         [6:0] dec_op;
    wire signed  [9:0] dec_rs1, dec_rs2, dec_rs3, dec_rd1, dec_rd2;
    wire signed [47:0] dec_imm48, dec_imm2_48;
    wire signed  [1:0] dec_fill;
    wire               dec_fmt_long;

    rebel6_decoder u_dec (
        .instr(instr),
        .valid(dec_valid),
        .err_11(dec_err11),
        .op(dec_op),
        .rs1(dec_rs1),
        .rs2(dec_rs2),
        .rs3(dec_rs3),
        .rd1(dec_rd1),
        .rd2(dec_rd2),
        .imm(dec_imm48),
        .imm2(dec_imm2_48),
        .fill(dec_fill),
        .fmt_long(dec_fmt_long)
    );

    // Latched decoded fields
    reg         [6:0] d_op;
    reg signed  [9:0] d_rs1, d_rs2, d_rs3, d_rd1, d_rd2;
    reg signed [39:0] d_imm, d_imm2;
    reg signed  [1:0] d_fill;

    // Register file (729 x 40-bit dual-bank BRAM)
    reg         [9:0] rf_ra_a, rf_ra_b;
    reg         [9:0] rf_wa;
    reg signed [39:0] rf_wd;
    reg               rf_we;
    wire signed [39:0] rf_rd_a, rf_rd_b;

    wire         [9:0] mux_rf_wa = (!running && host_rf_we) ? host_rf_waddr : rf_wa;
    wire signed [39:0] mux_rf_wd = (!running && host_rf_we) ? host_rf_wdata : rf_wd;
    wire               mux_rf_we = (!running && host_rf_we) ? 1'b1          : rf_we;

    always @* begin
        if (stage == S_DEC) begin
            rf_ra_a = dec_rs1[9:0];
            rf_ra_b = dec_rs2[9:0];
        end else if (stage == S_EXEC && (d_op == 7'd33 || d_op == 7'd34)) begin
            rf_ra_a = d_rs3[9:0];
            rf_ra_b = d_rs2[9:0];
        end else begin
            rf_ra_a = d_rs1[9:0];
            rf_ra_b = d_rs2[9:0];
        end
    end

    rebel6_regfile_dualbank #(.DEPTH(729), .W(40)) u_rf (
        .clk(clk),
        .raddr_a(rf_ra_a),
        .raddr_b(rf_ra_b),
        .waddr(mux_rf_wa),
        .din(mux_rf_wd),
        .we(mux_rf_we),
        .dout_a(rf_rd_a),
        .dout_b(rf_rd_b)
    );

    // Shadow copies of ABI syscall registers x10 (a0), x11 (a1), x12 (a2), x17 (a7)
    reg        [31:0] sh_a0, sh_a1, sh_a2;
    reg signed [39:0] sh_a7;
    assign ecall_a0 = sh_a0;
    assign ecall_a1 = sh_a1;
    assign ecall_a2 = sh_a2;
    assign ecall_a7 = sh_a7;

    always @(posedge clk) begin
        if (rst) begin
            sh_a0 <= 32'd0;
            sh_a1 <= 32'd0;
            sh_a2 <= 32'd0;
            sh_a7 <= 40'sd0;
        end else if (mux_rf_we) begin
            case (mux_rf_wa)
                10'd10: sh_a0 <= mux_rf_wd[31:0];
                10'd11: sh_a1 <= mux_rf_wd[31:0];
                10'd12: sh_a2 <= mux_rf_wd[31:0];
                10'd17: sh_a7 <= mux_rf_wd;
                default: ;
            endcase
        end
    end

    // Combinational ALU (W=40)
    reg         [4:0] alu_op;
    reg signed [39:0] alu_a, alu_b;
    wire signed [39:0] alu_res;
    wire              alu_eq, alu_lt, alu_ltu32;

    rebel6_alu #(.W(40)) u_alu (
        .op(alu_op),
        .a(alu_a),
        .b(alu_b),
        .res(alu_res),
        .eq(alu_eq),
        .lt(alu_lt),
        .ltu32(alu_ltu32)
    );

    // Sequential Tritwise/Shift/Rotate/Majority/Minority unit
    reg               trit_start;
    reg         [3:0] trit_mode;
    reg signed [39:0] trit_a, trit_b, trit_c;
    wire signed [39:0] trit_res;
    wire              trit_done;

    rebel6_trit_unit u_trit (
        .clk(clk),
        .rst(rst),
        .start(trit_start),
        .mode(trit_mode),
        .op_a(trit_a),
        .op_b(trit_b),
        .op_c(trit_c),
        .shamt_imm(d_imm),
        .fill(d_fill),
        .result(trit_res),
        .done(trit_done)
    );

    // Register index usage check (matching cpu.rs: negative 6-trit index -> InvalidRegister)
    reg uses_rs1, uses_rs2, uses_rs3, uses_rd1, uses_rd2;
    always @* begin
        uses_rs1 = 1'b0; uses_rs2 = 1'b0; uses_rs3 = 1'b0;
        uses_rd1 = 1'b0; uses_rd2 = 1'b0;
        case (dec_op)
            // Format R binary/ternary 2-src 1-dst
            7'd0, 7'd1, 7'd2, 7'd3, 7'd4, 7'd5, 7'd6, 7'd7,
            7'd8, 7'd11, 7'd12,
            7'd46, 7'd47, 7'd48, 7'd49, 7'd50, 7'd51, 7'd52, 7'd53, 7'd54: begin
                uses_rs1 = 1'b1; uses_rs2 = 1'b1; uses_rd1 = 1'b1;
            end
            // sti.t: 1-src 1-dst
            7'd9: begin
                uses_rs1 = 1'b1; uses_rd1 = 1'b1;
            end
            // mv2.t: 2-src 2-dst
            7'd10: begin
                uses_rs1 = 1'b1; uses_rs2 = 1'b1; uses_rd1 = 1'b1; uses_rd2 = 1'b1;
            end
            // Format I: 1-src 1-dst
            7'd13, 7'd14, 7'd15, 7'd16, 7'd17, 7'd18, 7'd19,
            7'd20, 7'd21, 7'd22, 7'd23,
            7'd55, 7'd56, 7'd57, 7'd58, 7'd59, 7'd60, 7'd61, 7'd62,
            7'd63, 7'd64, 7'd65, 7'd66, 7'd67: begin
                uses_rs1 = 1'b1; uses_rd1 = 1'b1;
            end
            // Format B: 2-src 0-dst
            7'd24, 7'd25, 7'd26, 7'd27, 7'd28, 7'd29,
            7'd30, 7'd31, 7'd32,
            7'd68, 7'd69, 7'd70, 7'd71, 7'd72: begin
                uses_rs1 = 1'b1; uses_rs2 = 1'b1;
            end
            // Format D: 3-src 1-dst
            7'd33, 7'd34: begin
                uses_rs1 = 1'b1; uses_rs2 = 1'b1; uses_rs3 = 1'b1; uses_rd1 = 1'b1;
            end
            // li2.t: 0-src 2-dst
            7'd35: begin
                uses_rd1 = 1'b1; uses_rd2 = 1'b1;
            end
            // lwa.t, li.t, jal.t, aipc.t: 0-src 1-dst
            7'd41, 7'd42, 7'd44, 7'd45: begin
                uses_rd1 = 1'b1;
            end
            // swa.t: 1-src 0-dst
            7'd43: begin
                uses_rs1 = 1'b1;
            end
            default: ;
        endcase
    end

    wire bad_reg = (uses_rs1 && dec_rs1[9]) ||
                   (uses_rs2 && dec_rs2[9]) ||
                   (uses_rs3 && dec_rs3[9]) ||
                   (uses_rd1 && dec_rd1[9]) ||
                   (uses_rd2 && dec_rd2[9]);

    // ALU input / op steering in S_EXEC
    always @* begin
        alu_op = 5'd0;
        alu_a  = rf_rd_a;
        alu_b  = rf_rd_b;
        case (d_op)
            7'd0:  alu_op = 5'd0;  // add.t
            7'd1:  alu_op = 5'd1;  // sub.t
            7'd4:  alu_op = 5'd2;  // slt.t
            7'd8:  alu_op = 5'd5;  // cmp.t
            7'd9:  alu_op = 5'd15; // sti.t
            7'd11: alu_op = 5'd3;  // min.t
            7'd12: alu_op = 5'd4;  // max.t
            7'd13: begin alu_op = 5'd0; alu_b = d_imm; end // addi.t
            7'd16: begin alu_op = 5'd2; alu_b = d_imm; end // slti.t
            7'd23, 7'd44: begin alu_op = 5'd0; alu_a = pc; alu_b = 40'sd1; end // jalr.t, jal.t link = wrap(pc+1)
            7'd42: begin alu_op = 5'd16; alu_b = d_imm; end // li.t
            7'd45: begin alu_op = 5'd0; alu_a = pc; alu_b = d_imm; end // aipc.t
            7'd46: alu_op = 5'd6;  // add
            7'd47: alu_op = 5'd7;  // sub
            7'd48: alu_op = 5'd8;  // sll
            7'd49: alu_op = 5'd9;  // srl
            7'd50: alu_op = 5'd10; // sra
            7'd51: alu_op = 5'd11; // sltu
            7'd52: alu_op = 5'd12; // or
            7'd53: alu_op = 5'd13; // xor
            7'd54: alu_op = 5'd14; // and
            7'd55: begin alu_op = 5'd6;  alu_b = d_imm; end // addi
            7'd56: begin alu_op = 5'd8;  alu_b = d_imm; end // slli
            7'd57: begin alu_op = 5'd9;  alu_b = d_imm; end // srli
            7'd58: begin alu_op = 5'd10; alu_b = d_imm; end // srai
            7'd59: begin alu_op = 5'd11; alu_b = d_imm; end // sltiu
            7'd60: begin alu_op = 5'd12; alu_b = d_imm; end // ori
            7'd61: begin alu_op = 5'd13; alu_b = d_imm; end // xori
            7'd62: begin alu_op = 5'd14; alu_b = d_imm; end // andi
            default: ;
        endcase
    end

    // Memory decode helpers
    wire is_load  = (d_op == 7'd20 || d_op == 7'd21 || d_op == 7'd22 || d_op == 7'd41 ||
                     d_op == 7'd63 || d_op == 7'd64 || d_op == 7'd65 || d_op == 7'd66 || d_op == 7'd67);
    wire is_store = (d_op == 7'd30 || d_op == 7'd31 || d_op == 7'd32 || d_op == 7'd43 ||
                     d_op == 7'd70 || d_op == 7'd71 || d_op == 7'd72);
    wire is_abs_mem = (d_op == 7'd41 || d_op == 7'd43);

    reg [2:0] mem_size;
    reg       mem_unsigned;
    always @* begin
        mem_size     = 3'd4;
        mem_unsigned = 1'b0;
        case (d_op)
            7'd20, 7'd41, 7'd63, 7'd30, 7'd43, 7'd70: mem_size = 3'd4;
            7'd21, 7'd64, 7'd31, 7'd71:               mem_size = 3'd2;
            7'd66: begin mem_size = 3'd2; mem_unsigned = 1'b1; end
            7'd22, 7'd65, 7'd32, 7'd72:               mem_size = 3'd1;
            7'd67: begin mem_size = 3'd1; mem_unsigned = 1'b1; end
            default: ;
        endcase
    end

    wire signed [40:0] eff_addr = (is_abs_mem ? 41'sd0 : $signed(rf_rd_a)) + $signed(d_imm);
    wire        [31:0] store_src = (d_op == 7'd43) ? rf_rd_a[31:0] : rf_rd_b[31:0];
    wire               mem_oob   = (eff_addr < 41'sd0) ||
                                   ((eff_addr + $signed({38'd0, mem_size})) > $signed({17'd0, dmem_limit}));

    // Latched state for multi-cycle memory, WB2 and D-format rs3
    reg        [23:0] m_addr;
    reg         [1:0] m_off;
    reg         [2:0] m_size;
    reg               m_unsigned;
    reg               m_is_store;
    reg               m_split;
    reg        [31:0] m_wval;
    reg        [31:0] m_rdata0;
    reg signed [39:0] wb2_val;

    // Extract load value from rdata0 (in S_MEM1 when !m_split) or {dmem_rdata, m_rdata0} (in S_MEM2)
    function automatic signed [39:0] format_load(
        input [31:0] w0,
        input [31:0] w1,
        input  [1:0] off,
        input  [2:0] sz,
        input        unsig
    );
        reg [31:0] raw32;
        reg [15:0] raw16;
        reg  [7:0] raw8;
        begin
            case (off)
                2'd0: raw32 = w0;
                2'd1: raw32 = {w1[7:0],  w0[31:8]};
                2'd2: raw32 = {w1[15:0], w0[31:16]};
                2'd3: raw32 = {w1[23:0], w0[31:24]};
            endcase
            raw16 = raw32[15:0];
            raw8  = raw32[7:0];
            if (sz == 3'd1) begin
                format_load = unsig ? {32'd0, raw8} : {{32{raw8[7]}}, raw8};
            end else if (sz == 3'd2) begin
                format_load = unsig ? {24'd0, raw16} : {{24{raw16[15]}}, raw16};
            end else begin
                format_load = {{8{raw32[31]}}, raw32};
            end
        end
    endfunction

    // First-word store byte-enable and shifted data
    function automatic [35:0] store_word0(
        input [31:0] val,
        input  [1:0] off,
        input  [2:0] sz
    );
        reg  [3:0] strb;
        reg [31:0] dat;
        begin
            if (sz == 3'd1) begin
                strb = 4'b0001 << off;
                dat  = {4{val[7:0]}};
            end else if (sz == 3'd2) begin
                case (off)
                    2'd0: begin strb = 4'b0011; dat = {16'd0, val[15:0]}; end
                    2'd1: begin strb = 4'b0110; dat = {8'd0, val[15:0], 8'd0}; end
                    2'd2: begin strb = 4'b1100; dat = {val[15:0], 16'd0}; end
                    2'd3: begin strb = 4'b1000; dat = {val[7:0], 24'd0}; end
                endcase
            end else begin
                case (off)
                    2'd0: begin strb = 4'b1111; dat = val; end
                    2'd1: begin strb = 4'b1110; dat = {val[23:0], 8'd0}; end
                    2'd2: begin strb = 4'b1100; dat = {val[15:0], 16'd0}; end
                    2'd3: begin strb = 4'b1000; dat = {val[7:0], 24'd0}; end
                endcase
            end
            store_word0 = {strb, dat};
        end
    endfunction

    // Second-word store byte-enable and shifted data (when crossing 32-bit boundary)
    function automatic [35:0] store_word1(
        input [31:0] val,
        input  [1:0] off,
        input  [2:0] sz
    );
        reg  [3:0] strb;
        reg [31:0] dat;
        begin
            if (sz == 3'd2) begin
                // only off == 3 splits for 16-bit
                strb = 4'b0001;
                dat  = {24'd0, val[15:8]};
            end else begin
                case (off)
                    2'd1: begin strb = 4'b0001; dat = {24'd0, val[31:24]}; end
                    2'd2: begin strb = 4'b0011; dat = {16'd0, val[31:16]}; end
                    default: begin strb = 4'b0111; dat = {8'd0, val[31:8]}; end
                endcase
            end
            store_word1 = {strb, dat};
        end
    endfunction

    wire [35:0] sw0_pack = store_word0(store_src, eff_addr[1:0], mem_size);
    wire [35:0] sw1_pack = store_word1(m_wval, m_off, m_size);

    always @(posedge clk) begin
        if (rst) begin
            stage         <= S_IDLE;
            pc            <= 40'sd0;
            running       <= 1'b0;
            halted        <= 1'b0;
            trapped       <= 1'b0;
            trap_cause    <= 3'd0;
            cycles_out    <= 64'd0;
            exit_code_out <= 40'sd0;
            imem_req      <= 1'b0;
            dmem_req      <= 1'b0;
            dmem_we       <= 1'b0;
            dmem_addr     <= 24'd0;
            dmem_wstrb    <= 4'd0;
            dmem_wdata    <= 32'd0;
            rf_we         <= 1'b0;
            rf_wa         <= 10'd0;
            rf_wd         <= 40'sd0;
            trit_start    <= 1'b0;
            ecall_req     <= 1'b0;
        end else begin
            rf_we      <= 1'b0;
            trit_start <= 1'b0;

            if (start_pulse) begin
                pc            <= start_pc;
                running       <= 1'b1;
                halted        <= 1'b0;
                trapped       <= 1'b0;
                trap_cause    <= 3'd0;
                cycles_out    <= 64'd0;
                exit_code_out <= 40'sd0;
                imem_req      <= 1'b0;
                dmem_req      <= 1'b0;
                ecall_req     <= 1'b0;
                stage         <= S_FETCH;
            end else if (running && !halted && !trapped) begin
                case (stage)
                    // ---------------------------------------------------------
                    S_FETCH: begin
                        if (pc < 40'sd0 || pc >= $signed({8'd0, instr_count})) begin
                            imem_req <= 1'b0;
                            halted   <= 1'b1;
                            running  <= 1'b0;
                            stage    <= S_IDLE;
                        end else if (!imem_req) begin
                            imem_req <= 1'b1;
                        end else if (imem_ready) begin
                            imem_req <= 1'b0;
                            instr    <= imem_rdata;
                            stage    <= S_DEC;
                        end
                    end

                    // ---------------------------------------------------------
                    S_DEC: begin
                        if (!dec_valid) begin
                            trapped    <= 1'b1;
                            trap_cause <= 3'd0; // InvalidInstruction
                            running    <= 1'b0;
                            stage      <= S_IDLE;
                        end else if (bad_reg) begin
                            trapped    <= 1'b1;
                            trap_cause <= 3'd1; // InvalidRegister
                            running    <= 1'b0;
                            stage      <= S_IDLE;
                        end else begin
                            d_op   <= dec_op;
                            d_rs1  <= dec_rs1;
                            d_rs2  <= dec_rs2;
                            d_rs3  <= dec_rs3;
                            d_rd1  <= dec_rd1;
                            d_rd2  <= dec_rd2;
                            d_imm  <= dec_imm48[39:0];
                            d_imm2 <= dec_imm2_48[39:0];
                            d_fill <= dec_fill;
                            stage  <= S_EXEC;
                        end
                    end

                    // ---------------------------------------------------------
                    S_EXEC: begin
                        // Traps (note: cpu.rs increments cycles before executing op)
                        if (d_op == 7'd37) begin
                            cycles_out <= cycles_out + 64'd1;
                            trapped    <= 1'b1;
                            trap_cause <= 3'd5; // WaitForInterrupt
                            running    <= 1'b0;
                            stage      <= S_IDLE;
                        end else if (d_op == 7'd38) begin
                            cycles_out <= cycles_out + 64'd1;
                            trapped    <= 1'b1;
                            trap_cause <= 3'd4; // TrapReturn
                            running    <= 1'b0;
                            stage      <= S_IDLE;
                        end else if (d_op == 7'd39) begin
                            cycles_out <= cycles_out + 64'd1;
                            trapped    <= 1'b1;
                            trap_cause <= 3'd3; // Breakpoint
                            running    <= 1'b0;
                            stage      <= S_IDLE;
                        end else if (d_op == 7'd40) begin
                            // ECALL.T: advance pc and cycles, wait for host/SoC service
                            pc         <= pc + 40'sd1;
                            cycles_out <= cycles_out + 64'd1;
                            ecall_req  <= 1'b1;
                            stage      <= S_ECALL;
                        end else if (is_load || is_store) begin
                            if (mem_oob) begin
                                cycles_out <= cycles_out + 64'd1;
                                trapped    <= 1'b1;
                                trap_cause <= 3'd2; // MemoryFault
                                running    <= 1'b0;
                                stage      <= S_IDLE;
                            end else begin
                                m_addr     <= {eff_addr[23:2], 2'b00};
                                m_off      <= eff_addr[1:0];
                                m_size     <= mem_size;
                                m_unsigned <= mem_unsigned;
                                m_is_store <= is_store;
                                m_split    <= (mem_size == 3'd4 && eff_addr[1:0] != 2'd0) ||
                                              (mem_size == 3'd2 && eff_addr[1:0] == 2'd3);
                                m_wval     <= store_src;

                                dmem_req   <= 1'b1;
                                dmem_we    <= is_store;
                                dmem_addr  <= {eff_addr[23:2], 2'b00};
                                dmem_wstrb <= is_store ? sw0_pack[35:32] : 4'b0000;
                                dmem_wdata <= sw0_pack[31:0];
                                stage      <= S_MEM1;
                            end
                        end else if (d_op == 7'd2  || d_op == 7'd3  || d_op == 7'd5  || d_op == 7'd6  ||
                                     d_op == 7'd7  || d_op == 7'd14 || d_op == 7'd15 || d_op == 7'd17 ||
                                     d_op == 7'd18 || d_op == 7'd19) begin
                            // 2-operand tritwise / shift / rotate
                            trit_a <= rf_rd_a;
                            trit_b <= (d_op == 7'd15 || d_op == 7'd17 || d_op == 7'd18 || d_op == 7'd19)
                                    ? d_imm : rf_rd_b;
                            case (d_op)
                                7'd5, 7'd17: trit_mode <= 4'd0; // OR.T / ORI.T
                                7'd6, 7'd18: trit_mode <= 4'd1; // XOR.T / XORI.T
                                7'd7, 7'd19: trit_mode <= 4'd2; // AND.T / ANDI.T
                                7'd2:        trit_mode <= 4'd5; // SH.T
                                7'd3:        trit_mode <= 4'd6; // ROTR.T
                                7'd14:       trit_mode <= 4'd7; // SHI.T
                                default:     trit_mode <= 4'd8; // ROT.T
                            endcase
                            trit_start <= 1'b1;
                            stage      <= S_TRIT_WAIT;
                        end else if (d_op == 7'd33 || d_op == 7'd34) begin
                            // MAJV.T / MINV.T: rf_ra_a = d_rs3 is issued now, arrives next cycle
                            trit_a    <= rf_rd_a;
                            trit_b    <= rf_rd_b;
                            trit_mode <= (d_op == 7'd33) ? 4'd3 : 4'd4;
                            stage     <= S_TRIT_RS3;
                        end else if (d_op == 7'd10) begin
                            // MV2.T: write rd1 = rs1, then rd2 = rs2 in S_WB2
                            rf_we   <= 1'b1;
                            rf_wa   <= d_rd1[9:0];
                            rf_wd   <= rf_rd_a;
                            wb2_val <= rf_rd_b;
                            stage   <= S_WB2;
                        end else if (d_op == 7'd35) begin
                            // LI2.T: write rd1 = imm, then rd2 = imm2 in S_WB2
                            rf_we   <= 1'b1;
                            rf_wa   <= d_rd1[9:0];
                            rf_wd   <= d_imm;
                            wb2_val <= d_imm2;
                            stage   <= S_WB2;
                        end else begin
                            // Single-cycle ALU, Jumps, Branches and FENCE.T
                            cycles_out <= cycles_out + 64'd1;
                            stage      <= S_FETCH;

                            // Register writeback if instruction writes rd1
                            if (d_op == 7'd0  || d_op == 7'd1  || d_op == 7'd4  || d_op == 7'd8  ||
                                d_op == 7'd9  || d_op == 7'd11 || d_op == 7'd12 || d_op == 7'd13 ||
                                d_op == 7'd16 || d_op == 7'd23 || d_op == 7'd42 || d_op == 7'd44 ||
                                d_op == 7'd45 || (d_op >= 7'd46 && d_op <= 7'd62)) begin
                                rf_we <= 1'b1;
                                rf_wa <= d_rd1[9:0];
                                rf_wd <= alu_res;
                            end

                            // Next PC selection
                            case (d_op)
                                7'd23: pc <= rf_rd_a + d_imm; // jalr.t
                                7'd44: pc <= pc + d_imm;      // jal.t
                                7'd24: begin // bcgs.t
                                    if (!alu_eq && !alu_lt)      pc <= pc + d_imm;
                                    else if (alu_lt)             pc <= pc + d_imm2;
                                    else                         pc <= pc + 40'sd1;
                                end
                                7'd25: begin // bceg.t
                                    if (alu_eq)                  pc <= pc + d_imm;
                                    else if (!alu_lt)            pc <= pc + d_imm2;
                                    else                         pc <= pc + 40'sd1;
                                end
                                7'd26: pc <= alu_eq    ? (pc + d_imm) : (pc + 40'sd1); // beq.t
                                7'd27: pc <= (!alu_eq) ? (pc + d_imm) : (pc + 40'sd1); // bne.t
                                7'd28: pc <= alu_lt    ? (pc + d_imm) : (pc + 40'sd1); // blt.t
                                7'd29: pc <= (!alu_lt) ? (pc + d_imm) : (pc + 40'sd1); // bge.t
                                7'd68: pc <= alu_ltu32    ? (pc + d_imm) : (pc + 40'sd1); // bltu
                                7'd69: pc <= (!alu_ltu32) ? (pc + d_imm) : (pc + 40'sd1); // bgeu
                                default: pc <= pc + 40'sd1;
                            endcase
                        end
                    end

                    // ---------------------------------------------------------
                    S_MEM1: begin
                        if (dmem_ready) begin
                            if (!m_split) begin
                                dmem_req   <= 1'b0;
                                dmem_we    <= 1'b0;
                                if (!m_is_store) begin
                                    rf_we <= 1'b1;
                                    rf_wa <= d_rd1[9:0];
                                    rf_wd <= format_load(dmem_rdata, 32'd0, m_off, m_size, m_unsigned);
                                end
                                pc         <= pc + 40'sd1;
                                cycles_out <= cycles_out + 64'd1;
                                stage      <= S_FETCH;
                            end else begin
                                m_rdata0 <= dmem_rdata;
                                dmem_req <= 1'b0;
                                dmem_we  <= 1'b0;
                                stage    <= S_MEM2;
                            end
                        end
                    end

                    // ---------------------------------------------------------
                    S_MEM2: begin
                        if (!dmem_req) begin
                            dmem_req   <= 1'b1;
                            dmem_we    <= m_is_store;
                            dmem_addr  <= m_addr + 24'd4;
                            dmem_wstrb <= m_is_store ? sw1_pack[35:32] : 4'b0000;
                            dmem_wdata <= sw1_pack[31:0];
                        end else if (dmem_ready) begin
                            dmem_req <= 1'b0;
                            dmem_we  <= 1'b0;
                            if (!m_is_store) begin
                                rf_we <= 1'b1;
                                rf_wa <= d_rd1[9:0];
                                rf_wd <= format_load(m_rdata0, dmem_rdata, m_off, m_size, m_unsigned);
                            end
                            pc         <= pc + 40'sd1;
                            cycles_out <= cycles_out + 64'd1;
                            stage      <= S_FETCH;
                        end
                    end

                    // ---------------------------------------------------------
                    S_TRIT_RS3: begin
                        trit_c     <= rf_rd_a;
                        trit_start <= 1'b1;
                        stage      <= S_TRIT_WAIT;
                    end

                    // ---------------------------------------------------------
                    S_TRIT_WAIT: begin
                        if (trit_done) begin
                            rf_we      <= 1'b1;
                            rf_wa      <= d_rd1[9:0];
                            rf_wd      <= trit_res;
                            pc         <= pc + 40'sd1;
                            cycles_out <= cycles_out + 64'd1;
                            stage      <= S_FETCH;
                        end
                    end

                    // ---------------------------------------------------------
                    S_WB2: begin
                        rf_we      <= 1'b1;
                        rf_wa      <= d_rd2[9:0];
                        rf_wd      <= wb2_val;
                        pc         <= pc + 40'sd1;
                        cycles_out <= cycles_out + 64'd1;
                        stage      <= S_FETCH;
                    end

                    // ---------------------------------------------------------
                    S_ECALL: begin
                        if (ecall_done) begin
                            ecall_req <= 1'b0;
                            if (ecall_write_a0) begin
                                rf_we <= 1'b1;
                                rf_wa <= 10'd10;
                                rf_wd <= ecall_ret_a0;
                            end
                            if (ecall_halt) begin
                                exit_code_out <= ecall_exit_code;
                                halted        <= 1'b1;
                                running       <= 1'b0;
                                stage         <= S_IDLE;
                            end else begin
                                stage <= S_FETCH;
                            end
                        end
                    end

                    default: stage <= S_IDLE;
                endcase
            end
        end
    end
endmodule

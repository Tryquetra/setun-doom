// H16: Physical FPGA synthesis and switching-energy comparison for N balanced
// zero-sum hexagonal unit cells (q + r + s = 0) across five binary codings:
//   0 hg_canon6b   : Canonical 2-bit/trit (6 FFs/cell)
//   1 hg_offset6b  : Offset 2-bit/trit (6 FFs/cell)
//   2 hg_onehot9b  : One-Hot 3-bit/trit (9 FFs/cell)
//   3 hg_onehot7b  : 7-state One-Hot (7 FFs/cell)
//   4 hg_hexgray3b : Hexagonal Gray Q3 (3 FFs/cell)
//
// Command `cmd` (per cell or broadcast):
//   2'b00 : hold
//   2'b01 : step CCW (+60 deg)
//   2'b10 : step CW  (-60 deg)
//   2'b11 : jump/reset to Center (0,0,0)

module hex_cell_canon6b (
    input  wire [5:0] cur,
    input  wire [1:0] cmd,
    output reg  [5:0] nxt
);
    localparam [5:0] C  = 6'b00_00_00;
    localparam [5:0] V0 = 6'b10_00_01;
    localparam [5:0] V1 = 6'b10_01_00;
    localparam [5:0] V2 = 6'b00_01_10;
    localparam [5:0] V3 = 6'b01_00_10;
    localparam [5:0] V4 = 6'b01_10_00;
    localparam [5:0] V5 = 6'b00_10_01;

    always @(*) begin
        case (cmd)
            2'b00: nxt = cur;
            2'b11: nxt = C;
            2'b01: begin // CCW
                case (cur)
                    V0: nxt = V1;
                    V1: nxt = V2;
                    V2: nxt = V3;
                    V3: nxt = V4;
                    V4: nxt = V5;
                    V5: nxt = V0;
                    default: nxt = V0;
                endcase
            end
            default: begin // CW
                case (cur)
                    V0: nxt = V5;
                    V5: nxt = V4;
                    V4: nxt = V3;
                    V3: nxt = V2;
                    V2: nxt = V1;
                    V1: nxt = V0;
                    default: nxt = V2;
                endcase
            end
        endcase
    end
endmodule

module hex_cell_offset6b (
    input  wire [5:0] cur,
    input  wire [1:0] cmd,
    output reg  [5:0] nxt
);
    localparam [5:0] C  = 6'b01_01_01;
    localparam [5:0] V0 = 6'b00_01_10;
    localparam [5:0] V1 = 6'b00_10_01;
    localparam [5:0] V2 = 6'b01_10_00;
    localparam [5:0] V3 = 6'b10_01_00;
    localparam [5:0] V4 = 6'b10_00_01;
    localparam [5:0] V5 = 6'b01_00_10;

    always @(*) begin
        case (cmd)
            2'b00: nxt = cur;
            2'b11: nxt = C;
            2'b01: begin
                case (cur)
                    V0: nxt = V1;
                    V1: nxt = V2;
                    V2: nxt = V3;
                    V3: nxt = V4;
                    V4: nxt = V5;
                    V5: nxt = V0;
                    default: nxt = V0;
                endcase
            end
            default: begin
                case (cur)
                    V0: nxt = V5;
                    V5: nxt = V4;
                    V4: nxt = V3;
                    V3: nxt = V2;
                    V2: nxt = V1;
                    V1: nxt = V0;
                    default: nxt = V2;
                endcase
            end
        endcase
    end
endmodule

module hex_cell_onehot9b (
    input  wire [8:0] cur,
    input  wire [1:0] cmd,
    output reg  [8:0] nxt
);
    localparam [8:0] C  = 9'b001_001_001;
    localparam [8:0] V0 = 9'b100_001_010;
    localparam [8:0] V1 = 9'b100_010_001;
    localparam [8:0] V2 = 9'b001_010_100;
    localparam [8:0] V3 = 9'b010_001_100;
    localparam [8:0] V4 = 9'b010_100_001;
    localparam [8:0] V5 = 9'b001_100_010;

    always @(*) begin
        case (cmd)
            2'b00: nxt = cur;
            2'b11: nxt = C;
            2'b01: begin
                case (cur)
                    V0: nxt = V1;
                    V1: nxt = V2;
                    V2: nxt = V3;
                    V3: nxt = V4;
                    V4: nxt = V5;
                    V5: nxt = V0;
                    default: nxt = V0;
                endcase
            end
            default: begin
                case (cur)
                    V0: nxt = V5;
                    V5: nxt = V4;
                    V4: nxt = V3;
                    V3: nxt = V2;
                    V2: nxt = V1;
                    V1: nxt = V0;
                    default: nxt = V2;
                endcase
            end
        endcase
    end
endmodule

module hex_cell_onehot7b (
    input  wire [6:0] cur,
    input  wire [1:0] cmd,
    output reg  [6:0] nxt
);
    localparam [6:0] C  = 7'b0000001;
    localparam [6:0] V0 = 7'b0000010;
    localparam [6:0] V1 = 7'b0000100;
    localparam [6:0] V2 = 7'b0001000;
    localparam [6:0] V3 = 7'b0010000;
    localparam [6:0] V4 = 7'b0100000;
    localparam [6:0] V5 = 7'b1000000;

    always @(*) begin
        case (cmd)
            2'b00: nxt = cur;
            2'b11: nxt = C;
            2'b01: begin
                case (cur)
                    V0: nxt = V1;
                    V1: nxt = V2;
                    V2: nxt = V3;
                    V3: nxt = V4;
                    V4: nxt = V5;
                    V5: nxt = V0;
                    default: nxt = V0;
                endcase
            end
            default: begin
                case (cur)
                    V0: nxt = V5;
                    V5: nxt = V4;
                    V4: nxt = V3;
                    V3: nxt = V2;
                    V2: nxt = V1;
                    V1: nxt = V0;
                    default: nxt = V2;
                endcase
            end
        endcase
    end
endmodule

module hex_cell_hexgray3b (
    input  wire [2:0] cur,
    input  wire [1:0] cmd,
    output reg  [2:0] nxt
);
    localparam [2:0] C  = 3'b000;
    localparam [2:0] V0 = 3'b001;
    localparam [2:0] V1 = 3'b011;
    localparam [2:0] V2 = 3'b010;
    localparam [2:0] V3 = 3'b110;
    localparam [2:0] V4 = 3'b111;
    localparam [2:0] V5 = 3'b101;
    localparam [2:0] S  = 3'b100;

    always @(*) begin
        case (cmd)
            2'b00: nxt = cur;
            2'b11: nxt = C;
            2'b01: begin // CCW (d_H = 1 everywhere on Q3)
                case (cur)
                    V0: nxt = V1;
                    V1: nxt = V2;
                    V2: nxt = V3;
                    V3: nxt = V4;
                    V4: nxt = V5;
                    V5: nxt = V0;
                    C:  nxt = V0;
                    default: nxt = V5; // 100 -> 101
                endcase
            end
            default: begin // CW (d_H = 1 everywhere on Q3)
                case (cur)
                    V0: nxt = V5;
                    V5: nxt = V4;
                    V4: nxt = V3;
                    V3: nxt = V2;
                    V2: nxt = V1;
                    V1: nxt = V0;
                    C:  nxt = V2;
                    default: nxt = V3; // 100 -> 110
                endcase
            end
        endcase
    end
endmodule

module hg_canon6b_n8 (
    input  wire        clk,
    input  wire        rst,
    input  wire [15:0] cmd,
    output reg  [47:0] state
);
    wire [47:0] nxt;
    genvar i;
    generate
        for (i = 0; i < 8; i = i + 1) begin : lane
            hex_cell_canon6b u (.cur(state[6*i+5 : 6*i]), .cmd(cmd[2*i+1 : 2*i]), .nxt(nxt[6*i+5 : 6*i]));
        end
    endgenerate
    always @(posedge clk) begin
        if (rst) state <= 48'b0;
        else     state <= nxt;
    end
endmodule

module hg_offset6b_n8 (
    input  wire        clk,
    input  wire        rst,
    input  wire [15:0] cmd,
    output reg  [47:0] state
);
    wire [47:0] nxt;
    genvar i;
    generate
        for (i = 0; i < 8; i = i + 1) begin : lane
            hex_cell_offset6b u (.cur(state[6*i+5 : 6*i]), .cmd(cmd[2*i+1 : 2*i]), .nxt(nxt[6*i+5 : 6*i]));
        end
    endgenerate
    always @(posedge clk) begin
        if (rst) state <= {8{6'b01_01_01}};
        else     state <= nxt;
    end
endmodule

module hg_onehot9b_n8 (
    input  wire        clk,
    input  wire        rst,
    input  wire [15:0] cmd,
    output reg  [71:0] state
);
    wire [71:0] nxt;
    genvar i;
    generate
        for (i = 0; i < 8; i = i + 1) begin : lane
            hex_cell_onehot9b u (.cur(state[9*i+8 : 9*i]), .cmd(cmd[2*i+1 : 2*i]), .nxt(nxt[9*i+8 : 9*i]));
        end
    endgenerate
    always @(posedge clk) begin
        if (rst) state <= {8{9'b001_001_001}};
        else     state <= nxt;
    end
endmodule

module hg_onehot7b_n8 (
    input  wire        clk,
    input  wire        rst,
    input  wire [15:0] cmd,
    output reg  [55:0] state
);
    wire [55:0] nxt;
    genvar i;
    generate
        for (i = 0; i < 8; i = i + 1) begin : lane
            hex_cell_onehot7b u (.cur(state[7*i+6 : 7*i]), .cmd(cmd[2*i+1 : 2*i]), .nxt(nxt[7*i+6 : 7*i]));
        end
    endgenerate
    always @(posedge clk) begin
        if (rst) state <= {8{7'b0000001}};
        else     state <= nxt;
    end
endmodule

module hg_hexgray3b_n8 (
    input  wire        clk,
    input  wire        rst,
    input  wire [15:0] cmd,
    output reg  [23:0] state
);
    wire [23:0] nxt;
    genvar i;
    generate
        for (i = 0; i < 8; i = i + 1) begin : lane
            hex_cell_hexgray3b u (.cur(state[3*i+2 : 3*i]), .cmd(cmd[2*i+1 : 2*i]), .nxt(nxt[3*i+2 : 3*i]));
        end
    endgenerate
    always @(posedge clk) begin
        if (rst) state <= 24'b0;
        else     state <= nxt;
    end
endmodule

// Tang Nano 20K PNR timing-measurement wrappers (constant 4-pin IO footprint)
module pnr_hg_canon6b_n8 (input wire I_clk, input wire [1:0] I_btn, output reg [0:0] O_led_n);
    reg [15:0] cmd_sr;
    wire [47:0] state;
    always @(posedge I_clk) cmd_sr <= {cmd_sr[14:0], I_btn[1]};
    hg_canon6b_n8 core (.clk(I_clk), .rst(I_btn[0]), .cmd(cmd_sr), .state(state));
    always @(posedge I_clk) O_led_n[0] <= ^state;
endmodule

module pnr_hg_offset6b_n8 (input wire I_clk, input wire [1:0] I_btn, output reg [0:0] O_led_n);
    reg [15:0] cmd_sr;
    wire [47:0] state;
    always @(posedge I_clk) cmd_sr <= {cmd_sr[14:0], I_btn[1]};
    hg_offset6b_n8 core (.clk(I_clk), .rst(I_btn[0]), .cmd(cmd_sr), .state(state));
    always @(posedge I_clk) O_led_n[0] <= ^state;
endmodule

module pnr_hg_onehot9b_n8 (input wire I_clk, input wire [1:0] I_btn, output reg [0:0] O_led_n);
    reg [15:0] cmd_sr;
    wire [71:0] state;
    always @(posedge I_clk) cmd_sr <= {cmd_sr[14:0], I_btn[1]};
    hg_onehot9b_n8 core (.clk(I_clk), .rst(I_btn[0]), .cmd(cmd_sr), .state(state));
    always @(posedge I_clk) O_led_n[0] <= ^state;
endmodule

module pnr_hg_onehot7b_n8 (input wire I_clk, input wire [1:0] I_btn, output reg [0:0] O_led_n);
    reg [15:0] cmd_sr;
    wire [55:0] state;
    always @(posedge I_clk) cmd_sr <= {cmd_sr[14:0], I_btn[1]};
    hg_onehot7b_n8 core (.clk(I_clk), .rst(I_btn[0]), .cmd(cmd_sr), .state(state));
    always @(posedge I_clk) O_led_n[0] <= ^state;
endmodule

module pnr_hg_hexgray3b_n8 (input wire I_clk, input wire [1:0] I_btn, output reg [0:0] O_led_n);
    reg [15:0] cmd_sr;
    wire [23:0] state;
    always @(posedge I_clk) cmd_sr <= {cmd_sr[14:0], I_btn[1]};
    hg_hexgray3b_n8 core (.clk(I_clk), .rst(I_btn[0]), .cmd(cmd_sr), .state(state));
    always @(posedge I_clk) O_led_n[0] <= ^state;
endmodule

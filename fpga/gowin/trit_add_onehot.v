// H14-B: one-hot coded trit adder.
//
// Trit k occupies bits [3k+2:3k]: 001 = 0, 010 = +1, 100 = -1, 111 error.
// Per stage, count plus flags and minus flags across the three one-hot inputs;
// total = pos - neg in -3..3 selects the balanced digit and next carry.
module trit_add_onehot #(
    parameter N = 24
) (
    input  wire [3*N-1:0] a,
    input  wire [3*N-1:0] b,
    input  wire [2:0] cin,
    output wire [3*N-1:0] y,
    output wire [2:0] cout
);
    wire [2:0] c_oh [N:0];
    assign c_oh[0] = cin;
    genvar i;
    generate
        for (i = 0; i < N; i = i + 1) begin : stages
            wire [1:0] pos =
                a[3*i+1] + b[3*i+1] + c_oh[i][1];
            wire [1:0] neg =
                a[3*i+2] + b[3*i+2] + c_oh[i][2];
            wire signed [3:0] total =
                $signed({2'b00, pos}) - $signed({2'b00, neg});
            reg [2:0] d, co;
            always @* begin
                case (total)
                    -4'sd3: begin d = 3'b001; co = 3'b100; end
                    -4'sd2: begin d = 3'b010; co = 3'b100; end
                    -4'sd1: begin d = 3'b100; co = 3'b001; end
                     4'sd0: begin d = 3'b001; co = 3'b001; end
                     4'sd1: begin d = 3'b010; co = 3'b001; end
                     4'sd2: begin d = 3'b100; co = 3'b010; end
                     4'sd3: begin d = 3'b001; co = 3'b010; end
                    default: begin d = 3'b001; co = 3'b001; end
                endcase
            end
            assign y[3*i+2] = d[2];
            assign y[3*i+1] = d[1];
            assign y[3*i]   = d[0];
            assign c_oh[i+1] = co;
        end
    endgenerate
    assign cout = c_oh[N];
endmodule


module tb ();
  wire A;
  reg  B;
  wire Y;

  reg  A_reg;
  assign A = A_reg;

  simple dut (
      A,
      B,
      Y
  );


  initial begin
    A_reg = 0;
    B = 0;
    #10 assert (Y == 1);

    A_reg = 0;
    B = 1;
    #10 assert (Y == 1);

    A_reg = 1;
    B = 0;
    #10 assert (Y == 1);

    A_reg = 1;
    B = 1;
    #10 assert (Y == 0);

    A_reg = 'Z;
    B = 0;
    #10 assert (Y == 1);

    A_reg = 'Z;
    B = 1;
    #10 assert (Y == 0);

    $display("All tests passed");
  end

endmodule

module \74LVC1G00 (
    input  A,   // pin 1
    input  B,   // pin 2
    output Out  // pin 4

);
  assign Out = ~(A & B);
endmodule

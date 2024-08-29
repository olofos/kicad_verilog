module simple (
    input  J1_A,
    input  J1_B,
    output J1_Y
);

  wire \/A ;
  wire \/B ;
  wire \/Y ;
  wire GND;
  wire VCC;

  assign VCC = 1;
  assign GND = 0;

  tran (J1_A, \/A );
  tran (J1_B, \/B );
  tran (J1_Y, \/Y );

  pullup R1 (\/A );
  \74LVC1G00 U1 (\/A , \/B , \/Y );
endmodule

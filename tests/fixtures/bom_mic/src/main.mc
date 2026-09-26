# ---------------------------------------------------------------------------------------------
#  Copyright (c) MCODE. All rights reserved.
# ---------------------------------------------------------------------------------------------
# U306 bucket ④ fixture: the abstract-slot pattern (power.mc's LDO shape)
# with the electret-microphone family — abstract slot plus the modern SIP2
# variant (mcpub's MICROPHONE.SIP2_1_25MM_WA), bound by bom.mc.

abstract component MICROPHONE.ELECTRET
{
    pins = [
        1 = P
        2 = N
        [3,4] = GND
    ]
}

component MICROPHONE.SIP2_1_25MM_WA : MICROPHONE.ELECTRET
{
    partno = "SIP2-1.25MM-WA"
    package = PKG.MIC_SIP2
}

module main(psnk dc{VDD_3V3, GND}::DC(3.3V))
{
    MICROPHONE.ELECTRET mic

    mic{1,2} -> C1::CAP(470pF)' -> dc.GND
    mic{3,4} -> dc.GND
}

# ---------------------------------------------------------------------------------------------
#  Copyright (c) MCODE. All rights reserved.
# ---------------------------------------------------------------------------------------------
use ./us513.mc
use ./power.mc

component MICROPHONE.SIP2
{
    partno = "SIP2-1.25MM-WA"
    package = PKG.MIC_SIP2  

    pins = [
        1 = P
        2 = N
        [3,4] = GND
    ]
}

component MICROPHONE.WM7121P
{
    partno = "WM7121P"
    package = PKG.MIC_WM7121P
    voltage = "3.3V"            // VCC fed from the VDD_3V3 rail

    pins = [
        1 = P
        [2,3] = GND             // GND
        4 = VCC
    ]  
}

module MIC_SIP(dc{VDD_3V3, GND}::DC(3.3V))
{
    out MIC{P, N}::ADC.DIFF(Transmitter)

    MICROPHONE.SIP2 mic

    // dc -> [RES(240Ω, ±1%, "R0402"), _] + CAP(4.7uF, ±20%, CAP.Y5V, 6.3V)'
    // -> VMIC::DC()
    // -> [RES(1kΩ, ±1%, "R0402"), RES(1kΩ, ±1%, "R0402")]  
    // -> CAP(1uF, ±10%, CAP.X5R, 10V)'                                            
    // -> [RES(1kΩ, ±1%, "R0402"), RES(1kΩ, ±1%, "R0402")] -> MIC{P,N}

    mic{1,2} -> C1::CAP(470pF)' -> MIC{P,N}
    mic{1,2} -> [dio[1:2]::DIO.ESD(5V)] -> [dc.GND, dc.GND]

    MICROPHONE.WM7121P wm7121(NC)
    CAP(100nF, ±20%, CAP.X5R, 25V, NC).Cap([[dc.VDD_3V3 -> wm7121.VCC], dc.GND])
    wm7121{2,3} - [dc.GND, dc.GND]
    MIC.N - RES(0R, NC) - dc.GND
}

component LPA4871
{   
    partno = "LPA4871"
    package = PKG.SOP8              // SOP8 footprint

    pins = [
        1 = EN
        2 = BYPASS                  // GND
        3 = IN.P
        4 = IN.N
        5 = VO1
        8 = VO2
        [6,7] = [VDD, GND]::DC()
    ]
}

component SPEAKER.PHB2AWB
{
    partno = "PHB-2AWB"
    package = PKG.SPEAKER_PHB2AWB

    pins = [
        1 = P
        2 = N
        3 = GND                     // GND
        4 = GND                     // GND
    ]
}

module SPEAKER_M(USB_VBUS_1{VDD_3V, GND}::DC(3.3V))
{
    in DAC_OUT , US_SPEAKER_MUTE

    LPA4871 lpa
    SPEAKER.PHB2AWB spk

    USB_VBUS_1 {VDD_3V, GND} -> C8::CAP()' -> lpa{VDD, GND}
    USB_VBUS_1.VDD_3V -> RES(10kΩ) -> (lpa.EN, US_SPEAKER_MUTE)
    lpa.BYPASS + lpa.IN.P -> CAP(1uF, ±10%, CAP.X5R, 10V) -> USB_VBUS_1.GND

    DAC_OUT -> RES(15kΩ, ±1%) -> (lpa.IN.N, RES(30kΩ, ±1%) -> lpa.VO1 + spk.N)
    lpa.VO2 -> spk.P

    spk.P -> TP1::TP()
    spk.N -> TP2::TP()
    spk.P -> DIO.ESD(5V) -> USB_VBUS_1.GND
    spk.N -> DIO.ESD(5V) -> USB_VBUS_1.GND
    spk.3 + spk.4 -> USB_VBUS_1.GND
}

# ---------------------------------------------------------------------------------------------
#  Copyright (c) MCODE. All rights reserved.
# ---------------------------------------------------------------------------------------------

component USB.MINI_B
{
    partno = "HUM011D-5-S"
    package = PKG.USB_Mini
    voltage = "5V"                  // VBUS is a 5V power rail

    pins = [
        1 = VBUS
        5 = GND
        2 = D\+
        3 = D\-
        4 = ID
        [6,7] = GND
        8 = SHIELD3
        9 = SHIELD4
    ]
}

module POWER_USB()
{
    io vin{POWER_SYS, GND}::DC(5V)

    USB.MINI_B usbsock
    TP1::TP()
    TP3::TP()

    ((usbsock.VBUS -> USB_VBUS) + TP1) -> RES(0R) -> vin.POWER_SYS
    (usbsock.5 + usbsock.6 + usbsock.7 + usbsock.SHIELD3 + usbsock.SHIELD4) + TP3 -> vin.GND
}

component LDO.SGM2019_33YN5G_TR
{
    partno = "SGM2019-3.3YN5G/TR"
    package = PKG.SOT_23_5

    pins = [
        in [1,2] = VIN{Vin, GND}::DC(2.5V~5.5V)
        in 3 = CE
        in 4 = FB
        out [5, 2] = VOUT{Vout, GND}::DC(3.3V)
    ]

    func enable(){
        VIN.Vin -> CE
    }
}

module POWER_LDO()
{
    in vin::DC(5V)
    out vout::DC(3.3V)

    LDO.SGM2019_33YN5G_TR   ldo
    ldo.enable()

    vin -> ldo.VIN =>
    CAP(10uF, ±20%, CAP.X5R, 10V).Cap(_)

    CAP(4.7uF, ±20%, CAP.Y5V, 6.3V).Cap(ldo.VOUT)
    -> vout

}

component DCDC.LP3220AB5F
{
    partno = "LP3220AB5F"
    package = PKG.SOT_23_5
    input_voltage = "2.5V~5.5V"  // Vin operating range per datasheet

    pins = [
        1 = EN
        3 = LX
        2 = GND
        4 = Vin
        5 = FB
    ]
    func enable(){
        Vin -> RES(47kΩ) -> EN
    }
}

module POWER_DCDC()
{
    in [VDD_3V3, GND]::DC(3.3V)
    out [VCC_1V2, GND]::DC(1.2V)

    DCDC.LP3220AB5F   lp322dcdc.enable()

    [VDD_3V3, GND] -> lp322dcdc{Vin, GND}
    CAP(10uF,10V).Cap(lp322dcdc{Vin, GND})
    CAP(1uF,10V).Cap(lp322dcdc{EN, GND})

    lp322dcdc.LX -> IND(2.2uH, 1.5A) -> VCC_1V2
    CAP(10uF,10V).Cap([VCC_1V2, GND])
    CAP(100nF,25V).Cap([VCC_1V2, GND])

    VCC_1V2 - RES(137kΩ, 1%) - lp322dcdc.FB - RES(150kΩ, 1%) - GND
    CAP(15pF).Cap([lp322dcdc.FB, GND])
}

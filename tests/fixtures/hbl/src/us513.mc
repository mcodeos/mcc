# ---------------------------------------------------------------------------------------------
#  Copyright (c) MCODE. All rights reserved.
# ---------------------------------------------------------------------------------------------
use ./power.mc


component MCU.US513_20_F
{
    partno = "US513_20_F"
    package = PKG.QFN20

    pins = [
        io [1,2] =  I2C0::I2C(Master)
                    | GPIO[3, 4]::GPIO(2, Controller)

        in [3,4] =  XTAL::XTAL(32kHz)

        ps [5,21] = [VDD, GND]::DC(3.3V)

        io [6,7] =  UART0::UART.TTL(DCE)
                    | I2C1::I2C(Master)
                    | GPIO[5, 6]::GPIO(2, Controller)

        io [8,9] =  PDM[CLK, DATA] 
                    | PBus{CLK, DATA} 
                    | GPIO[7, 8]::GPIO(2, Controller)
        
        io [10,11] = I2C1::I2C(Master) | GPIO[9, 10]::GPIO(2, Controller),
                    ["I2C", "GPIO"], volt:1.2V, amp:100mA

        io [8:11] = SPI{SCLK, MOSI, CSN, MISO}::SPI(Master)

        io [12,13] = UART1::UART.TTL(DCE)
                    | GPIO[5, 6]::GPIO(2, Controller)

        ps [14,21] = [VDD_CORE,GND]::DC(1.2V)

        in 15 = AVDD09_CAP

        io [16, 17] = ADC::ADC.DIFF(Receiver)

        io [18,19] = JTAG::DBG.JTAG.2WIRE(TAP)
                    | GPIO[0,1]::GPIO(2, Controller)

        io 20 = GPIO[2] | EXT_CLK_IN
    ]

    func power(V3V3::DC(3.3V), V1V2::DC(1.2V))
    {
        V3V3 => CAP(1uF, ±10%, CAP.X5R, 10V).Cap(_) -> [VDD, GND]
        V1V2 => CAP(1uF, ±10%, CAP.X5R, 10V).Cap(_) -> [VDD_CORE, GND]
        CAP(1uF, ±10%, CAP.X5R, 10V).Cap([AVDD09_CAP, GND])
    }

    func i2c(address)
    {

        if address == 0x36
            VDD -> RES(100kΩ) -> GPIO[2]
        else //if address == 0x35
            GPIO[2] - RES(100kΩ) -> GND

        RES(10kΩ).Pullup([I2C0.SCL, VDD])
        RES(10kΩ).Pullup([I2C0.SDA, VDD])
    }
}

component Crystal2.DST310S
{
    partno = "DST310S"
    package = PKG.Xtal_3215
    spec = [
        frequency = 32kHz
    ]
    
    pins = [ 
        [1,2] = XTAL::XTAL()
    ]

    func setup(GND) {
        XTAL - R442::RES(1MΩ, ±1%)'
        - [
            CAP(18pF, ±5%, CAP.C0G, 50V),  
            CAP(18pF, ±5%, CAP.C0G, 50V)         
        ] 
        - [GND, GND]
    }
}

component FLASH.GD25Q32E
{
    partno = "GD25Q32ESIG"
    package = PKG.SOP8                          // SOP8 footprint
    pins = [ 
        1 = _CS
        2 = SO | IO1
        3 = _WP | IO2
        5 = SI | IO0
        6 = SCLK
        7 = _HOLD | IO3
        [8,4] = [VCC,VSS]::DC(3.3V)
        
        [1, 2, 5, 6] = SPI::SPI("Slave")
    ]
        
    func GD25Q32E([V3V3, GND]::DC(3.3V))
    {
        [V3V3, GND] => CAP(100nF, ±20%, CAP.X5R, 25V).Cap(_) -> [VCC, VSS]

        RES(10kΩ).Pullup([_CS, V3V3])
        RES(10kΩ).Pullup([_WP, V3V3])
        RES(10kΩ).Pullup([_HOLD, V3V3])
    }
}

module US513([VDD_3V3,GND]::DC(3.3V), [VCC_1V2,GND]::DC(1.2V))
{
    io MIC{P,N}, I2C0, SPI, UART0, UART1, port1{A,B,C,D}
    out DAC_OUT, SPK_MUTE

    MCU.US513_20_F UC

    UC.power([VDD_3V3,GND], [VCC_1V2,GND])

    Crystal2.DST310S(NC) X6
    X6.setup(GND).XTAL -> UC.XTAL

    func i2c()
    {
        UC.i2c(0x36).I2C0 -> I2C0
    }

    func loadFlash(spi)
    {
        spi + UC.SPI
    }

    UART0 - res[1:2]::RES(0Ω) - UC.UART0
    RES(100kΩ).Pullup([UC.7, UC.VDD])

    MIC{P,N} -> [C4::CAP(),C5::CAP()] -> UC.ADC{P,N}

    UC.6 -> CAP(2.2uF, ±20%, CAP.X5R, 25V) -> RES(15kΩ, ±1%)
    -> (
        CAP(330nF, CAP.X5R, 10V) -> DAC_OUT,
        (CAP(1nF, ±20%, CAP.X5R, 25V) + RES(10kΩ)) -> GND
    )
    UC.19 -> SPK_MUTE
}

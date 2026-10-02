# ---------------------------------------------------------------------------------------------
#  Copyright (c) MCODE. All rights reserved.
# ---------------------------------------------------------------------------------------------
use ./power.mc


component MCU.US513_20_F
{
    partno = "US513U61"
    package = PKG.QFN20

    pins = [
        io [1,2] =  I2C0::I2C(MASTER)
                    | GPIO[3, 4]::GPIO(CONTROLLER)

        in [3,4] =  XTAL::XTAL(32kHz)

        psnk [5,21] = [VDD, GND]::DC(3.3V)

        io [6,7] =  UART0::UART.TTL(DCE)
                    | I2C1::I2C(MASTER)
                    | GPIO[5, 6]::GPIO(CONTROLLER)

        io [8,9] =  PDM[CLK, DATA] 
                    | PBus{CLK, DATA} 
                    | GPIO[7, 8]::GPIO(CONTROLLER)
        
        io [10,11] = I2C1::I2C(MASTER) | GPIO[9, 10]::GPIO(CONTROLLER),
                    ["I2C", "GPIO"], volt:1.2V, amp:100mA

        // Master wire order [SCLK, MOSI, MISO, CS] (b3804 face): pins
        // 8=SCLK, 9=MOSI, 11=MISO, 10=CSN
        io [8, 9, 11, 10] = SPI{SCLK, MOSI, MISO, CSN}::SPI(MASTER)

        io [12,13] = UART1::UART.TTL(DCE)
                    | GPIO[5, 6]::GPIO(CONTROLLER)

        psnk [14,21] = [VDD_CORE,GND]::DC(1.2V)

        in 15 = AVDD09_CAP

        io [16, 17] = ADC::ADC.DIFF(RECEIVER)

        io [18,19] = JTAG::DBG.JTAG.2WIRE(TAP)
                    | GPIO[0,1]::GPIO(CONTROLLER)

        io 20 = GPIO[2] | EXT_CLK_IN
    ]

    func Power([VDD_3V3, GND]::DC(3.3V), [VCC_1V2, GND]::DC(1.2V))
    {
        VDD_3V3 - CAP(1uF, ±10%, CAP.X5R, 10V) - GND
        VDD_3V3 - VDD
        this.GND - GND
        VCC_1V2 - CAP(1uF, ±10%, CAP.X5R, 10V) - GND
        VCC_1V2 - VDD_CORE
        GND -> this.GND
        AVDD09_CAP - CAP(1uF, ±10%, CAP.X5R, 10V) - GND
    }

    func I2C(address)
    {

        if address == 0x36
            VDD -> RES(100kΩ) -> GPIO[2]
        else //if address == 0x35
            GPIO[2] - RES(100kΩ) -> GND

        I2C0.SCL - RES(10kΩ) - VDD
        I2C0.SDA - RES(10kΩ) - VDD
    }
}

component Crystal2.DST310S
{
    partno = "DST310S"
    package = PKG.XTAL_3215
    spec = [
        frequency = 32kHz
    ]
    
    pins = [ 
        [1,2] = XTAL::XTAL()
    ]

    func Setup(GND) {
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
        
        // Slave wire order [SCLK, SI, SO, CS] (b3804 face): pins
        // 6=SCLK, 5=SI, 2=SO, 1=_CS
        [6, 5, 2, 1] = SPI::SPI(SLAVE)
    ]
        
    func GD25Q32E([V3V3, GND]::DC(3.3V))
    {
        [V3V3, GND] -> [VCC, VSS]
        VCC - CAP(100nF, ±20%, CAP.X5R, 25V) - VSS

        _CS - RES(10kΩ) - V3V3
        _WP - RES(10kΩ) - V3V3
        _HOLD - RES(10kΩ) - V3V3
    }
}

module US513(psnk [VDD_3V3,GND]::DC(3.3V), psnk [VCC_1V2,GND]::DC(1.2V))
{
    io MIC{P,N}, I2C0, SPI, UART0, UART1, PORT_1{A,B,C,D}
    out DAC_OUT, SPK_MUTE

    MCU.US513_20_F UC

    UC.Power([VDD_3V3,GND], [VCC_1V2,GND])

    Crystal2.DST310S(NC) X6
    X6.Setup(GND).XTAL -> UC.XTAL

    func I2C()
    {
        UC.I2C(0x36).I2C0 -> I2C0
    }

    func LoadFlash(SPI)
    {
        SPI + UC.SPI
    }

    UART0 - R[1:2]::RES(0Ω) - UC.UART0
    UC.7 - RES(100kΩ) - UC.VDD

    MIC{P,N} -> [C4::CAP(),C5::CAP()] -> UC.ADC{P,N}

    UC.6 -> CAP(2.2uF, ±20%, CAP.X5R, 25V) -> RES(15kΩ, ±1%)
    -> (
        CAP(330nF, CAP.X5R, 10V) -> DAC_OUT,
        (CAP(1nF, ±20%, CAP.X5R, 25V) + RES(10kΩ)) -> GND
    )
    UC.19 -> SPK_MUTE
}

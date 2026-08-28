# ---------------------------------------------------------------------------------------------
#  Copyright (c) MCODE. All rights reserved.
# ---------------------------------------------------------------------------------------------

# =============================================================================
# =============================================================================
define USB.MINI_B {
    manufacture = "Manufacture"
    partno = "HUM011D-5-S"
    package = PKG.USB_Mini
    symbol = "usb.mini_socket.sym"
    footprint = "usbmini.ftp"
}

define LDO.SGM2019_33YN5G_TR {
    manufacture = "Manufacture"
    partno = "SGM2019-3.3YN5G/TR"
    package = PKG.SOT_23_5
    symbol = "ldo.sgm2019_33yn5g_tr.sym"
    footprint = "sgm2019.ftp"
}

define DCDC.LP3220AB5F {
    manufacture = "Manufacture"
    partno = "LP3220AB5F"
    package = PKG.SOT_23_5
    symbol = "dcdc.lp3220ab5f.sym"
    footprint = "lp3220.ftp"
}

define IND.2u2H_1A5 {
    manufacture = "Manufacture"
    partno = ""
    package = PKG.L1206
    spec = [
        inductance = 2.2uH
        current = 1.5A
    ]
    symbol = "ind.2u2H_1A5.sym"
    footprint = "ind.2u2H_1A5.ftp"
}

# =============================================================================
# =============================================================================

define MCU.US513_20_F {
    manufacture = "Manufacture"
    partno = "US513_20_F"
    package = PKG.QFN20
    symbol = "mcu.us513_20_f.sym"
    footprint = "us513.ftp"
}

define Crystal2.DST310S {
    manufacture = "Manufacture"
    partno = "DST310S"
    package = PKG.Xtal_3215
    symbol = "crystal2.dst310s.sym"
    footprint = "dst310s.ftp"
}

define FLASH.GD25Q32E {
    manufacture = "Manufacture"
    partno = "GD25Q32ESIG"
    package = PKG.SOP8
    symbol = "flash.gd25q32e.sym"
    footprint = "gd25q32e.ftp"

    // GD25Q32E SPI Flash - 8-pin SOIC
    // Pinout: CS=/1, DO=/2, WP=/3, GND=/4, DI=/5, CLK=/6, HOLD=/7, VCC=/8
    pins = [
        in  1 = CS,     "Chip Select (Low active)"
        out 2 = DO,     "Data Output (IO1/SO)"
        in  3 = WP,     "Write Protect (IO2, pull-up to VCC)"
        ps  4 = GND,    "Ground (VSS)"
        in  5 = DI,     "Data Input (IO0/SI)"
        in  6 = CLK,    "Serial Clock (SCLK)"
        in  7 = HOLD,   "Hold (IO3, pull-up to VCC)"
        ps  8 = VCC,    "Power (VDD)"
    ]
}

# =============================================================================
# =============================================================================

define Microphone.Sip2
{
    manufacture = "Manufacture"
    partno = "SIP2-1.25MM-WA"
    package = PKG.SIP2
    pitch = 1.25mm
    symbol = "microphone.sip2.sym"
    footprint = "SIP2-1.25MM-WA.ftp"
}

define MICROPHONE.SIP2 {
    manufacture = "Manufacture"
    partno = "SIP2-1.25MM-WA"
    package = PKG.SIP2
    symbol = "microphone.sip2.sym"
    footprint = "sip2.ftp"
}

define LPA4871 {
    manufacture = "Manufacture"
    partno = "LPA4871"
    package = PKG.SOP8
    symbol = "lpa4871.sym"
    footprint = "lpa4871.ftp"
}

define SPEAKER.PHB2AWB {
    manufacture = "Manufacture"
    partno = "PHB-2AWB"
    package = PKG.SPEAKER_PHB2AWB
    symbol = "speaker.phb2awb.sym"
    footprint = "phb2awb.ftp"
}

define CAP.10uF_X5R_10V
{
    manufacture = "Manufacture"
    partno = "C3225X5R1C106K"
    package = PKG.C0805
    spec = [
        capacitance = 10uF
        tolerance = ±20%
        dielectric = CAP.X5R 
        voltage = 10V
    ]
    footprint = "cap_0805.ftp"
}
define CAP.100nF_X5R_25V
{
    manufacture = "Manufacture"
    partno = "C1608X5R1C104K"
    package = PKG.C0603
    spec = [
        capacitance = 100nF
        tolerance = ±20%
        dielectric = CAP.X5R
        voltage = 25V
    ]
    footprint = "cap_0603.ftp"
}
define CAP.1uF_X5R_10V
{
    manufacture = "Manufacture"
    partno = "C1608X5R1C105K"
    package = PKG.C0603
    spec = [
        capacitance = 1uF
        tolerance = ±10%
        dielectric = CAP.X5R
        voltage = 10V
    ]
    footprint = "cap_0603.ftp"
}
define CAP.18pF_NPO_50V
{
    manufacture = "Manufacture"
    partno = "GRM1555C1H180JA01D"
    package = PKG.C0402
    spec = [
        capacitance = 18pF
        tolerance = ±5%
        dielectric = CAP.C0G
        voltage = 50V
    ]
    footprint = "cap_0402.ftp"
}
define CAP.2u2_X5R_25V
{
    manufacture = "Manufacture"
    partno = "C1608X5R1C225K"
    package = PKG.C0603
    spec = [
        capacitance = 2.2uF
        tolerance = ±20%
        dielectric = CAP.X5R
        voltage = 25V
    ]
    footprint = "cap_0603.ftp"
}
define CAP.330nF_X5R_10V
{
    manufacture = "Manufacture"
    partno = "C1608X5R1C334K"
    package = PKG.C0603
    spec = [
        capacitance = 330nF
        tolerance = ±20%
        dielectric = CAP.X5R
        voltage = 10V
    ]
    footprint = "cap_0603.ftp"
}
define CAP.4u7_Y5V_6V3
{
    manufacture = "Manufacture"
    partno = "C1608Y5V1C475K"
    package = PKG.C0603
    spec = [
        capacitance = 4.7uF
        tolerance = ±20%
        dielectric = CAP.Y5V
        voltage = 6.3V
    ]
    footprint = "cap_0603.ftp"
}
define CAP.470pF_X5R_10V
{
    manufacture = "Manufacture"
    partno = "C1608X5R1C471K"
    package = PKG.C0603
    spec = [
        capacitance = 470pF
        tolerance = ±20%
        dielectric = CAP.X5R
        voltage = 10V
    ]
    footprint = "cap_0603.ftp"
}
define CAP.1nF_X5R_25V
{
    manufacture = "Manufacture"
    partno = "C1608X5R1C102K"
    package = PKG.C0603
    spec = [
        capacitance = 1nF
        tolerance = ±20%
        dielectric = CAP.X5R
        voltage = 25V
    ]
    footprint = "cap_0603.ftp"
}
define CAP.15pF_NPO_50V
{
    manufacture = "Manufacture"
    partno = "GRM1555C1H150JA01D"
    package = PKG.C0402
    spec = [
        capacitance = 15pF
        tolerance = ±5%
        dielectric = CAP.C0G
        voltage = 50V
    ]
    footprint = "cap_0402.ftp"
}
define RES.137kΩ_1%_R0805
{
    manufacture = "Manufacture"
    partno = "RC0805FR-07137KL"
    package = PKG.R0805
    spec = [
        resistance = 137kΩ
        tolerance = 1%
        voltage = 10V
    ]
    footprint = "res_0805.ftp"
}
define RES.10kΩ_R0805
{
    manufacture = "Manufacture"
    partno = "RC0805FR-0710KL"
    package = PKG.R0805
    spec = [
        resistance = 10kΩ
        tolerance = ±5%
        voltage = 10V
    ]
    footprint = "res_0805.ftp"
}
define RES.100kΩ_R0805
{
    manufacture = "Manufacture"
    partno = "RC0805FR-07100KL"
    package = PKG.R0805
    spec = [
        resistance = 100kΩ
        tolerance = ±5%
        voltage = 10V
    ]
    footprint = "res_0805.ftp"
}
define RES.1MΩ_1%_R0402
{
    manufacture = "Manufacture"
    partno = "RC0402FR-071ML"
    package = PKG.R0402
    spec = [
        resistance = 1MΩ
        tolerance = ±1%
        voltage = 10V
    ]
    footprint = "res_0402.ftp"
}
define RES.15kΩ_1%_R0805
{
    manufacture = "Manufacture"
    partno = "RC0805FR-0715KL"
    package = PKG.R0805
    spec = [
        resistance = 15kΩ
        tolerance = ±1%
        voltage = 10V
    ]
    footprint = "res_0805.ftp"
}
define RES.240Ω_1%_R0402
{
    manufacture = "Manufacture"
    partno = "RC0402FR-07240RL"
    package = PKG.R0402
    spec = [
        resistance = 240Ω
        tolerance = ±1%
        voltage = 10V
    ]
    footprint = "res_0402.ftp"
}
define RES.1kΩ_1%_R0402
{
    manufacture = "Manufacture"
    partno = "RC0402FR-071KL"
    package = PKG.R0402
    spec = [
        resistance = 1kΩ
        tolerance = ±1%
        voltage = 10V
    ]
    footprint = "res_0402.ftp"
}
define RES.30kΩ_1%_R0805
{
    manufacture = "Manufacture"
    partno = "RC0805FR-0730KL"
    package = PKG.R0805
    spec = [
        resistance = 30kΩ
        tolerance = ±1%
        voltage = 10V
    ]
    footprint = "res_0805.ftp"
}
define RES.0R_R0603
{
    manufacture = "Manufacture"
    partno = "RC0603JR-070RL"
    package = PKG.R0603
    spec = [
        resistance = 0R
        tolerance = ±5%
        voltage = 10V
    ]
    footprint = "res_0603.ftp"
}
define RES.47kΩ_R0805
{
    manufacture = "Manufacture"
    partno = "RC0805FR-0747KL"
    package = PKG.R0805
    spec = [
        resistance = 47kΩ
        tolerance = ±5%
        voltage = 10V
    ]
    footprint = "res_0805.ftp"
}
define RES.150kΩ_1%_R0805
{
    manufacture = "Manufacture"
    partno = "RC0805FR-07150KL"
    package = PKG.R0805
    spec = [
        resistance = 150kΩ
        tolerance = ±1%
        voltage = 10V
    ]
    footprint = "res_0805.ftp"
}
define RES.0Ω_R0805
{
    manufacture = "Manufacture"
    partno = "RC0805JR-070RL"
    package = PKG.R0805
    spec = [
        resistance = 0Ω
        tolerance = ±5%
        voltage = 10V
    ]
    footprint = "res_0805.ftp"
}
define RES.0R_NC
{
    manufacture = "Manufacture"
    partno = "RC0805JR-070RL"
    package = PKG.R0805
    spec = [
        resistance = 0R
        tolerance = ±5%
        voltage = 10V
    ]
    footprint = "res_0805.ftp"
}

# ESD Diode
define DIO.ESD9B5V_2_TR
{
    manufacture = "Manufacture"
    partno = "ESD9B5V-2/TR"
    package = PKG.SOT_23
    spec = [
        voltage = 5V
        capacitance = 0.8pF
    ]
    footprint = "esd.ftp"
}

# Test Point
define TEST_POINT
{
    manufacture = "Manufacture"
    partno = "TP-0805"
    package = PKG.PAD_CIRC_1_5MM
    footprint = "test_point.ftp"
}

define main
{
    libpath = "./blib/"    // path to BOM files
    
    usbsocket.usb1       = USB.MINI_B
    usbsocket.res26_power = RES.0R_R0603

    modldo.ldo          = LDO.SGM2019_33YN5G_TR
    modldo.cap_in       = CAP.10uF_X5R_10V
    modldo.cap_out      = CAP.4u7_Y5V_6V3
    modldo.cap59_vin    = CAP.10uF_X5R_10V
    modldo.cap62_vout   = CAP.4u7_Y5V_6V3

    moddcdc.p322dcdc    = DCDC.LP3220AB5F
    moddcdc.cap_in      = CAP.10uF_X5R_10V
    moddcdc.cap_en      = CAP.1uF_X5R_10V
    moddcdc.inductor    = IND.2u2H_1A5
    moddcdc.cap_out1    = CAP.10uF_X5R_10V
    moddcdc.cap_out2    = CAP.100nF_X5R_25V
    moddcdc.res_fb1     = RES.137kΩ_1%_R0805
    moddcdc.res83_en    = RES.47kΩ_R0805
    moddcdc.cap97_vin   = CAP.10uF_X5R_10V
    moddcdc.cap98_en    = CAP.1uF_X5R_10V
    moddcdc.cap102_vout = CAP.10uF_X5R_10V
    moddcdc.cap103_vout = CAP.100nF_X5R_25V
    moddcdc.res106a_fb  = RES.137kΩ_1%_R0805
    moddcdc.res106b_fb  = RES.150kΩ_1%_R0805
    moddcdc.cap107_fb   = CAP.15pF_NPO_50V

    mcu513.uC                   = MCU.US513_20_F
    mcu513.X6                   = Crystal2.DST310S
    mcu513.cap50_vdd_io         = CAP.1uF_X5R_10V
    mcu513.cap51_vdd_core       = CAP.1uF_X5R_10V
    mcu513.res67_i2c            = RES.10kΩ_R0805
    mcu513.res80_crystal        = RES.1MΩ_1%_R0402
    mcu513.cap82_crystal        = CAP.18pF_NPO_50V
    mcu513.cap83_crystal        = CAP.18pF_NPO_50V
    mcu513.cap107_avdd          = CAP.100nF_X5R_25V
    mcu513.res109_cs            = RES.10kΩ_R0805
    mcu513.res110_wp            = RES.10kΩ_R0805
    mcu513.res111_hold          = RES.10kΩ_R0805
    mcu513.res137a_uart         = RES.0Ω_R0805
    mcu513.res137b_uart         = RES.0Ω_R0805
    mcu513.res138_uart_rx       = RES.100kΩ_R0805
    mcu513.cap141a_mic          = CAP.1uF_X5R_10V
    mcu513.cap141b_mic          = CAP.1uF_X5R_10V
    mcu513.cap144_dac           = CAP.2u2_X5R_25V
    mcu513.res144_dac           = RES.15kΩ_1%_R0402
    mcu513.cap146_dac_out       = CAP.330nF_X5R_10V
    mcu513.cap147_dac_filter    = CAP.1nF_X5R_25V
    mcu513.res147_dac_filter    = RES.10kΩ_R0805

    flash.cap107_vcc           = CAP.100nF_X5R_25V
    flash.res109_cs            = RES.10kΩ_R0805
    flash.res110_wp            = RES.10kΩ_R0805
    flash.res111_hold          = RES.10kΩ_R0805

    mic.mic                    = MICROPHONE.SIP2
    mic.res34_vmic             = RES.240Ω_1%_R0402
    mic.cap34_vmic             = CAP.4u7_Y5V_6V3
    mic.res36a_bias            = RES.1kΩ_1%_R0402
    mic.res36b_bias            = RES.1kΩ_1%_R0402
    mic.cap37_bias             = CAP.1uF_X5R_10V
    mic.res38a_mic             = RES.1kΩ_1%_R0402
    mic.res38b_mic             = RES.1kΩ_1%_R0402
    mic.cap41_mic              = CAP.470pF_X5R_10V
    mic.esd43a                 = DIO.ESD9B5V_2_TR
    mic.esd43b                 = DIO.ESD9B5V_2_TR
    mic.res56_gnd              = RES.0R_NC
    mic.cap54_wm7121           = CAP.100nF_X5R_25V

    speaker.lpa                 = LPA4871
    speaker.spk                 = SPEAKER.PHB2AWB
    speaker.cap97_vdd           = CAP.10uF_X5R_10V
    speaker.res98_en            = RES.10kΩ_R0805
    speaker.cap99_bypass        = CAP.1uF_X5R_10V
    speaker.res101a_in          = RES.15kΩ_1%_R0805
    speaker.res101b_feedback    = RES.30kΩ_1%_R0805
    speaker.tp104a              = TEST_POINT
    speaker.tp104b              = TEST_POINT
    speaker.dio104_esd          = DIO.ESD9B5V_2_TR
}

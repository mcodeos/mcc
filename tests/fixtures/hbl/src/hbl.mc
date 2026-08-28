# ---------------------------------------------------------------------------------------------
#  Copyright (c) MCODE. All rights reserved.
# ---------------------------------------------------------------------------------------------

use ./power.mc
use ./us513.mc
use ./periph.mc

module main
{
    POWER_USB           USB
    POWER_LDO           LDO
    POWER_DCDC          DCDC
    MIC_SIP             MIC
    SPEAKER_M           SPK(V3V3)

    USB.vin -> V5V::DC(5V)
    V5V -> LDO{vin|vout} -> V3V3::DC(3.3V)
    V3V3 -> DCDC -> V1V2::DC(1.2V)

    US513 MCU513(V3V3, V1V2)
    FLASH.GD25Q32E FLASH(V3V3)
    MCU513.i2c().loadFlash(FLASH.SPI)
    
    MIC(V3V3).MIC -> MCU513{ MIC | DAC_OUT, SPK_MUTE } -> SPK{DAC_OUT, US_SPEAKER_MUTE}

}

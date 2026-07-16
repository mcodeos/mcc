# mcode Class Naming Convention

> **Version**: v1.0
> **Date**: 2026-07-17
>
> This document defines the naming rules for all class definitions (component, interface, enum)
> in the mcode standard component library.

---

## 1. General Rules

1. **All class names SHALL be `UPPER_CASE`** — letters A–Z, digits 0–9, and the allowed separators below.
2. **Hierarchy is expressed with dot (`.`) separator** — `FAMILY.SUBTYPE`.
3. **Multi-word sub-names use underscore (`_`)** — `MIL_SPEC`, `DC_JACK`, `BANANA_PLUG`.
4. **Industry-standard acronyms are preferred** over fully-spelled words — `LDO` not `LOW_DROPOUT`, `SMD` not `SURFACE_MOUNT_DEVICE`.
5. **Proper nouns, model numbers, and standard codes are kept intact** (no underscore insertion) — `XT60`, `DIN41612`, `ANDERSON`, `SPEAKON`.
6. **Manufacturer series names use underscore** between manufacturer and series — `JST_XH`, `MOLEX_KK`.
7. **Digits may appear as part of a name** — `TRS_35MM`, `HDR_1x10`, `SPI.3WIRE`.

---

## 2. Component Naming

### 2.1 Base Family Name

The base family is a **2–5 character industry-standard abbreviation**:

| Family | Meaning |
|---|---|
| `RES` | Resistor |
| `CAP` | Capacitor |
| `IND` | Inductor |
| `DIO` | Diode |
| `TRANS` | Transistor — BJT, IGBT, SCR, TRIAC |
| `FET` | Field-Effect Transistor — JFET, MOSFET |
| `AMP` | Amplifier (op-amp, instrumentation amp, comparator, OTA, buffer) |
| `LED` | LED |
| `OPTO` | Optocoupler |
| `RELAY` | Relay |
| `SWITCH` | Switch |
| `FUSE` | Fuse |
| `FILTER` | Filter |
| `XTAL` | Crystal / Oscillator / Ceramic resonator |
| `XFR` | Transformer |
| `ANT` | Antenna |
| `DC` | DC Power source |
| `REG` | Regulator |
| `SENSOR` | Sensor |
| `TP` | Test Point |

### 2.2 Subtype

Subtypes refine the base family by technology, package, or functional variant:

```
RES.SMD          Surface-mount resistor
RES.THT          Through-hole resistor
RES.NTC          NTC thermistor (negative temperature coefficient)
RES.PTC          PTC thermistor (positive temperature coefficient)
CAP.ELEC         Electrolytic capacitor (polarized)
CAP.CER          Ceramic capacitor
DIO.SCH          Schottky diode
DIO.ZEN          Zener diode
DIO.PHOTO        Photodiode
TRANS.NPN        NPN bipolar junction transistor
TRANS.DARLINGTON  Darlington pair transistor
FET.MOSFET.N      N-channel MOSFET
FET.JFET.P        P-channel JFET
REG.LDO          Low-dropout regulator
REG.BUCK         Buck (step-down) switching regulator
REG.BUCK_BOOST   Buck-boost switching regulator
```

Rules:
- Technology variants use standard abbreviations: `SCH`, `ZEN`, `TVS`, `LDO`.
- Transistor polarity/channel uses standard notation: `NPN`, `PNP`, `NMOS`, `PMOS`.
- Package variants may be used where they disambiguate: `XTAL.SMD`, `FUSE.SMD`.

### 2.3 Connector Namespaces

Connectors are categorized by physical interface type as top-level namespaces:

| Namespace | Category | Examples | Referenced Standard |
|---|---|---|---|
| `AUDIO` | Audio connectors | `AUDIO.TRS_35MM`, `AUDIO.XLR` | EIA RS-453 (TRS), IEC 61076-2-103 (XLR) |
| `CIRC` | Circular / RF connectors | `CIRC.BNC`, `CIRC.SMA`, `CIRC.MIL_SPEC` | MIL-STD-348 (BNC), IEC 60169 (SMA), MIL-DTL-38999 (MIL-SPEC) |
| `USB` | USB 2.x connectors | `USB.TYPEA`, `USB.MICROB`, `USB.C` | USB-IF connector specifications |
| `USB3` | USB 3.x connectors | `USB3.TYPEA`, `USB3.MICROB` | USB-IF USB 3.x connector specifications |
| `VIDEO` | Video connectors | `VIDEO.HDMI`, `VIDEO.DISPLAYPORT` | HDMI Licensing specification, VESA DisplayPort |
| `POWER` | Power connectors | `POWER.DC_JACK`, `POWER.XT60`, `POWER.ATX` | de facto (DC jack), Amass XT60, Intel ATX |
| `WTB` | Wire-to-board connectors | `WTB.JST_XH`, `WTB.MOLEX_KK` | JST XH series, Molex KK 254 series |
| `B2B` | Board-to-board connectors | `B2B`, `MEZZANINE` | de facto |
| `HDR` | Pin headers | `HDR_1x10`, `HDR_2x5` | de facto (0.1″ / 2.54 mm pitch) |

**Note on `HDR`**: Pin headers use underscore (`_`) instead of dot for the pin-count suffix
(`HDR_1x10`, `HDR_2x5`) because the count is a parametric dimension, not a subtype.
The `x` format (`1x10` = 1 row × 10 pins) is the industry-standard shorthand.

---

## 3. Interface Naming

### 3.1 Communication Protocols

Protocol names use their **industry-standard acronym**:

| Interface | Description | Referenced Standard |
|---|---|---|
| `SPI` | Serial Peripheral Interface (4-wire) | Motorola SPI (de facto) |
| `SPI.3WIRE` | SPI 3-wire variant | Motorola SPI (de facto) |
| `SPI.QUAD` | Quad SPI (6-wire, 4 bidirectional data lines) | JEDEC JESD216 (SFDP) |
| `SDIO` | SD Card interface (4-bit) | SD Association specification |
| `SDIO.1BIT` | SD Card interface (1-bit) | SD Association specification |
| `I2C` | Inter-Integrated Circuit | NXP UM10204 (I²C-bus specification) |
| `I2C.SMBUS` | System Management Bus (I2C variant) | SBS Implementers Forum specification |
| `UART.TTL` | UART at TTL logic levels | de facto (TTL logic levels) |
| `UART.RS232` | RS-232 | EIA/TIA-232-F |
| `UART.RS422` | RS-422 differential | EIA/TIA-422-B |
| `UART.RS423` | RS-423 unbalanced differential | EIA/TIA-423-B |
| `UART.RS449` | RS-449 enhanced | EIA/TIA-449 |
| `UART.RS485` | RS-485 multi-point | EIA/TIA-485-A |
| `CAN` | Controller Area Network | ISO 11898 (all parts) |
| `LIN` | Local Interconnect Network | ISO 17987 (all parts) |
| `FLEXRAY` | FlexRay automotive bus | ISO 17458 (all parts) |
| `ETHERNET` | Ethernet (10/100/1000/10G) | IEEE 802.3 |
| `ONEWIRE` | 1-Wire | Maxim/Dallas proprietary |
| `MOST` | Media Oriented Systems Transport | MOST Cooperation specification |
| `I2S` | Inter-IC Sound | NXP I²S specification |
| `PCM` | Pulse Code Modulation (digital audio) | AES3 / IEC 60958 |

### 3.2 Analog Interfaces

| Interface | Description | Referenced Standard |
|---|---|---|
| `ADC.DIFF` | Differential ADC input | de facto |
| `DAC` | Digital-to-Analog Converter output | de facto |
| `PWM` | Pulse Width Modulation | de facto |
| `GPIO` | General Purpose I/O | de facto |

### 3.3 USB Interfaces

All USB interface names follow **USB-IF** connector and protocol designations.

| Interface | Description | Referenced Standard |
|---|---|---|
| `USB` | USB 2.0 base interface | USB-IF USB 2.0 specification |
| `USB.TYPEA` | USB 2.0 Type A | USB-IF Type-A connector specification |
| `USB.TYPEB` | USB 2.0 Type B | USB-IF Type-B connector specification |
| `USB.MINIB` | USB 2.0 Mini B | USB-IF Mini-B connector specification |
| `USB.MICROB` | USB 2.0 Micro B | USB-IF Micro-B connector specification |
| `USB.C` | USB Type C (24-pin) | USB-IF Type-C specification |
| `USB.DATA` | USB data lines only (D+/D−) | USB-IF USB 2.0 (data subset) |
| `USB.PD` | USB Power Delivery | USB-IF Power Delivery specification |
| `USB3.TYPEA` | USB 3.x Type A | USB-IF USB 3.x Type-A specification |
| `USB3.TYPEB` | USB 3.x Type B | USB-IF USB 3.x Type-B specification |
| `USB3.MICROB` | USB 3.x Micro B | USB-IF USB 3.x Micro-B specification |
| `USB3.TX` | USB 3.x SuperSpeed transmit pair | USB-IF USB 3.x SuperSpeed |
| `USB3.RX` | USB 3.x SuperSpeed receive pair | USB-IF USB 3.x SuperSpeed |

### 3.4 Debug Interfaces

Debug interfaces are grouped under the `DBG` namespace:

| Interface | Description | Referenced Standard |
|---|---|---|
| `DBG.JTAG` | Standard 5-wire JTAG | IEEE 1149.1 (JTAG) |
| `DBG.JTAG.2WIRE` | 2-wire JTAG (cJTAG) | IEEE 1149.7 (cJTAG) |
| `DBG.SWD` | ARM Serial Wire Debug | ARM CoreSight SWD |
| `DBG.SWIM` | ST SWIM single-wire debug | STMicroelectronics SWIM |
| `DBG.DAP` | ARM Debug Access Port | ARM CoreSight DAP |
| `DBG.DAP3PU` | DAP 3-pin unidirectional | ARM CoreSight |
| `DBG.DAPWM` | DAP wide mode | ARM CoreSight |
| `DBG.CMSISDAP` | ARM CMSIS-DAP | ARM CMSIS-DAP |
| `DBG.ICD` | Microchip In-Circuit Debugger | Microchip ICD |
| `DBG.UARTBOOT` | UART bootloader | de facto (various MCU ROM bootloaders) |

### 3.5 Logic Gate Interfaces

| Interface | Description | Referenced Standard |
|---|---|---|
| `LOGIC.AND` | AND gate | de facto (Boolean algebra) |
| `LOGIC.OR` | OR gate | de facto (Boolean algebra) |
| `LOGIC.NOT` | NOT gate | de facto (Boolean algebra) |
| `LOGIC.NAND` | NAND gate | de facto (Boolean algebra) |
| `LOGIC.NOR` | NOR gate | de facto (Boolean algebra) |
| `LOGIC.XOR` | XOR gate | de facto (Boolean algebra) |
| `LOGIC.XNOR` | XNOR gate | de facto (Boolean algebra) |

### 3.6 Infrastructure Interfaces

| Interface | Description | Referenced Standard |
|---|---|---|
| `XTAL` | Crystal / oscillator pins (xin, xout) | de facto |
| `DC` | DC power supply rails | de facto |

---

## 4. Package Naming (PKG Enum)

The `PKG` enum follows **JEDEC Publication 95 / IPC-7351** standards, with adaptations for
the mcode parser:

### 4.1 Rules

1. **Base family name is glued to the pin count** — `DIP8`, `QFN48`, `LQFP100`.
2. **Body size is appended with underscore** — `QFN20_4x4`, `QFN20_5x5`.
3. **Family prefix modifiers are part of the family name** — `VQFN16_3x3` (Very-thin QFN).
4. **Hyphens in JEDEC names are replaced with underscores** — `SOT-23-3` → `SOT_23_3`, `TO-220` → `TO_220`.
5. **All identifiers are UPPER_CASE**.

### 4.2 Examples

| JEDEC Name | mcode PKG Variant |
|---|---|
| SOT-23-3 | `SOT_23_3` |
| SOT-23-5 | `SOT_23_5` |
| TO-220 | `TO_220` |
| QFN-48 (7×7) | `QFN48_7x7` |
| LQFP-100 (14×14) | `LQFP100_14x14` |
| BGA-256 | `BGA256` |
| 0402 (imperial) | `C0402` (capacitor), `R0402` (resistor) |

### 4.3 Chip Package Prefix Letters

| Prefix | Component Type | Example |
|---|---|---|
| `C` | Capacitor (EIA codes) | `C0402` |
| `R` | Resistor (EIA codes) | `R0402` |
| `L` | Inductor / Ferrite bead | `L0402` |
| `D` | Diode package | — |
| `T` | Transistor / IC outline | — |

---

## 5. Abbreviation Reference

### 5.1 Accepted Abbreviations

| Abbrev | Full Term |
|---|---|
| SMD | Surface Mount Device |
| THT | Through-Hole Technology |
| ELEC | Electrolytic |
| CER | Ceramic |
| TANT | Tantalum |
| CMC | Common Mode Choke |
| FB | Ferrite Bead |
| HF | High Frequency |
| SCH | Schottky |
| ZEN | Zener |
| TVS | Transient Voltage Suppressor |
| PHOTO | Photodiode |
| ESD | Electrostatic Discharge |
| NPN / PNP | NPN / PNP |
| NMOS / PMOS | N-channel / P-channel MOSFET |
| JFET | Junction Field-Effect Transistor |
| IGBT | Insulated Gate Bipolar Transistor |
| SCR | Silicon Controlled Rectifier |
| OTA | Operational Transconductance Amplifier |
| LDO | Low Dropout |
| PTC | Positive Temperature Coefficient |
| LP / HP / BP / BS / AP | Low-pass / High-pass / Band-pass / Band-stop / All-pass |
| SC | Switched Capacitor |
| CT | Center Tapped |
| ISO | Isolation |
| XTAL | Crystal |
| OSC | Oscillator |
| TRS | Tip-Ring-Sleeve |
| WTB | Wire-to-Board |
| B2B | Board-to-Board |

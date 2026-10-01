# mcode Class Naming Convention

> **Version**: v1.1
> **Date**: 2026-10-01
>
> This document defines the naming rules for all class definitions (component, interface, enum)
> in the mcode standard component library, and for instance designators (§7).

---

## 1. General Rules

1. **All class names SHALL be `UPPER_CASE`** — letters A–Z, digits 0–9, and the allowed separators below.
2. **Hierarchy is expressed with dot (`.`) separator** — `FAMILY.SUBTYPE`.
3. **Multi-word sub-names use underscore (`_`)** — `MIL_SPEC`, `DC_JACK`, `BANANA_PLUG`.
4. **Industry-standard acronyms are preferred** over fully-spelled words — `LDO` not `LOW_DROPOUT`, `SMD` not `SURFACE_MOUNT_DEVICE`.
5. **Proper nouns, model numbers, and standard codes are kept intact** (no underscore insertion) — `XT60`, `DIN41612`, `ANDERSON`, `SPEAKON`.
6. **Manufacturer series names use underscore** between manufacturer and series — `JST_XH`, `MOLEX_KK`.
7. **Digits may appear as part of a name** — `TRS_35MM`, `HDR.1X10`, `SPI.3WIRE`.

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
- Package variants may be used where they disambiguate: `FUSE.SMD`.

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
| `HDR` | Pin headers | `HDR.1X10`, `HDR.2X5` | de facto (0.1″ / 2.54 mm pitch) |

**Note on `HDR`**: Pin headers follow the family.face law (`HDR.1X10`, `HDR.2X5`); the
face is all-uppercase (`1X10`, not `1x10` — case is not significant in the industry
shorthand, and library faces are uppercase). The generic parametric faces are
`HDR.SINGLE(cols)` / `HDR.MULTI(rows, cols)`; the concrete `1XN`/`2XN` faces are
fixed-size variants in the `IEC.C8`/`C14` style.

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
| `LOGIC.NAND.3` | NAND gate, 3-input (fan-in variant) | de facto (74HC10 class) |
| `LOGIC.NAND.4` | NAND gate, 4-input (fan-in variant) | de facto (74HC20 class) |
| `LOGIC.NAND.8` | NAND gate, 8-input (fan-in variant) | de facto (74HC30 class) |
| `LOGIC.NOR` | NOR gate | de facto (Boolean algebra) |
| `LOGIC.XOR` | XOR gate | de facto (Boolean algebra) |
| `LOGIC.XNOR` | XNOR gate | de facto (Boolean algebra) |

Gate member names follow the 74-family data-book gate names (`A`/`B`/…/`Y`);
the default logical name equals the member name, so a plain adoption needs no
rename braces. Inverting outputs carry the `_` prefix on the member (`_AB`,
`_A`) while the logical name stays `Y`. Each fan-in lane count is its own
dotted family point, never a parameter (S3 R-CV3).

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

---

## 6. Pin Member Naming

Sections 1–5 govern **class names** (component, interface, enum). The member
names inside `pins = [...]` follow their own rules; the canonical text is
pin-semantics-and-usage-design.md §2.8 (mcd). The rules:

1. **Active-low signals carry the `_` prefix, kept in the registered name** —
   `_CS`, `_WP`, `_HOLD`. mcc derives `McPin.n = true` from the prefix
   automatically (semantic/component/mc_pins), so the polarity is machine-
   readable, not prose. The logical name stays plain: LOGIC inverting outputs
   are `_AB`/`_A` while the logical name stays `Y` (ifs/logic.mc).
2. **The bare `_` is not a name.** In a role-less conductor view (interface-
   level `pins` table) `_` is the anonymous lane of conductor-view R-CV1 — it
   asserts a lane, never a name. `_CS` (prefix + name) and `_` (anonymous
   lane) are distinct forms.
3. **Prose polarity markers are legacy.** Describing polarity in the
   description string ("active low") or using other prefixes (`nRST`) does not
   set `n`; such corpus is pending migration with its consuming ERC polarity
   pass (U365), so the rename lands together with the checker that reads it.

---

## 7. Instance Designator Naming

Sections 1–5 govern class names and §6 pin member names. This section governs
the **instance designator** — the name an instance carries in the module body:
author-written names (`R1`, `Q101`, `R[123:131]`) and the auto names of
anonymous constructions (`_C1`, `_TP2`).

### 7.1 Designator Form

An instance designator is **`PREFIX` + `DIGITS`** — one to three uppercase
letters (the designator prefix) glued to a contiguous decimal number:
`R1`, `C12`, `Q101`, `TP3`, `J4`. No separators (`R_1` is wrong), no zero
padding (`R001` is wrong), no unit-letter suffixes (`R1a` is not supported).
The rule holds for **both named and anonymous** instances; an anonymous
instance carries the same form behind the `_` anonymous-lane prefix (§7.4).

### 7.2 Designator Prefix

The prefix follows the family's industry reference-designator letter:

1. **Single letter first** — the family's own initial when it is the industry
   letter: `RES→R`, `CAP→C`, `DIO→D`, `FUSE→F`.
2. **Conventional letter where industry usage differs** — `IND→L` (`I` is
   reserved for current), `RELAY→K`, `XFR→T`, `XTAL→Y`, `TRANS→Q`.
3. **Family letter + variant letter when two families contest one letter** —
   `FET→QF`, `AUDIO→JA`, `VIDEO→JV`, `DC→PS`, `OSC→XO`, `FILTER→FL`;
   a few keep the industry multi-letter form (`LED`, `TP`).
4. **`M` is reserved** for the module segment of a projected refdes — no
   prefix may start with `M`.

The **one authoritative prefix table** is `REFDES_PREFIXES` in
`src/instant/refdes.rs`; this section deliberately does not duplicate it.
Lookup is by the **root segment of the resolved class name** (`CAP.MLCC` →
`C`, `FET.MOSFET.N` → `QF`); a class with no table row has no prefix, and no
consumer may derive one from the spelling. Representative rows:

| Family | Prefix | Family | Prefix |
|---|---|---|---|
| `RES` | `R` | `TRANS` | `Q` |
| `CAP` | `C` | `FET` | `QF` |
| `IND` | `L` | `RELAY` | `K` |
| `DIO` | `D` | `XTAL` / `XTAL2` / `XTAL4` | `Y` |
| `LED` | `LED` | `TP` | `TP` |

### 7.3 Numbering — Named Instances

1. **The author chooses the number.** Any decimal number is legal; gaps are
   legal (`R5` with no `R1`–`R4`); deleting an instance never obliges
   renumbering, and no other instance is auto-assigned a freed number.
2. **Recommended sequence** — in a fresh module, number contiguously from 1
   in reading (source) order. On multi-sheet designs the accepted
   alternative is **sheet-block numbering**: `R1xx` on sheet 1, `R2xx` on
   sheet 2, so the hundred digit names the sheet.
3. **Uniqueness scope is the module.** Two instances of one module may not
   carry the same designator (name binding); design-wide uniqueness is a
   BOM-time concern and is not a parse error.
4. **Prefix match is part of the convention** — a capacitor named `R5`
   parses, but its prefix disagrees with the family row (§7.2) and should
   be renamed.
5. **The range form follows the same law** — `R[123:131]` declares
   `R123`…`R131`; every member is `PREFIX` + `DIGITS`.

### 7.4 Numbering — Anonymous Constructions

An anonymous inline construction (`CAP(0.1uF)`, `TP()*2`,
`[VDD,GND]::DC(1.8V)`) receives an auto name in the **underscore anonymous
lane**:

1. **Form**: `_` + designator prefix + digits — `_C1`, `_R2`, `_TP1`. The
   `_` marks the anonymous lane — the same lane marker §6 rule 2 reserves on
   pin names — so an auto name can never collide with a named designator:
   `_R1` and `R1` coexist. A class with no table row falls back to its full
   class name, dots as underscores (`DIO.ESD` → `_DIO_ESD1`).
2. **The counter is independent.** It is keyed by `(module, prefix)`, starts
   at 1, and counts only anonymous instances: it neither skips nor fills
   numbers used by named instances, and named numbering is unaffected by
   anonymous constructions.
3. **Assignment order is spelling-position order** — statement order in the
   module body, left-to-right within a statement, block order for `*N` and
   list expansions.
4. **Auto names are not identity keys.** The spelled name is a pure function
   of the current source: inserting or removing an anonymous construction
   can renumber later anonymous instances of the same prefix. Registration
   is anchored by the construction site (`AutoAnchor`), not by the name, so
   the identity survives an edit even where the spelling does not — never
   reference an anonymous instance by its auto name in anything that must
   survive an edit.

### 7.5 Boundary: Instance Name vs Projected Refdes

The instance designator (this section) lives in the source. A separate
display refdes for BOM / schematic cross-checking — `M1C1`: module segment +
class prefix + in-class ordinal — is allocated at the end of the build from
the frozen circuit; see refdes-design.md (mcd, doc/world). The two naming
systems coexist and may be shown side by side; neither is ever written back
into the source.


---

## 8. Attribute Key Naming

Attribute rows (`key = value` in a component / module body), pin-row value
keys (`volt: 1.2V`), and interface-body rows share one key vocabulary. This
section is that vocabulary's **single authority**: which keys exist, which
face each is written on, what its value means, which contract half it
carries, and which words its value may take. The row *syntax* is grammar and
stays with the spec canon (mcode-grammar.md §7.1, mcd); everything
registered *about a key* is registered here and nowhere else.

The machine mirror is `ATTR_KEYS` in `src/semantic/basic/attr_keys.rs` —
the same rows and columns, one row per key. `scripts/check-attr-keys.py`
reconciles the two row by row and is wired into `scripts/check.sh`, the
pre-commit hook, and CI.

### 8.1 Convention

1. **Open vocabulary, closed semantics.** Any key is legal — an unregistered
   key is not rejected. Name a key the way the datasheet names the
   parameter; registration is never a precondition for writing. But
   everything semantic comes only from the ledger: unit inference, contract
   half, directionality. The compiler never guesses from an unrecognized key
   name, so an unregistered key is legal *and meaningless* (§8.5).
2. **A row exists because a consumer has a question to ask**, never to
   enumerate a device's parameters:

| Consumer | Question | Column read |
|---|---|---|
| ERC / admission (N1, N2, N7) | is this word reserved at attribute positions? | Admission |
| Value semantics (D5, constructor-param type inference) | what quantity does this key carry? | Value |
| Parameter matching (demand ↔ supply) | which half is this key? | Contract |
| Table discipline (HW1, simulation) | is this key a supply voltage? on which face? | Supply + Faces |
| Word admission (`E5360`) | which words may this key's value take? | Words |

3. **A key is a name: exact, full-path, case-sensitive.** `spec.Capacitance`
   is unregistered; `spec.cap` is not `spec.capacitance`. The same spelling
   on two faces is two keys, not one key on two faces: `output` (interface
   body) and `spec.output` (spec table) share a name and register nothing in
   common.

### 8.2 Faces

| Face | Namespace |
|---|---|
| `Body` | component / module / `define` body |
| `Interface` | interface body (`mcode/ifs/*.mc`) |
| `Spec` | `spec` table keys, registered by **full path** (`spec.capacitance`) |
| `PinRow` | pin-row value keys (`volt: 1.2V`) |

### 8.3 Ledger Table

<!-- attr-keys-ledger: reconciled with src/semantic/basic/attr_keys.rs by scripts/check-attr-keys.py -->

| Key | Faces | Value | Contract | Admission | Arity | Supply | Words |
|---|---|---|---|---|---|---|---|
| `this` | Body | - | Plain | reserved | Single | no | - |
| `pins` | Body | - | Plain | reserved | Single | no | - |
| `role` | Body, PinRow | - | Plain | reserved | Single | no | `main`, `quiet`, `protective`, `earth`, `isolated` |
| `func` | Body | - | Plain | reserved | Single | no | - |
| `return` | Body | - | Plain | reserved | Single | no | - |
| `in` | Body | - | Plain | reserved | Single | no | - |
| `out` | Body | - | Plain | reserved | Single | no | - |
| `io` | Body | - | Plain | reserved | Single | no | - |
| `psrc` | Body | - | Plain | reserved | Single | no | - |
| `psnk` | Body | - | Plain | reserved | Single | no | - |
| `psbi` | Body | - | Plain | reserved | Single | no | - |
| `nc` | Body | - | Plain | reserved | Single | no | - |
| `if` | Body | - | Plain | reserved | Single | no | - |
| `else` | Body | - | Plain | reserved | Single | no | - |
| `spec` | Body | - | Plain | general | Single | no | - |
| `name` | Body | Text | Plain | general | Single | no | - |
| `description` | Body | Text | Plain | general | Single | no | - |
| `partno` | Body | Text | Plain | general | Single | no | - |
| `package` | Body | Text | Plain | general | Single | no | - |
| `manufacturer` | Body | Text | Plain | general | Single | no | - |
| `class` | Body, PinRow | - | Plain | general | Single | no | `digital`, `analog`, `radio` |
| `nature` | Body | - | Plain | general | Single | no | `ac`, `dc` |
| `noise` | Body | - | Plain | general | Single | no | `noisy`, `quiet`, `sensitive` |
| `exposed` | Body, PinRow | - | Plain | general | Single | no | `esd_contact`, `esd_air`, `eft`, `surge`, `lightning` |
| `bind_role` | Body | - | Plain | general | Single | no | `main`, `quiet`, `protective`, `earth`, `isolated` |
| `protect` | Body | - | Plain | general | Single | no | `shunt`, `series` |
| `drive` | PinRow | - | Plain | reserved | Single | no | `pp`, `od` |
| `star` | Body | - | Plain | general | Single | no | flag |
| `barrier` | PinRow | Text | Plain | reserved | Single | no | open |
| `bond` | PinRow | Text | Plain | reserved | Single | no | open |
| `pair` | PinRow | Text | Plain | reserved | Single | no | open |
| `voltage` | Body, PinRow, Interface | Quantity(Volt) | Plain | general | Single | yes | - |
| `volt` | PinRow | Quantity(Volt) | Plain | general | Single | yes | - |
| `power` | Body | Quantity(Wat) | Plain | general | Single | yes | - |
| `vcc` | Body | - | Plain | general | Single | yes | - |
| `vdd` | Body | - | Plain | general | Single | yes | - |
| `vss` | Body | - | Plain | general | Single | yes | - |
| `supply` | Body | - | Plain | general | Single | yes | - |
| `operating_voltage` | Body | - | Plain | general | Single | yes | - |
| `input_voltage` | Body | - | Plain | general | Single | yes | - |
| `output_voltage` | Body | - | Plain | general | Single | yes | - |
| `vrange` | Body | - | Plain | general | Single | yes | - |
| `spec.resistance` | Spec | Quantity(Ohm) | Plain | general | Single | no | - |
| `spec.impedance` | Spec | Quantity(Ohm) | Plain | general | Single | no | - |
| `spec.esr` | Spec | Quantity(Ohm) | Plain | general | Single | no | - |
| `spec.voltage` | Spec | Quantity(Volt) | Plain | general | Single | no | - |
| `spec.HBM` | Spec | Quantity(Volt) | Plain | general | Single | no | - |
| `spec.capacitance` | Spec | Quantity(Cap) | Plain | general | Single | no | - |
| `spec.inductance` | Spec | Quantity(Ind) | Plain | general | Single | no | - |
| `spec.current` | Spec | Quantity(Amp) | Plain | general | Single | no | - |
| `spec.rated_current` | Spec | Quantity(Amp) | Plain | general | Single | no | - |
| `spec.sat_current` | Spec | Quantity(Amp) | Plain | general | Single | no | - |
| `spec.ripple_rated` | Spec | Quantity(Amp) | Plain | general | Single | no | - |
| `spec.frequency` | Spec | Quantity(Hz) | Plain | general | Single | no | - |
| `spec.test_frequency` | Spec | Quantity(Hz) | Plain | general | Single | no | - |
| `spec.power` | Spec | Quantity(Wat) | Plain | general | Single | no | - |
| `spec.power_rated` | Spec | Quantity(Wat) | Plain | general | Single | no | - |
| `spec.capacity` | Spec | Quantity(Charge) | Plain | general | Single | no | - |
| `spec.tolerance` | Spec | Quantity(Percent) | Plain | general | Single | no | - |
| `spec.accuracy` | Spec | Quantity(Percent) | Plain | general | Single | no | - |
| `spec.temp_min` | Spec | Quantity(Temp) | Plain | general | Single | no | - |
| `spec.temp_max` | Spec | Quantity(Temp) | Plain | general | Single | no | - |
| `spec.life_hours` | Spec | Quantity(Time) | Plain | general | Single | no | - |
| `spec.length` | Spec | Quantity(Len) | Plain | general | Single | no | - |
| `spec.dielectric` | Spec | Text | Plain | general | Single | no | - |
| `spec.construction` | Spec | Text | Plain | general | Single | no | - |
| `spec.polarized` | Spec | Text | Plain | general | Single | no | - |
| `spec.safety_class` | Spec | Text | Plain | general | Single | no | - |
| `spec.rohs` | Spec | Text | Plain | general | Single | no | - |
| `spec.derating_note` | Spec | Text | Plain | general | Single | no | - |
| `spec.input_req` | Spec | Quantity(Volt) | Demand | general | Single | no | - |
| `spec.output` | Spec | Quantity(Volt) | Supply | general | Single | no | - |
| `receiver` | Interface | Quantity(Volt) | Demand | general | Single | no | - |
| `output` | Interface | Quantity(Volt) | Supply | general | Single | no | - |

Column semantics:

- **Faces** (`AttrFace`) — the namespace the key is written in. One spelling
  may register on several faces (`voltage` on Body, PinRow, and Interface);
  that is still one row, while two keys that share a name across namespaces
  (`output` / `spec.output`) are two rows.
- **Value** (D5) — `Quantity(unit)` / `Text` / `Number`; `-` means no
  registered semantics.
- **Contract** — `Plain` (not matched) / `Demand` (what this part needs from
  the outside) / `Supply` (what this part guarantees to the outside).
  Direction comes only from this column, never from the key name.
  Comparison operators (≥, range containment, enum hits) have no column:
  no consumer asks for them.
- **Admission** — `general` (usable as an ordinary attribute key) /
  `reserved` (the grammar reserves the word at attribute positions, N1).
- **Arity** (U43) — `Single` (at most once per attribute list) / `Set`
  (accumulating). No row declares `Set` today.
- **Supply** (HW1) — `yes` means the key names the supply voltage of the
  thing it is written on. Readers test this column; they never substring-
  match the name (`contains("volt")` is the bug this column exists to
  prevent).
- **Words** — the shape of the key's value vocabulary (§8.4).

### 8.4 Value Words

| Words cell | Meaning |
|---|---|
| `-` | Unregistered: the value is open and not judged. Unregistered ≠ illegal (§8.5 still applies). |
| `` `w1`, `w2`, … `` | Closed set: the value must be one of the listed words, compared exactly, case-insensitive to nothing — `@nature(AC)` errors, keys and words alike are names. |
| `flag` | No-value key: presence activates, a value is an error (bare `@star` is silent, `@star(true)` errors). |
| `open` | Open vocabulary: the value is the author's own identifier (a `@barrier(pri)` group name); every identifier is in the set — spelling is not judged, *absence* is (bare `@barrier` errors `E5360`: naming no group declares nothing, and reads silently as "outside the barrier"). The key and its semantics are registered; consumer admission is a gate-name whitelist — each consuming gate names its key in its design doc (barrier / bond: barrier-design.md §3–§4, mcd doc/attribute). |

**Where word semantics live.** This table registers that a word *exists*;
what a word *means* is canon: intent-design.md §5.2 (mcd, identity axis)
and exposed-protection-design.md §4 (mcd, the `protect` pair). Those canons
refer back to §8.3 — add or remove a word here first. `bind_role` shares
`role`'s word set (its value is the *target* role the parent must bind to;
both read one constant table). The compiler hangs the set on the row's
`vocab` column (`None` / `Words(&[…])` / `Flag` / `Open`); a word outside
the set errors `E5360`; an unregistered key is never judged.

### 8.5 Unregistered Keys — Conservative Defaults

Unregistered ≠ illegal. An unregistered key is legal and carries no
registered meaning, handled by five conservative defaults:

1. Unit normalization (D1) — `60mΩ` carries its own unit and normalizes.
2. Windows take no part in numeric comparison (D2).
3. `_` takes no part (D3).
4. Only equality and in-range tests (D8).
5. **No directionality** — demand/supply judgments come only from the
   ledger.

Comparison and matching capability comes from the **value** (its eight
views: quantity / set / window / bound / record / reference / pending /
map), never from whether the key is registered. A device-specific key that
is not a snake_case full word loses nothing: the ledger neither registers it
nor errors on it.

### 8.6 Adding or Retiring a Key

1. **Ledger first**: add or remove the row in §8.3.
2. **Mirror second**: the same row in `ATTR_KEYS`
   (`src/semantic/basic/attr_keys.rs`).
3. `scripts/check-attr-keys.py` must reconcile with zero drift (wired into
   `scripts/check.sh`, the pre-commit hook, and CI).
4. A closed word's *semantics* go to its canon; its *existence* stays here.
5. If the key falls into one of the six classes of the reading view
   (keys-categorized.md, mcd doc/attribute), add its classification row
   there.

**Openness.** No runtime global configuration — it would make one `.mc`
file mean different things on different machines and drags file IO into the
parse. Extensibility lives in *registration*, not configuration, in layers:
**L0 language core** — this table, read-only; extension means registering
here and in the mirror. **L1 library** and **L2 project** registration
points are reserved, not implemented; when they land, a library or project
may declare its own keys, may not shadow L0, and a conflict is an error,
never a silent win. Retiring a key means downgrading it to unregistered;
since keys in public libraries may still be consumed, a retired row carries
a lifecycle (active / deprecated / retired).

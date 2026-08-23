# Netcheck regression case set

Every rule has at least one known true positive (expect=hit) and one known
false positive (expect=miss). When a rule's criteria change, this set is the
guardrail.

Format:
```
[case] rule=<rule-id> expect=<hit|miss>
module = "<module path>"
reason = "<judgment rationale>"
```

---

## R01 LITERAL_POINT

[case] rule=R01 expect=hit
module = "<top/unassigned>"
reason = "4 unique vectors such as dc{VDD_3V3, GND} have unexpanded brace references, 16 occurrences total"

[case] rule=R01 expect=miss
module = "main.moddcdc"
reason = "no endpoint path inside moddcdc contains braces; after scanning the full table, R01=0"

---

## R02 SHORT_PASSIVE

[case] rule=R02 expect=hit
module = "main"
reason = "both pins of RES_1 sit on net V3V3 (net#101035) — short circuit"

[case] rule=R02 expect=miss
module = "main.modldo"
reason = "the two pins of C_ldo_vin sit on ldo.VIN.Vin and GND respectively — not shorted"

---

## R03 SHORT_RAIL

[case] rule=R03 expect=hit
module = "(any module)"
reason = "if one net contains both VDD and GND endpoints → power-ground short"

[case] rule=R03 expect=miss
module = "main"
reason = "V3V3 net only has power endpoints (V3V3, VCC1V2, VDD_3V3), no ground endpoint"

---

## R03a RAIL_ALIAS

[case] rule=R03a expect=hit
module = "main"
reason = "V3V3 net contains three power-domain aliases {V3V3, VCC1V2, VDD_3V3}"

[case] rule=R03a expect=miss
module = "main.moddcdc"
reason = "the GND net inside moddcdc only holds the GND name — no alias conflict"

---

## R04 SHORT_LANE

[case] rule=R04 expect=hit
module = "(any module)"
reason = "if two members of the same bus (e.g. MIC.P and MIC.N) land on the same net"

[case] rule=R04 expect=miss
module = "main.mic"
reason = "the members of the MIC bus land on distinct nets"

---

## R07 GHOST_INSTANCE

[case] rule=R07 expect=hit
module = "(any module)"
reason = "if the owner of a net endpoint cannot be resolved to a registered Component/Module/Bus/Port → ghost"

[case] rule=R07 expect=miss
module = "main"
reason = "every net endpoint owner resolves to a registered instance (R07=0)"

---

## R09 FLOATING_PWR_PIN

[case] rule=R09 expect=hit
module = "main.mcu513"
reason = "21 power/ground pins of uC are unconnected"

[case] rule=R09 expect=miss
module = "main.modldo"
reason = "both ldo VIN.Vin and VOUT.Vout are connected"

---

## R11 SPLIT_RAIL

★ P0.5-6: report only same-module rail splits, scoped per module.
Cross-module rails merge through port union and are no longer reported.
Note: the current netlist (after the NC arity fix) has no same-module rail
split; R11=0 is the normal state.

[case] rule=R11 expect=hit
module = "main.modldo"
reason = "if GND and vin.GND inside modldo are two disconnected nets that both carry the GND identity → same-module GND split into multiple nets"

[case] rule=R11 expect=miss
module = "main.moddcdc"
reason = "GND and main::GND are bound together via the [VDD_3V3,GND] port, merged into one group after union — not reported"

[case] rule=R11 expect=miss
module = "main.mcu513"
reason = "mcu513 has only one GND net; no same-module split (P0.5-6 scopes per module)"

---

## R14 ORPHAN_INSTANCE

[case] rule=R14 expect=hit
module = "main.mcu513"
reason = "5 instances such as C_dac_dc_block are registered but appear in no net"

[case] rule=R14 expect=miss
module = "main.modldo"
reason = "ldo, C_ldo_vin, C_ldo_vout are all present in nets"

---

## R15 SYNTHETIC_PIN

[case] rule=R15 expect=hit
module = "<top>"
reason = "the viz layer detects a synthetic terminal (pin_id does not belong to any real pin), produced by port scalar/member handling"

[case] rule=R15 expect=miss
module = "<top>"
reason = "if the viz layer emits no GHOST_PIN output, R15=0"

# Netcheck 回归用例集

每条规则至少一个已知真例（expect=hit）和一个已知误报例（expect=miss）。
规则改判据时，这个集合就是护栏。

格式：
```
[case] rule=<规则号> expect=<hit|miss>
module = "<模块路径>"
reason = "<判断理由>"
```

---

## R01 LITERAL_POINT

[case] rule=R01 expect=hit
module = "<顶层/未归属>"
reason = "dc{VDD_3V3, GND} 等 4 个唯一向量的花括号引用未展开，共 16 次出现"

[case] rule=R01 expect=miss
module = "main.moddcdc"
reason = "moddcdc 内部所有端点路径不含花括号，netcheck 扫描全表后 R01=0"

---

## R02 SHORT_PASSIVE

[case] rule=R02 expect=hit
module = "main"
reason = "RES_1 两脚都在网 V3V3 (net#101035) —— 短路"

[case] rule=R02 expect=miss
module = "main.modldo"
reason = "C_ldo_vin 两脚分别在 ldo.VIN.Vin 和 GND 网，未短路"

---

## R03 SHORT_RAIL

[case] rule=R03 expect=hit
module = "（任意模块）"
reason = "若一张网同时含 VDD 和 GND 端点 → 电源-地短路"

[case] rule=R03 expect=miss
module = "main"
reason = "V3V3 网只含电源端点（V3V3, VCC1V2, VDD_3V3），无地端点"

---

## R03a RAIL_ALIAS

[case] rule=R03a expect=hit
module = "main"
reason = "V3V3 网同时含 {V3V3, VCC1V2, VDD_3V3} 三个电源域别名"

[case] rule=R03a expect=miss
module = "main.moddcdc"
reason = "moddcdc 内部 GND 网只含 GND 名，无别名冲突"

---

## R04 SHORT_LANE

[case] rule=R04 expect=hit
module = "（任意模块）"
reason = "若同一总线的两个成员（如 MIC.P 和 MIC.N）落在同一张网"

[case] rule=R04 expect=miss
module = "main.mic"
reason = "MIC 总线的各成员落在不同网"

---

## R07 GHOST_INSTANCE

[case] rule=R07 expect=hit
module = "（任意模块）"
reason = "若网内端点 owner 解析不到已注册 Component/Module/Bus/Port → ghost"

[case] rule=R07 expect=miss
module = "main"
reason = "所有网内端点 owner 均可解析到已注册实例（R07=0）"

---

## R09 FLOATING_PWR_PIN

[case] rule=R09 expect=hit
module = "main.mcu513"
reason = "uC 的电源/地管脚 21 未连接"

[case] rule=R09 expect=miss
module = "main.modldo"
reason = "ldo 的 VIN.Vin 和 VOUT.Vout 均已连接"

---

## R11 SPLIT_RAIL

[case] rule=R11 expect=hit
module = "main.mcu513"
reason = "GND 网与 main::GND 互不相连，mcu513 的 GND 端口未在父层绑定"

[case] rule=R11 expect=miss
module = "main.moddcdc"
reason = "GND 与 main::GND 经端口 [VDD_3V3,GND] 绑定连通，union 后同组"

---

## R14 ORPHAN_INSTANCE

[case] rule=R14 expect=hit
module = "main.mcu513"
reason = "C_dac_dc_block 等 5 个实例注册了但不在任何网里"

[case] rule=R14 expect=miss
module = "main.modldo"
reason = "ldo、C_ldo_vin、C_ldo_vout 均在网中"
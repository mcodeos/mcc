---
name: part-binding
description: Bottom-up playbook for binding a real part to an abstract base-library
  component (`component X : Base` variant inheritance). Use when migrating a
  project-local component to inherit from an mcode/mclibs abstract shape, when
  asked to rewrite a part into the bound/inherited form or to follow the USB
  worked template, or when auditing an abstract base against
  a verified real device before binding.
---

# Part binding — the USB worked template

The canon (rationale, worked example with readouts, naming law) lives in
`mcd/doc/library/part-binding-playbook.md`. Read it before the first run of
any step you are unsure about. This file is the operational checklist.

## Direction

Bottom-up, one seam at a time. Do NOT touch the project file before steps 1–3
are green. The verified real device (the project's existing component) is the
ground truth for pinout; the library defers to it.

## Checklist

1. **Interface first** — `mcode/ifs/`: does an interface exist for the bus the
   part speaks? Check its pin table AND its name table against each other
   (signal order vs display names have been known to contradict — USB.MINIB
   disease). File a ledger finding, do not silently pick a side.
2. **Abstract shape** — find the base component (`mcode/conn/`, `mclibs/`).
   Inheritance requires an **abstract** base (`:` targets abstract component
   only; `abstract` + `:` is itself an error). If the shape is concrete,
   promote it (`abstract component ...`) after listing direct-instantiation
   consumers: each one gains the by-design W `abstract-unselected` (plan §6.1
   posture — report it, don't fix it).
3. **Pin-by-pin reconciliation** — verified device vs base pins: numbers,
   signals, DC pairs, direction words, row attrs (`@exposed`, `@class`).
   Every difference gets a verdict: *library annotation that is true* (the
   bind will surface new gate readouts — accepted, never silenced) or *library
   defect* (fix the library, not the project). Also reconcile any mcpub sample
   of the same part — pin-order conflicts between libs go to the ledger.
4. **Rewrite the project part as a variant** —
   `component <FAMILY>.<PARTNO_UNDERSCORED> : <BASE>` (naming law:
   `LDO.SGM2019_33YN5G_TR` shape). **Delete the `pins` block entirely** — the
   data lock (`VARIANT_REDECLARES_PINS_PARAMS_FUNCS`) forbids redeclaring
   pins/params/funcs; the base's pins are authoritative. Keep attr overrides
   (partno/package/...) and `layout`.
5. **Fix addressing** — interface-adopted pins address through the group name
   (`usbsock.USB.VBUS`); plain pins stay flat (`usbsock.6`, `SHIELD3`).
   Grep the module bodies for flat references to adopted pins.
6. **A/B acceptance** — same binary, before/after `mcc build --local`; compare
   as sorted multisets. Every diff line gets one of three verdicts: expected
   consequence / same finding moved lines / substantive. Net tables must match
   member-for-member. New gate findings: attribute to a cause, never silence.
7. **Installed copy surgery** — after a library edit, copy ONLY the edited
   file into `~/.mcode/<lib>/` (incremental). Never whole-library `cp.sh` over
   a shared install: it eats concurrent sessions' installed state.
8. **Ledger + batch** — claim the batch number as max(BUILDNr, ledger max)+1
   after scanning all repos AND the ledger; one commit per repo, ledger entry
   in mcd last; bump mcd BUILDNr. Connected findings become new ledger items,
   not footnotes.

## Traps seen in the worked run

- `usbsock.GND` silently changed meaning after binding (base names pin 5 via
  the interface group) — address it as `usbsock.USB.GND`.
- Inherited `@exposed` surfaces `exposed-net-no-clamp` (PWR-6) on boards that
  tie shells to GND without a clamp — that is the gate doing its job; the
  design-side ruling (add clamp vs accept) belongs to the user, record the +N.
- Two sessions can claim the same batch number in one day; if the collision is
  already pushed, do not rewrite history — note it in the ledger and take the
  next number.

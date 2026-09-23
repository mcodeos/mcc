// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! CSS theme (light / dark, automatically follows system)
//!
//! Extracted from the old `viz/template/legacy.rs` `<style>` block.
//! Visual style fully preserved; no color values changed.

/// Returns the CSS to be embedded in `<style>...</style>`
pub fn css() -> &'static str {
    r##":root {
  --bg: #ffffff;
  --fg: #1a1a1a;
  --bg-panel: #f8f8f8;
  --border: #e0e0e0;
  --highlight: #ffeaa7;
  --link: #3b82f6;
}
@media (prefers-color-scheme: dark) {
  :root {
    --bg: #1a1a2e;
    --fg: #e0e0e0;
    --bg-panel: #16213e;
    --border: #334155;
    --highlight: #854d0e;
    --link: #60a5fa;
  }
  svg text { fill: #ccc !important; }
  svg rect { stroke: #888 !important; }
  svg line { stroke: #888 !important; }
  svg path { stroke: #888 !important; }
  .comp.multi-pin rect { fill: #2a2548 !important; stroke: #7F77DD !important; }
  .comp.multi-pin text { fill: #b0a8e8 !important; }
  .comp.two-pin rect { stroke: #999 !important; }
  .edge.bus path { stroke: #d4a017 !important; }
  .edge.bus line { stroke: #d4a017 !important; }
  .edge.bus text { fill: #d4a017 !important; }
  .comp.power-label line { stroke: #8bc34a !important; }
  .comp.power-label text { fill: #8bc34a !important; }
}
* { box-sizing: border-box; margin: 0; padding: 0; }
body {
  background: var(--bg);
  color: var(--fg);
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
  display: flex;
  flex-direction: column;
  height: 100vh;
  overflow: hidden;
}
#breadcrumb {
  padding: 8px 16px;
  border-bottom: 1px solid var(--border);
  font-size: 13px;
  display: flex;
  align-items: center;
  gap: 4px;
  flex-shrink: 0;
}
#breadcrumb span {
  cursor: pointer;
  color: var(--link);
}
#breadcrumb span:hover {
  text-decoration: underline;
}
#breadcrumb .sep {
  color: #888;
  cursor: default;
}
#breadcrumb .current {
  color: var(--fg);
  cursor: default;
  font-weight: 500;
}
#breadcrumb .back-btn {
  margin-right: 12px;
  padding: 2px 8px;
  border: 1px solid var(--border);
  border-radius: 3px;
  cursor: pointer;
  color: var(--fg);
  user-select: none;
}
#breadcrumb .back-btn.disabled {
  opacity: 0.3;
  cursor: default;
}
#breadcrumb .back-btn:not(.disabled):hover {
  background: var(--bg-panel);
}
#main-container {
  display: flex;
  flex: 1;
  overflow: hidden;
  position: relative;
}
#canvas {
  flex: 1;
  /* The canvas is a camera viewport, not a scroll container: panning moves
     the translate + scale transform on #zoom-pane (interact.rs), never a
     scrollbar. */
  overflow: hidden;
  padding: 16px;
}
/* The drawing is wrapped in #zoom-pane at its zoom=1 size; the camera is a
   translate + scale transform on the pane. The SVG itself is never resized. */
#zoom-pane {
  transform-origin: 0 0;
  will-change: transform;
  cursor: grab;
}
#zoom-pane.dragging {
  cursor: grabbing;
}
#zoom-pane svg {
  display: block;
  width: 100%;
  height: 100%;
}
#zoom-control {
  position: absolute;
  top: 10px;
  right: 10px;
  z-index: 10;
  display: flex;
  align-items: center;
  gap: 2px;
  padding: 2px 4px;
  background: var(--bg-panel);
  border: 1px solid var(--border);
  border-radius: 4px;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.15);
  user-select: none;
}
#zoom-control button {
  border: none;
  background: transparent;
  color: var(--fg);
  font-size: 13px;
  width: 24px;
  height: 24px;
  line-height: 24px;
  text-align: center;
  cursor: pointer;
  border-radius: 3px;
}
#zoom-control button:hover {
  background: rgba(128, 128, 128, 0.2);
}
#zoom-control button:disabled {
  opacity: 0.35;
  cursor: default;
}
#zoom-control button:disabled:hover {
  background: transparent;
}
#zoom-level {
  font-size: 11px;
  min-width: 44px;
  text-align: center;
  color: var(--fg);
  font-variant-numeric: tabular-nums;
}
#stats {
  padding: 6px 16px;
  border-top: 1px solid var(--border);
  font-size: 12px;
  color: #888;
  display: flex;
  gap: 16px;
  flex-shrink: 0;
}
/* Source-navigation hot zone
   ------------------------------------------------------------------------
   A stamped element's clickable area is its whole drawn extent, not only the
   strokes it paints. An SVG group has no area of its own, so on the default
   `visiblePainted` a click only lands on the ink: a lead is a 1.2px hairline
   plus 8-10px glyphs, and a lead drawn without labels (a two-pin part's
   marker-only pin) leaves nothing at all to hit. `bounding-box` makes the
   group's own box the target instead.

   Measured in Chromium over the hbl fixture, all 7 layers, 58 leads x 49
   sample points each (scripts aside, the numbers are reproducible with
   `document.elementsFromPoint`): points inside a lead's box that reached that
   lead go 53.6% -> 96.6%, points reaching no stamped element at all (dead
   zone) 535 -> 0. Spill onto a *different* element stays 0 on every layer
   except SPK, whose 3.4% is a pre-existing pin collision — two leads drawn at
   the same coordinate — not something a hit box can cause or cure.

   This widening is deliberately free of the render golden: the rule lives in
   the page's CSS, so no layer's `svg` string (what `VizDocument::to_json()`
   and therefore tests/golden/hbl.golden.json record) changes.

   Firefox has never implemented `bounding-box`; there the declaration is
   dropped and the page keeps the painted-only behaviour it had before. */
#canvas [data-src-uri] {
  pointer-events: bounding-box;
}
/* Source-navigation affordance (design §4 D5)
   ------------------------------------------------------------------------
   Nothing on the page says a box or pin *can* be jumped from, so the gesture
   is undiscoverable until someone happens to hold the modifier. Two cues, both
   driven from here and the page JS, so neither touches the render golden:

   1. a one-line hint in the status bar, present only when the current layer
      actually holds something jumpable (see `updateHint` in the page JS);
   2. this outline, shown while the modifier is held (`body.nav-armed`).

   The outline traces the element's box, which is the region `bounding-box`
   above makes clickable — the cue shows the target rather than leaving the
   user to aim at a 1.2px lead. The offset is *negative* on purpose: the
   outline is painted inside the box, so every pixel it lights up is a pixel
   that a modifier-click would actually hit. A positive offset would draw a
   frame just outside the hit zone.

   Measured in Chromium (screenshot pixel diff, so "no visible effect" is a
   measurement and not an assumption): `outline` on the stamped groups paints
   (~31k px changed over the hbl root layer); so does `filter: drop-shadow`.
   `background` and a bare `cursor` change nothing. drop-shadow lost out
   because it glows around the *ink* — it would advertise the hairline as the
   target, which is exactly the misreading the hot zone above exists to fix.

   Firefox never implemented `bounding-box`, so there the drawn box is wider
   than the hit zone (ink only). The outline still marks *which* elements can
   be jumped, which is what the cue is for; it just cannot promise the whole
   frame. */
body.nav-armed #canvas [data-src-uri] {
  outline: 2px solid var(--link);
  outline-offset: -2px;
  cursor: pointer;
}
/* S4: the host commits a viewframe onto one instance (`viz:highlight`) —
   the amber halo marks which box the workbench is looking at. */
#canvas g.inst-focused {
  filter: drop-shadow(0 0 6px #f59e0b);
}
/* Hover cards (mouseover): one floating panel, four card kinds, all data
   read from the embedded LAYOUT manifest — never from the SVG. */
#viz-card {
  position: fixed;
  display: none;
  z-index: 999;
  max-width: 420px;
  max-height: 60vh;
  overflow: auto;
  background: var(--panel, #fff);
  color: var(--text, #222);
  border: 1px solid var(--link, #1565c0);
  border-radius: 6px;
  padding: 8px 10px;
  font-size: 12px;
  line-height: 1.45;
  box-shadow: 0 4px 14px rgba(0,0,0,0.25);
}
#viz-card .vc-title { font-weight: 700; font-size: 13px; margin-bottom: 2px; }
#viz-card .vc-sub { color: var(--link, #1565c0); margin-bottom: 4px; }
#viz-card .vc-dnp { color: #b00; font-weight: 700; }
#viz-card table { border-collapse: collapse; margin: 4px 0; width: 100%; }
#viz-card td, #viz-card th { padding: 1px 6px; text-align: left; border-bottom: 1px solid rgba(128,128,128,0.25); }
#viz-card .vc-nets { margin-top: 4px; word-break: break-all; }
#viz-card .vc-jump { color: var(--link, #1565c0); cursor: pointer; margin-top: 4px; text-decoration: underline; }
#viz-card tr[data-jump] { cursor: pointer; }
#viz-card tr[data-jump]:hover { background: rgba(21,101,192,0.12); }
/* Whole-net highlight (viz:highlightNet): wires carry data-net directly,
   pins and boxes sit in groups whose stroke/fill follows. */
#canvas .net-focused, #canvas g.net-focused * {
  stroke: #f59e0b !important;
}
#canvas circle.net-focused {
  fill: #f59e0b !important;
}
/* Hint sits at the far end of the status row, opposite the layer stats. */
#stats .hint {
  margin-left: auto;
  color: var(--link);
}"##
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The lead hit zone. Two halves have to stay in step: the selector must key
    /// off the same `data-src-uri` stamp the interaction JS reads (a different
    /// selector silently leaves every lead on painted-only hit testing, which is
    /// the 53.6% -> 96.6% regression above), and the value must stay
    /// `bounding-box` (any other `pointer-events` value either does nothing or
    /// makes the group unhittable entirely).
    #[test]
    fn stamped_elements_hit_their_whole_box() {
        let css = css();
        assert!(
            css.contains("[data-src-uri]"),
            "hot zone no longer keys off the source stamp"
        );
        assert!(
            css.contains("pointer-events: bounding-box"),
            "hot zone no longer widens past the painted strokes"
        );
    }

    /// The D5 affordance. Three things have to stay in step: it must key off the
    /// same stamp the hot zone and the page JS use (any other selector outlines
    /// nothing while still reading as plausible), it must be scoped to
    /// `body.nav-armed` (unscoped, every page would ship permanently outlined),
    /// and the offset must stay negative so the frame is painted *inside* the
    /// box the click would land in.
    #[test]
    fn armed_modifier_outlines_the_same_stamp() {
        let css = css();
        assert!(
            css.contains("body.nav-armed #canvas [data-src-uri]"),
            "affordance no longer keys off the source stamp while armed"
        );
        assert!(
            css.contains("outline-offset: -2px"),
            "the frame now paints outside the clickable box"
        );
        assert!(
            css.contains("#stats .hint"),
            "the hint no longer sits in the status row"
        );
    }
}

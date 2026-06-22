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
}
#canvas {
  flex: 1;
  overflow: auto;
  padding: 16px;
  display: flex;
  justify-content: center;
}
#canvas svg {
  max-width: 100%;
  height: auto;
}
#stats {
  padding: 6px 16px;
  border-top: 1px solid var(--border);
  font-size: 12px;
  color: #888;
  display: flex;
  gap: 16px;
  flex-shrink: 0;
}"##
}

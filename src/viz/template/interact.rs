// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ Actually working expand / collapse / navigation JS (fixed version)
//!
//! ## Fixes (vs P2 original)
//! 1. **DOC integrity self-check**: init() first checks whether DOC is parsed correctly
//! 2. **Type-safe layer lookup**: tries both numeric and string keys
//! 3. **Fallback to first layer**: if root_bid cannot be found, use the first in `layers`
//! 4. **Detailed error diagnostics**: no longer just saying "not found", lists all available bids
//! 5. **window.DOC exposure**: convenient for users to run `console.log(DOC)` to debug

pub fn js() -> &'static str {
    r##"
'use strict';

// DOC self-check
// If DOC is undefined or corrupted, give a clear diagnosis instead of an opaque crash
if (typeof DOC === 'undefined') {
    document.getElementById('canvas').innerHTML =
        '<div style="padding:40px;color:red;font-family:monospace">' +
        '<h2>Critical: DOC global is undefined</h2>' +
        '<p>The <code>const DOC = {...}</code> declaration in the page header did not execute.</p>' +
        '<p>Likely cause: the embedded JSON contains a sequence the HTML parser ' +
        'misinterprets as an HTML tag (e.g. <code>&lt;/script&gt;</code> inside an SVG).</p>' +
        '<p>Open browser dev tools console for details.</p>' +
        '</div>';
    throw new Error('DOC undefined');
}

// Expose to global for easy console debugging
window.DOC = DOC;

// Check DOC integrity
if (!DOC.layers || typeof DOC.layers !== 'object') {
    document.getElementById('canvas').innerHTML =
        '<div style="padding:40px;color:red;font-family:monospace">' +
        '<h2>DOC.layers is invalid</h2>' +
        '<p>DOC was parsed but missing the <code>layers</code> object.</p>' +
        '<pre>' + escapeHtml(JSON.stringify(DOC, null, 2).slice(0, 500)) + '...</pre>' +
        '</div>';
    throw new Error('DOC.layers invalid');
}

// Output diagnostic information to console (even when everything is normal, it helps users verify)
console.group('[McVec Viewer] DOC loaded');
console.log('root_bid:', DOC.root_bid, '(type:', typeof DOC.root_bid, ')');
console.log('root_name:', DOC.root_name);
console.log('layers count:', Object.keys(DOC.layers).length);
console.log('layer bids:', Object.keys(DOC.layers));
console.groupEnd();

// State
const navStack = [];
let currentBid = DOC.root_bid;

// Zoom control
// Zoom is implemented by wrapping the current <svg> in a #zoom-pane and
// sizing the pane to (canvas-width × zoom). The SVG has a viewBox and fills
// the pane, so it is re-rendered at the target size (vector-crisp) rather
// than bitmap-scaled. Scrolling pans the zoomed content via #canvas overflow.
const ZOOM_MIN = 0.25;
const ZOOM_MAX = 8;
const ZOOM_STEP = 1.25;
let zoomLevel = 1;
let zoomAspect = 1; // current SVG viewBox aspect ratio (w / h)

// Ensure the current <svg> lives inside a #zoom-pane. Must re-wrap after
// every innerHTML replacement in init() / switchToLayer().
function ensureZoomPane() {
    const canvas = document.getElementById('canvas');
    const svg = canvas.querySelector('svg');
    if (!svg) return null;

    let pane = document.getElementById('zoom-pane');
    if (!pane) {
        pane = document.createElement('div');
        pane.id = 'zoom-pane';
        canvas.appendChild(pane);
        pane.appendChild(svg);
    }

    // Re-read the viewBox aspect ratio in case this layer's SVG differs.
    const vb = svg.getAttribute('viewBox');
    if (vb) {
        const parts = vb.trim().split(/[\s,]+/).map(Number);
        if (parts.length === 4 && parts[2] > 0 && parts[3] > 0) {
            zoomAspect = parts[2] / parts[3];
        }
    }
    return pane;
}

function applyZoom(scale) {
    zoomLevel = Math.max(ZOOM_MIN, Math.min(ZOOM_MAX, scale));
    const pane = ensureZoomPane();
    if (!pane) return;

    const canvas = document.getElementById('canvas');
    // Base width = canvas content box (clientWidth minus padding), so zoom=1
    // fills the canvas exactly with no scrollbar.
    const cs = getComputedStyle(canvas);
    const padX = (parseFloat(cs.paddingLeft) || 0) + (parseFloat(cs.paddingRight) || 0);
    const baseW = Math.max(1, canvas.clientWidth - padX);
    const paneW = Math.max(1, baseW * zoomLevel);
    pane.style.width = paneW + 'px';
    pane.style.height = (paneW / zoomAspect) + 'px';

    const label = document.getElementById('zoom-level');
    if (label) label.textContent = Math.round(zoomLevel * 100) + '%';
    const zin = document.getElementById('zoom-in');
    const zout = document.getElementById('zoom-out');
    if (zin) zin.disabled = zoomLevel >= ZOOM_MAX;
    if (zout) zout.disabled = zoomLevel <= ZOOM_MIN;
}

function zoomIn()  { applyZoom(zoomLevel * ZOOM_STEP); }
function zoomOut() { applyZoom(zoomLevel / ZOOM_STEP); }
function zoomReset() { applyZoom(1); }

// Smart layer lookup: try number / string / fallback
function findLayer(bid) {
    if (bid === null || bid === undefined) return null;

    // 1) Direct try (JS will auto-convert numeric keys to strings, usually works here)
    let layer = DOC.layers[bid];
    if (layer) return layer;

    // 2) Explicitly convert to string and try (large i64 may lose precision, string is safe)
    layer = DOC.layers[String(bid)];
    if (layer) return layer;

    // 3) Explicitly convert to number and try (reverse, in case the key is a numeric literal)
    layer = DOC.layers[Number(bid)];
    if (layer) return layer;

    return null;
}

// Initialization
function init() {
    let root = findLayer(DOC.root_bid);
    let usedFallback = false;
    let fallbackBid = DOC.root_bid;

    // Fallback: root not found → use the first available layer
    if (!root) {
        const keys = Object.keys(DOC.layers);
        if (keys.length === 0) {
            showFatalError(
                'Document has no layers',
                'DOC.layers is empty. The Rust pipeline produced a VizDocument with zero layers. ' +
                'This usually means the input graph had no boxes, or builder/promote dropped everything. ' +
                'Run with MC_VIZ_DUMP=1 to debug.'
            );
            return;
        }
        usedFallback = true;
        fallbackBid = keys[0];
        root = DOC.layers[fallbackBid];
        currentBid = parseBid(fallbackBid);
        console.warn(
            '[McVec] Root layer #' + DOC.root_bid + ' not found, ' +
            'falling back to first available: #' + fallbackBid
        );
    }

    document.getElementById('canvas').innerHTML = root.svg;

    if (usedFallback) {
        // Prominent notice
        const banner = document.createElement('div');
        banner.style.cssText =
            'background:#fff3cd;border-bottom:1px solid #f0ad4e;padding:6px 12px;' +
            'font-size:12px;color:#856404';
        banner.innerHTML =
            '⚠ Root layer <code>#' + escapeHtml(String(DOC.root_bid)) +
            '</code> not in document; showing fallback <code>#' + escapeHtml(String(fallbackBid)) +
            '</code>. Available bids: <code>' +
            escapeHtml(Object.keys(DOC.layers).join(', ')) + '</code>';
        document.body.insertBefore(banner, document.body.firstChild);
    }

    updateBreadcrumb();
    updateStats();
    applyZoom(zoomLevel);
}

function parseBid(s) {
    const n = Number(s);
    return Number.isFinite(n) ? n : s;
}

function showFatalError(title, msg) {
    document.getElementById('canvas').innerHTML =
        '<div style="padding:40px;color:#c00;font-family:monospace;max-width:800px">' +
        '<h2>' + escapeHtml(title) + '</h2>' +
        '<p>' + escapeHtml(msg) + '</p>' +
        '<p>Available DOC keys: <code>' +
        escapeHtml(Object.keys(DOC).join(', ')) + '</code></p>' +
        '<p>Layer bids: <code>' +
        escapeHtml(Object.keys(DOC.layers || {}).join(', ') || '(empty)') + '</code></p>' +
        '</div>';
}

// Switch layer
function switchToLayer(bid) {
    const layer = findLayer(bid);
    if (!layer) {
        console.warn('[McVec] Layer not found: bid=' + bid +
                     '. Available: ' + Object.keys(DOC.layers).join(', '));
        return false;
    }
    currentBid = bid;
    document.getElementById('canvas').innerHTML = layer.svg;
    updateBreadcrumb();
    updateStats();
    applyZoom(zoomLevel);
    return true;
}

// Expand sub-module (SVG onclick="expandSubModule(<bid>)")
function expandSubModule(bid) {
    const layer = findLayer(bid);
    if (!layer) {
        console.warn('[McVec] Cannot expand: layer #' + bid +
                     ' not in document. Available: ' + Object.keys(DOC.layers).join(', '));
        return;
    }
    navStack.push(currentBid);
    switchToLayer(bid);
}

function goBack() {
    if (navStack.length === 0) return;
    const prev = navStack.pop();
    switchToLayer(prev);
}

function goToLayer(bid) {
    const idx = navStack.indexOf(bid);
    if (idx >= 0) {
        navStack.length = idx;
        switchToLayer(bid);
        return;
    }
    navStack.length = 0;
    switchToLayer(bid);
}

// Breadcrumb
function updateBreadcrumb() {
    const bc = document.getElementById('breadcrumb');
    if (!bc) return;

    let html = '';

    if (navStack.length > 0) {
        html += '<span class="back-btn" onclick="goBack()">◀ Back</span>';
    } else {
        html += '<span class="back-btn disabled">◀ Back</span>';
    }

    for (const bid of navStack) {
        const lyr = findLayer(bid);
        const name = lyr ? lyr.name : ('#' + bid);
        html += '<span onclick="goToLayer(' + bid + ')">' + escapeHtml(name) + '</span>';
        html += '<span class="sep"> ▸ </span>';
    }

    const cur = findLayer(currentBid);
    const curName = cur ? cur.name : ('#' + currentBid);
    html += '<span class="current">' + escapeHtml(curName) + '</span>';

    bc.innerHTML = html;
}

// Bottom stats
function updateStats() {
    const stats = document.getElementById('stats');
    if (!stats) return;

    const cur = findLayer(currentBid);
    if (!cur) {
        stats.innerHTML = '';
        return;
    }
    const subs = cur.clickable_subs ? cur.clickable_subs.length : 0;
    const svgKb = (cur.svg.length / 1024).toFixed(1);
    stats.innerHTML =
        '<span>Layer: ' + escapeHtml(cur.name) + ' (#' + cur.bid + ')</span>' +
        '<span>Sub-modules: ' + subs + '</span>' +
        '<span>Total layers: ' + Object.keys(DOC.layers).length + '</span>' +
        '<span>Current SVG: ' + svgKb + ' KB</span>' +
        '<span class="hint" id="hint"></span>';
    updateHint();
}

function escapeHtml(text) {
    if (text === null || text === undefined) return '';
    return String(text)
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;');
}

// Zoom gesture / button wiring
// The canvas element itself persists across layer switches (only its
// innerHTML is replaced), so listeners attached here survive navigation.
const zoomCanvas = document.getElementById('canvas');
zoomCanvas.addEventListener('wheel', function (e) {
    // Trackpad pinch and Ctrl/Cmd + wheel both zoom. Chromium reports a
    // pinch as wheel events with ctrlKey=true (on macOS metaKey is also set);
    // the deltaY sign selects the direction.
    if (e.ctrlKey || e.metaKey) {
        e.preventDefault();
        applyZoom(zoomLevel * (e.deltaY < 0 ? ZOOM_STEP : 1 / ZOOM_STEP));
    }
}, { passive: false });

document.getElementById('zoom-in').addEventListener('click', zoomIn);
document.getElementById('zoom-out').addEventListener('click', zoomOut);
document.getElementById('zoom-reset').addEventListener('click', zoomReset);
window.addEventListener('resize', function () { applyZoom(zoomLevel); });

// Source navigation
// The Rust renderer stamps every box and pin that has a real source position
// with data-src-uri + data-src-offset (a byte offset into the .mc file). A
// modifier-click hands that coordinate to the host, which opens the file and
// reveals it.
//
// Gesture: Cmd/Ctrl + click only. A plain click is never intercepted — a
// sub-module box still drills down through its own onclick="expandSubModule",
// a component box still does nothing — so the existing interactions are
// untouched. The listener is therefore registered in the *capture* phase: a
// bubble-phase one would run too late, since the box's inline handler fires on
// the way up through the inner <g> before the event reaches #canvas.
//
// With no host — a standalone circuit.html opened in a browser — a link is
// followed instead, when the writer left one. `mcc build --viz` converts each
// byte offset into a `vscode://file/<abs>:<line>:<col>` and stamps it as
// data-src-vscode (viz::sourcelink); line/column need the file's contents, so
// only the writing side can produce them. A page with neither a host nor a link
// — an older artifact, or a source file that has since moved — shows and copies
// the raw coordinate rather than doing nothing at all (design §3.4).
const mcodeHost = (typeof acquireVsCodeApi === 'function') ? acquireVsCodeApi() : null;
// S4: inside a plain <iframe> (the mcide workbench) the host is the parent
// window — same postMessage contract, no VS Code API. Standalone (no parent)
// stays hostless: the click falls back to link/copy.
const hostTarget = mcodeHost || ((window.parent && window.parent !== window) ? window.parent : null);

// Discoverability (design §4 D5). Holding the modifier is the whole gesture and
// nothing announces it, so the status line states it, and the wording follows
// what a click would really do on *this* page: a host to open the file, a
// stamped link to hand to the OS, or nothing but the coordinate to copy.
const IS_MAC = /Mac|iPhone|iPad/.test(navigator.platform || '');
const MOD_LABEL = IS_MAC ? '⌘' : 'Ctrl';

function hintText() {
    const gesture = MOD_LABEL + ' + click a box or pin: ';
    if (hostTarget) return gesture + 'open its source';
    if (document.querySelector('#canvas [data-src-vscode]')) {
        return gesture + 'open its source in VS Code';
    }
    return gesture + 'copy its source coordinate';
}

// Empty when the layer on screen holds nothing jumpable — a hint for a gesture
// that would do nothing is worse than no hint.
function updateHint() {
    const el = document.getElementById('hint');
    if (!el) return;
    const jumpable = document.querySelectorAll('#canvas [data-src-uri]').length;
    el.textContent = jumpable ? hintText() : '';
}

// While the modifier is down, the CSS outlines every element the gesture would
// act on (theme.rs). blur is disarmed too: a modifier held as the window loses
// focus never delivers its keyup, and a highlight stuck on is worse than none.
function setNavArmed(on) {
    document.body.classList.toggle('nav-armed', on);
}
window.addEventListener('keydown', function (e) {
    if (e.key === 'Meta' || e.key === 'Control') setNavArmed(true);
});
window.addEventListener('keyup', function (e) {
    if (e.key === 'Meta' || e.key === 'Control') setNavArmed(false);
});
window.addEventListener('blur', function () { setNavArmed(false); });

function sourceCoordOf(ev) {
    if (!(ev.metaKey || ev.ctrlKey)) return null;
    const el = ev.target && ev.target.closest ? ev.target.closest('[data-src-uri]') : null;
    if (!el) return null;
    const uri = el.getAttribute('data-src-uri');
    const offset = Number(el.getAttribute('data-src-offset'));
    if (!uri || !Number.isFinite(offset)) return null;
    return { uri: uri, offset: offset, link: el.getAttribute('data-src-vscode') };
}

function copySourceCoord(coord) {
    const text = coord.uri + ':' + coord.offset;
    if (navigator.clipboard && navigator.clipboard.writeText) {
        navigator.clipboard.writeText(text).catch(function () {});
    }
    const note = document.createElement('div');
    note.textContent = 'source coordinate copied: ' + text;
    note.style.cssText =
        'position:fixed;left:50%;bottom:24px;transform:translateX(-50%);' +
        'background:#333;color:#fff;padding:6px 12px;border-radius:4px;' +
        'font:12px monospace;z-index:9999;pointer-events:none';
    document.body.appendChild(note);
    setTimeout(function () { note.remove(); }, 2500);
}

// #canvas persists across layer switches (only its innerHTML is replaced), so
// this listener survives navigation, like the zoom handlers above.
document.getElementById('canvas').addEventListener('click', function (ev) {
    const coord = sourceCoordOf(ev);
    if (!coord) return;
    ev.preventDefault();
    ev.stopPropagation();
    if (hostTarget) {
        hostTarget.postMessage({ type: 'openSource', uri: coord.uri, offset: coord.offset }, '*');
    } else if (coord.link) {
        // Hand the URI to the OS. Only ever on this modifier-click, so merely
        // opening the file never launches anything.
        window.location.href = coord.link;
    } else {
        copySourceCoord(coord);
    }
}, true);

// S4: double-click a named box = the drill gesture. The artifact stays a
// read-only projection — it only *reports* the box; the host decides what the
// committed viewframe is (workbench: focus `inst:<name>` + highlight).
document.getElementById('canvas').addEventListener('dblclick', function (ev) {
    const g = ev.target.closest ? ev.target.closest('#canvas g[data-name]') : null;
    if (!g) return;
    const name = g.getAttribute('data-name');
    if (!name || !hostTarget) return;
    ev.preventDefault();
    ev.stopPropagation();
    hostTarget.postMessage({
        type: 'viz:drill',
        name: name,
        uri: g.getAttribute('data-src-uri') || '',
        offset: parseInt(g.getAttribute('data-src-offset') || '0', 10) || 0,
    }, '*');
    setInstFocus(name);
}, true);

// S4: the host can mark the box the current viewframe looks at (replay of a
// committed `inst:` frame). One focused box at a time; an empty name clears.
window.addEventListener('message', function (e) {
    const m = e.data;
    if (!m || m.type !== 'viz:highlight') return;
    setInstFocus(m.name || '');
});

function setInstFocus(name) {
    document.querySelectorAll('#canvas g.inst-focused').forEach(function (g) {
        g.classList.remove('inst-focused');
    });
    if (name) {
        const g = document.querySelector('#canvas g[data-name="' + name + '"]');
        if (g) g.classList.add('inst-focused');
    }
    // Echo back (also on clear): the frame is sandboxed (opaque origin), so
    // the host cannot read the DOM — it learns the highlight state through
    // this message.
    const host = mcodeHost || ((window.parent && window.parent !== window) ? window.parent : null);
    host && host.postMessage({ type: 'viz:highlighted', name: name }, '*');
}

// Startup
function announceReady() {
    const host = mcodeHost || ((window.parent && window.parent !== window) ? window.parent : null);
    host && host.postMessage({ type: 'viz:ready' }, '*');
}

// Keeper protocol (mcide iframe-keeper): the workbench frame persists across
// reparent/reload and asks the artifact for its zoom state (viz:save →
// viz:state), restores it (viz:restore), and learns when the artifact is
// listening (viz:ready) so committed-frame highlights survive the load race.
window.addEventListener('message', function (e) {
    const m = e.data;
    if (!m) return;
    if (m.type === 'viz:save') {
        const host = mcodeHost || ((window.parent && window.parent !== window) ? window.parent : null);
        host && host.postMessage({ type: 'viz:state', state: { zoomLevel: zoomLevel } }, '*');
    } else if (m.type === 'viz:restore' && m.state && Number(m.state.zoomLevel) > 0) {
        applyZoom(Number(m.state.zoomLevel));
    }
});

if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', function () { init(); announceReady(); });
} else {
    init();
    announceReady();
}
"##
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The page's source-navigation bridge. Two halves must survive a refactor
    /// together: the modifier gate (a plain click has to stay with
    /// `expandSubModule`, so the handler may only consume modified clicks) and
    /// the `data-src-*` stamp the Rust renderer writes onto boxes and pins.
    /// Dropping either silently downgrades every click to nothing.
    #[test]
    fn source_navigation_js_gates_on_the_modifier_and_reads_the_stamp() {
        let js = js();
        assert!(js.contains("data-src-uri"), "no source stamp lookup");
        assert!(js.contains("data-src-offset"), "no source stamp lookup");
        assert!(
            js.contains("ev.metaKey || ev.ctrlKey"),
            "no modifier gate: a plain click must keep its existing meaning"
        );
        assert!(
            js.contains("acquireVsCodeApi"),
            "no webview host acquisition"
        );
        assert!(js.contains("'openSource'"), "no host message type");
    }

    /// Three rungs, in order: a host to post to, a `vscode://` link the writer
    /// stamped, and finally the copy-the-coordinate fallback. Losing the middle
    /// rung sends every standalone click back to a fallback no editor accepts;
    /// losing the last one makes a click do nothing at all, which is worse than
    /// showing a coordinate nobody can use.
    #[test]
    fn source_navigation_falls_back_host_then_link_then_copy() {
        let js = js();
        // Only the click handler's own rungs count: `copySourceCoord`'s
        // *definition* sits above it and would otherwise pass as the last rung.
        let handler = &js[js
            .find("addEventListener('click', function (ev)")
            .expect("click handler")..];
        let host = handler.find("hostTarget.postMessage").expect("host rung");
        let link = handler.find("coord.link").expect("vscode:// rung");
        let copy = handler.find("copySourceCoord(coord)").expect("copy rung");
        assert!(
            host < link && link < copy,
            "the fallback rungs are out of order"
        );
        assert!(
            js.contains("data-src-vscode"),
            "the link attribute is never read"
        );
    }

    /// D5's two halves: the status line announces the gesture (and goes quiet
    /// when the layer on screen holds nothing jumpable, so the page never
    /// advertises a click that would do nothing), and holding the modifier arms
    /// the CSS that outlines the jumpable set.
    #[test]
    fn the_gesture_is_announced_and_armed_by_the_modifier() {
        let js = js();
        assert!(
            js.contains(r#"class="hint" id="hint""#),
            "the status row has no hint element to fill"
        );
        assert!(
            js.matches("updateHint").count() >= 2,
            "the hint is defined but never filled in"
        );
        assert!(
            js.contains("nav-armed"),
            "the modifier never arms the outline"
        );
        assert!(
            js.contains("e.key === 'Meta' || e.key === 'Control'"),
            "wrong modifier keys: the outline would arm on an unrelated key"
        );
        assert!(
            js.contains("'blur'"),
            "a highlight stuck on after focus loss is never disarmed"
        );
    }
}

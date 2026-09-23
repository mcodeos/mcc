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

// Camera control
// The canvas is a camera over an infinite plane, not a window with scrollbars:
// the #zoom-pane keeps its zoom=1 size and the view is a translate + scale
// transform on it. Zoom is anchored at the cursor — the point under the
// pointer stays under the pointer — and a plain wheel or a drag pans. SVG
// bytes are untouched; only the pane is transformed.
const ZOOM_MIN = 0.05;
const ZOOM_MAX = 16;
const ZOOM_STEP = 1.25;
const PAN_GRAB = 3; // px of travel before a press counts as a drag
let zoomLevel = 1;
let camX = 0; // camera translate, in canvas px
let camY = 0;
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

    // Re-read the viewBox aspect ratio in case this layer's SVG differs, then
    // size the pane to its zoom=1 footprint (canvas content width, height per
    // the drawing's aspect). The camera transform does the rest.
    const vb = svg.getAttribute('viewBox');
    if (vb) {
        const parts = vb.trim().split(/[\s,]+/).map(Number);
        if (parts.length === 4 && parts[2] > 0 && parts[3] > 0) {
            zoomAspect = parts[2] / parts[3];
        }
    }
    const cs = getComputedStyle(canvas);
    const padX = (parseFloat(cs.paddingLeft) || 0) + (parseFloat(cs.paddingRight) || 0);
    const baseW = Math.max(1, canvas.clientWidth - padX);
    pane.style.width = baseW + 'px';
    pane.style.height = (baseW / zoomAspect) + 'px';
    return pane;
}

function applyCamera() {
    zoomLevel = Math.max(ZOOM_MIN, Math.min(ZOOM_MAX, zoomLevel));
    const pane = ensureZoomPane();
    if (!pane) return;
    pane.style.transformOrigin = '0 0';
    pane.style.transform =
        'translate(' + camX + 'px,' + camY + 'px) scale(' + zoomLevel + ')';

    const label = document.getElementById('zoom-level');
    if (label) label.textContent = Math.round(zoomLevel * 100) + '%';
    const zin = document.getElementById('zoom-in');
    const zout = document.getElementById('zoom-out');
    if (zin) zin.disabled = zoomLevel >= ZOOM_MAX;
    if (zout) zout.disabled = zoomLevel <= ZOOM_MIN;
    // VS Code keeps webview state across an html replacement of the same
    // panel, so the zoom survives a re-render without a host round-trip.
    // (The standalone page and the mcide iframe have no such store; the
    // iframe keeper protocol covers the latter.)
    if (mcodeHost && typeof mcodeHost.setState === 'function') {
        mcodeHost.setState({ zoomLevel: zoomLevel });
    }
}

// The view v = translate(t) · scale(k) puts a plane point p at s = t + k·p.
// Zooming with the cursor pinned (s fixed) while k → k' therefore means
// t' = s − (k'/k)·(s − t); dropping the second term zooms toward the origin.
function zoomAround(kNext, sx, sy) {
    const k = Math.max(ZOOM_MIN, Math.min(ZOOM_MAX, kNext));
    const ratio = k / zoomLevel;
    camX = sx - ratio * (camX - sx);
    camY = sy - ratio * (camY - sy);
    zoomLevel = k;
    applyCamera();
}

// Anchor for the buttons: the center of the canvas viewport, in the pane's
// view coordinates (distance from the pane's transformed origin).
function zoomAnchor() {
    const pane = ensureZoomPane();
    const canvas = document.getElementById('canvas');
    if (!pane) return { x: 0, y: 0 };
    const pr = pane.getBoundingClientRect();
    const cr = canvas.getBoundingClientRect();
    return {
        x: cr.left + cr.width / 2 - pr.left,
        y: cr.top + cr.height / 2 - pr.top,
    };
}

function zoomIn() {
    const a = zoomAnchor();
    zoomAround(zoomLevel * ZOOM_STEP, a.x, a.y);
}
function zoomOut() {
    const a = zoomAnchor();
    zoomAround(zoomLevel / ZOOM_STEP, a.x, a.y);
}
function zoomReset() {
    camX = 0;
    camY = 0;
    zoomLevel = 1;
    applyCamera();
}

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
    camX = 0;
    camY = 0;
    applyCamera();
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
    camX = 0;
    camY = 0;
    applyCamera();
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

// Camera gesture / button wiring
// The canvas element itself persists across layer switches (only its
// innerHTML is replaced), so listeners attached here survive navigation.
const zoomCanvas = document.getElementById('canvas');
zoomCanvas.addEventListener('wheel', function (e) {
    // Trackpad pinch and Ctrl/Cmd + wheel zoom, anchored at the pointer.
    // Chromium reports a pinch as wheel events with ctrlKey=true (on macOS
    // metaKey is also set); the deltaY sign selects the direction. A plain
    // wheel pans the plane.
    e.preventDefault();
    if (e.ctrlKey || e.metaKey) {
        const pane = ensureZoomPane();
        if (!pane) return;
        const r = pane.getBoundingClientRect();
        zoomAround(
            zoomLevel * (e.deltaY < 0 ? ZOOM_STEP : 1 / ZOOM_STEP),
            e.clientX - r.left, e.clientY - r.top);
    } else {
        camX -= e.deltaX;
        camY -= e.deltaY;
        applyCamera();
    }
}, { passive: false });

// Drag to pan. A press only becomes a drag after PAN_GRAB px of travel, and a
// click that ends a real drag is swallowed (capture, ahead of the handlers
// below), so plain clicks keep their meaning — drill, select, navigate.
let dragStart = null;
let dragMoved = false;
zoomCanvas.addEventListener('mousedown', function (e) {
    if (e.button !== 0 || e.metaKey || e.ctrlKey) return;
    dragStart = { x: e.clientX, y: e.clientY, camX: camX, camY: camY };
    dragMoved = false;
    e.preventDefault();
});
window.addEventListener('mousemove', function (e) {
    if (!dragStart) return;
    const dx = e.clientX - dragStart.x;
    const dy = e.clientY - dragStart.y;
    if (!dragMoved) {
        if (Math.abs(dx) < PAN_GRAB && Math.abs(dy) < PAN_GRAB) return;
        dragMoved = true;
        const pane = document.getElementById('zoom-pane');
        if (pane) pane.classList.add('dragging');
    }
    camX = dragStart.camX + dx;
    camY = dragStart.camY + dy;
    applyCamera();
});
window.addEventListener('mouseup', function () {
    if (!dragStart) return;
    dragStart = null;
    const pane = document.getElementById('zoom-pane');
    if (pane) pane.classList.remove('dragging');
});
function swallowDragClick(ev) {
    if (!dragMoved) return;
    dragMoved = false;
    ev.preventDefault();
    ev.stopPropagation();
}
zoomCanvas.addEventListener('click', swallowDragClick, true);

document.getElementById('zoom-in').addEventListener('click', zoomIn);
document.getElementById('zoom-out').addEventListener('click', zoomOut);
document.getElementById('zoom-reset').addEventListener('click', zoomReset);
window.addEventListener('resize', function () { applyCamera(); });

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

// S5.5: single click on a class label → tell the host to open the selector
// filtered to that class. Only fires when the target has a class stamp.
document.getElementById('canvas').addEventListener('click', function (ev) {
    const g = ev.target.closest ? ev.target.closest('#canvas g[data-symbol-source]') : null;
    if (!g) return;
    const cls = g.getAttribute('data-symbol-source');
    if (!cls || !hostTarget) return;
    ev.preventDefault();
    ev.stopPropagation();
    hostTarget.postMessage({ type: 'viz:classSelect', class: cls }, '*');
}, true);

// AI/agent selection: a plain click reports WHAT was touched - pin first
// (its stage key), then the box, then the wire's net - and swallows
// nothing, so expand-onclick and modifier navigation keep working. The
// host turns this into an editor selection; an agent turns it into a
// query.
document.getElementById('canvas').addEventListener('click', function (ev) {
    const pinG = ev.target.closest ? ev.target.closest('#canvas g[data-point]') : null;
    const boxG = ev.target.closest ? ev.target.closest('#canvas g[data-name]') : null;
    const netEl = ev.target.closest ? ev.target.closest('#canvas [data-net]') : null;
    let msg = null;
    if (pinG) {
        msg = { type: 'viz:select', kind: 'pin', point: pinG.getAttribute('data-point') };
    } else if (boxG) {
        msg = {
            type: 'viz:select',
            kind: 'box',
            name: boxG.getAttribute('data-name'),
            class: boxG.getAttribute('data-class') || '',
            uri: boxG.getAttribute('data-src-uri') || '',
            offset: parseInt(boxG.getAttribute('data-src-offset') || '0', 10) || 0,
        };
    } else if (netEl) {
        msg = { type: 'viz:select', kind: 'net', net: netEl.getAttribute('data-net') };
    }
    if (msg && hostTarget) hostTarget.postMessage(msg, '*');
});

// Whole-net highlight: wires stamp data-net, so one selector marks every
// segment of the net on the current layer; the count echoes back so the
// host knows whether the net exists here.
window.addEventListener('message', function (e) {
    const m = e.data;
    if (!m || m.type !== 'viz:highlightNet') return;
    document.querySelectorAll('#canvas .net-focused').forEach(function (el) {
        el.classList.remove('net-focused');
    });
    const net = m.net || '';
    let count = 0;
    if (net) {
        document.querySelectorAll('#canvas [data-net="' + net + '"]').forEach(function (el) {
            el.classList.add('net-focused');
            count += 1;
        });
    }
    const host = mcodeHost || ((window.parent && window.parent !== window) ? window.parent : null);
    host && host.postMessage({ type: 'viz:netHighlighted', net: net, count: count }, '*');
});

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

// === Hover cards: component / pin / net / module, all data from LAYOUT ===
// The artifact never parses its own SVG for facts: data-* only routes the
// element to an identity key, and the card reads the embedded manifest.
let cardEl = null;
let cardPin = null;

function layoutReady() {
    return typeof LAYOUT !== 'undefined' && LAYOUT && LAYOUT.boxes;
}

function boxByPath(p) {
    if (!layoutReady()) return null;
    return (LAYOUT.boxes || []).find(function (b) { return b.path === p; }) || null;
}

function netRowsOnLayer(netName, bid) {
    if (!layoutReady()) return [];
    return (LAYOUT.nets || []).filter(function (n) {
        return n.name === netName && (bid === undefined || n.layer === bid);
    });
}

function esc(t) {
    return String(t === null || t === undefined ? '' : t)
        .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

function ensureCard() {
    if (cardEl) return cardEl;
    cardEl = document.createElement('div');
    cardEl.id = 'viz-card';
    cardEl.addEventListener('mouseleave', function () { hideCard(); });
    document.body.appendChild(cardEl);
    return cardEl;
}

function hideCard() {
    if (cardEl) cardEl.style.display = 'none';
}

function showCard(html, x, y) {
    const c = ensureCard();
    c.innerHTML = html;
    c.style.display = 'block';
    const r = c.getBoundingClientRect();
    let left = x + 16, top = y + 12;
    if (left + r.width > window.innerWidth - 8) left = x - r.width - 12;
    if (top + r.height > window.innerHeight - 8) top = y - r.height - 10;
    c.style.left = left + 'px';
    c.style.top = top + 'px';
}

function pinRowsOf(b) {
    return (b && b.pins || []).map(function (p) {
        const src = p.src ? ' data-jump=\"' + esc(JSON.stringify(p.src)) + "\"'" : '';
        return '<tr' + src + '><td>' + esc(p.num) + '</td><td>' + esc(p.name) +
            '</td><td>' + esc(p.io) + '</td><td>' + esc(p.side || '-') + '</td></tr>';
    }).join('');
}

function componentCard(b, x, y) {
    const pr = b.params && typeof b.params === 'object' ? b.params : {};
    const rows = pinRowsOf(b);
    showCard(
        '<div class="vc-title">' + esc(b.name) + (b.dnp ? ' <span class="vc-dnp">DNP</span>' : '') + '</div>' +
        '<div class="vc-sub">' + esc(b.class) + ' · ' + esc(b.path) + '</div>' +
        '<table class="vc-params">' +
        (pr.value ? '<tr><td>value</td><td>' + esc(pr.value) + '</td></tr>' : '') +
        (pr.partno ? '<tr><td>partno</td><td>' + esc(pr.partno) + '</td></tr>' : '') +
        (pr.package ? '<tr><td>package</td><td>' + esc(pr.package) + '</td></tr>' : '') +
        '</table>' +
        (rows ? '<table class="vc-pins"><tr><th>#</th><th>name</th><th>io</th><th>side</th></tr>' + rows + '</table>' : ''),
        x, y
    );
}

function pinCard(point, x, y) {
    if (!layoutReady()) return;
    let b = null;
    let pin = null;
    (LAYOUT.boxes || []).forEach(function (cand) {
        (cand.pins || []).forEach(function (p) {
            if (p.point === point) { b = cand; pin = p; }
        });
    });
    if (!b || !pin) return;
    const full = b.path + '.' + pin.num;
    const nets = (LAYOUT.nets || []).filter(function (n) {
        return (n.endpoints || []).indexOf(full) >= 0;
    });
    const netList = nets.map(function (n) { return esc(n.name); }).join(', ');
    const srcAttr = pin.src ? ' data-jump="' + esc(JSON.stringify(pin.src)) + '"' : '';
    showCard(
        '<div class="vc-title">' + esc(b.name) + ' · ' + esc(pin.num) + '</div>' +
        '<div class="vc-sub">' + esc(pin.name) + ' · ' + esc(pin.io) + ' · ' + esc(pin.side || '-') + '</div>' +
        (netList ? '<div class="vc-nets">nets: ' + netList + '</div>' : '') +
        (pin.src ? '<div class="vc-jump"' + srcAttr + '>→ source</div>' : ''),
        x, y
    );
}

function netCard(netName, bid, x, y) {
    const rows = netRowsOnLayer(netName, bid);
    const ends = new Set();
    rows.forEach(function (n) { (n.endpoints || []).forEach(function (e2) { ends.add(e2); }); });
    const segs = rows.reduce(function (a, n) { return a + (n.segments || []).length; }, 0);
    showCard(
        '<div class="vc-title">' + esc(netName) + '</div>' +
        '<div class="vc-sub">net · ' + rows.map(function (n) { return esc(n.kind); }).join('/') +
        ' · ' + segs + ' segments</div>' +
        (ends.size ? '<div class="vc-nets">' + [...ends].map(esc).join('<br>') + '</div>' : ''),
        x, y
    );
}

function initCards() {
    if (!layoutReady()) return;
    const canvas = document.getElementById('canvas');
    canvas.addEventListener('mousemove', function (ev) {
        const boxG = ev.target.closest ? ev.target.closest('#canvas g[data-name]') : null;
        const pinG = ev.target.closest ? ev.target.closest('#canvas g[data-point]') : null;
        const netEl = ev.target.closest ? ev.target.closest('#canvas [data-net]') : null;
        if (pinG) {
            const pt = pinG.getAttribute('data-point');
            if (pt) { pinCard(pt, ev.clientX, ev.clientY); return; }
        }
        if (boxG) {
            const b = boxByPath(boxG.getAttribute('data-mcc-path') || '');
            if (b) { componentCard(b, ev.clientX, ev.clientY); return; }
        }
        if (netEl) {
            netCard(netEl.getAttribute('data-net'), currentBid, ev.clientX, ev.clientY);
            return;
        }
        hideCard();
    });
    canvas.addEventListener('mouseleave', hideCard);
    window.addEventListener('keydown', function (e2) { if (e2.key === 'Escape') hideCard(); });
    // Card source rows and jump chips post mcc.openSource like canvas clicks.
    document.body.addEventListener('click', function (ev) {
        const j = ev.target.closest ? ev.target.closest('[data-jump]') : null;
        if (!j || !hostTarget) return;
        try {
            const src = JSON.parse(j.getAttribute('data-jump'));
            if (src && src.uri) hostTarget.postMessage(
                { type: 'mcc.openSource', uri: src.uri, offset: src.offset }, '*');
        } catch (err) { /* malformed payload: ignore */ }
    });
}

// Startup
function announceReady() {
    const host = mcodeHost || ((window.parent && window.parent !== window) ? window.parent : null);
    host && host.postMessage({ type: 'viz:ready' }, '*');
}

// The VS Code webview store read back before the first render, so a panel
// re-render (html replacement) keeps the zoom the user had set.
function restoreZoomState() {
    if (!mcodeHost || typeof mcodeHost.getState !== 'function') return;
    const s = mcodeHost.getState();
    const z = s && Number(s.zoomLevel);
    if (z > 0) zoomLevel = Math.max(ZOOM_MIN, Math.min(ZOOM_MAX, z));
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
        zoomLevel = Math.max(ZOOM_MIN, Math.min(ZOOM_MAX, Number(m.state.zoomLevel)));
        applyCamera();
    } else if (m.type === 'viz:queryClass') {
        // The host cannot read this sandboxed DOM, so the class selector's
        // candidates are enumerated here: every named box stamped with the
        // class (data-class; data-symbol-source as the legacy narrower stamp).
        // uri/offset ride along so the host can jump on pick.
        const cls = m.class || '';
        const seen = {};
        const out = [];
        document.querySelectorAll('#canvas g[data-name]').forEach(function (g) {
            if (g.getAttribute('data-class') !== cls && g.getAttribute('data-symbol-source') !== cls) return;
            const name = g.getAttribute('data-name');
            if (!name || seen[name]) return;
            seen[name] = 1;
            out.push({
                name: name,
                uri: g.getAttribute('data-src-uri') || '',
                offset: parseInt(g.getAttribute('data-src-offset') || '0', 10) || 0,
            });
        });
        (mcodeHost || hostTarget) && (mcodeHost || hostTarget).postMessage({
            type: 'viz:classInstances',
            class: cls,
            instances: out,
        }, '*');
    }
});

if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', function () { restoreZoomState(); init(); initCards(); announceReady(); });
} else {
    restoreZoomState();
    init();
    initCards();
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

    /// The host-facing protocol surface S4/S5.5 wire against: drill and class
    /// select report outward; highlight, save/restore and the class query are
    /// answered; the echo closes the loop for a sandboxed frame the host
    /// cannot read.
    #[test]
    fn host_protocol_messages_round_trip() {
        let js = js();
        for msg in [
            "'viz:drill'",
            "'viz:classSelect'",
            "'viz:highlighted'",
            "'viz:ready'",
            "'viz:state'",
            "'viz:highlight'",
            "'viz:save'",
            "'viz:restore'",
            "'viz:queryClass'",
            "'viz:classInstances'",
        ] {
            assert!(js.contains(msg), "protocol message {msg} missing");
        }
    }

    /// The agent-facing pair: a plain click reports what was touched (pin
    /// key, box, or net) without swallowing expand/navigation, and the host
    /// can light up a whole net by name because wires carry data-net.
    #[test]
    fn select_reports_touch_and_net_highlight_answers() {
        let js = js();
        assert!(
            js.contains("type: 'viz:select'"),
            "no select message emitted"
        );
        assert!(
            js.contains("closest('#canvas g[data-point]')"),
            "pin identity never read"
        );
        assert!(
            js.contains("'viz:highlightNet'"),
            "net highlight not received"
        );
        assert!(
            js.contains("viz:netHighlighted"),
            "highlight echo missing"
        );
        // The select listener must not stop propagation: expand-onclick and
        // modifier navigation share the same click.
        let sel_at = js.find("closest('#canvas g[data-point]')").expect("sel handler");
        assert!(
            !js[sel_at..].starts_with("stopPropagation"),
            "select must not swallow the click"
        );
    }

    /// Hover cards are self-querying: they read the embedded LAYOUT manifest
    /// (never the SVG), gate on its presence so older artifacts degrade
    /// silently, route elements to identity via data-mcc-path / data-point /
    /// data-net, and jump rows post mcc.openSource like canvas clicks.
    #[test]
    fn hover_cards_read_the_layout_manifest() {
        let js = js();
        assert!(js.contains("typeof LAYOUT !== 'undefined'"), "no LAYOUT gate");
        assert!(js.contains("closest('#canvas g[data-name]')"), "box identity not routed");
        assert!(js.contains("data-mcc-path"), "box path never read");
        assert!(js.contains("closest('#canvas g[data-point]')"), "pin identity not routed");
        assert!(js.contains("closest('#canvas [data-net]')"), "net identity not routed");
        assert!(js.contains("componentCard("), "component card missing");
        assert!(js.contains("pinCard("), "pin card missing");
        assert!(js.contains("netCard("), "net card missing");
        assert!(js.contains("mcc.openSource"), "card jump does not reach the host");
        assert!(js.contains("initCards()"), "cards never initialize");
    }

    /// The VS Code-native keeper: zoom writes into the webview state store and
    /// is read back before the first render, so a panel re-render keeps the
    /// user's zoom without a host round-trip.
    #[test]
    fn zoom_persists_through_the_vscode_state_store() {
        let js = js();
        assert!(
            js.contains("mcodeHost.setState({ zoomLevel: zoomLevel })"),
            "zoom never written to the webview state store"
        );
        let restore = js.find("function restoreZoomState").expect("restore fn");
        let startup = js.find("restoreZoomState();").expect("startup call");
        let init_call = js.find("init();").expect("init call");
        assert!(
            restore < startup && startup < init_call,
            "the zoom restore must run before the first render"
        );
    }

    /// M2's gate (9.16 plan): the canvas is a camera, not a scrollbar. Three
    /// halves must survive a refactor together: the translate + scale
    /// transform on the pane (the SVG itself is never resized), the
    /// cursor-anchored zoom (the anchor term that pins the point under the
    /// pointer), and drag/wheel panning that leaves a plain click's meaning
    /// intact (travel threshold + a captured swallow of drag-ending clicks).
    /// Losing the anchor term turns every zoom into a jump toward the origin;
    /// losing the swallow makes every pan end in a stray drill or select.
    #[test]
    fn the_camera_is_translate_scale_with_cursor_anchored_zoom() {
        let js = js();
        assert!(
            js.contains("'translate(' + camX + 'px,' + camY + 'px) scale(' + zoomLevel + ')'"),
            "the pane is not moved by a camera transform"
        );
        assert!(
            js.contains("transformOrigin = '0 0'"),
            "the transform anchors at the pane corner, not its center"
        );
        assert!(
            js.contains("camX = sx - ratio * (camX - sx)"),
            "zoom is not cursor-anchored: the translate is never rewritten"
        );
        assert!(
            js.contains("e.clientX - r.left"),
            "the wheel anchor is not the pointer position"
        );
        assert!(
            js.contains("camX -= e.deltaX"),
            "a plain wheel does not pan the plane"
        );
        assert!(
            js.contains("PAN_GRAB"),
            "no drag threshold: a plain click would pan"
        );
        assert!(
            js.contains("swallowDragClick"),
            "a drag-ending click keeps its old meaning"
        );
        // Range per the plan (0.05–16×); the pane keeps its zoom=1 size, so
        // only the transform may depend on zoomLevel.
        assert!(js.contains("const ZOOM_MIN = 0.05;"), "narrow zoom floor");
        assert!(js.contains("const ZOOM_MAX = 16;"), "narrow zoom ceiling");
        let cam = js.find("function applyCamera").expect("camera fn");
        let size = js.find("pane.style.width").expect("pane sizing");
        assert!(
            size < cam && !js[cam..cam + 4000].contains("pane.style.width"),
            "the camera must not resize the pane; it only transforms it"
        );
    }
}

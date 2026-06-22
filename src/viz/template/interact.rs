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

// ── DOC self-check ────────────────────────────────────────────────
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

// ── State ──────────────────────────────────────────────────────
const navStack = [];
let currentBid = DOC.root_bid;

// ── Smart layer lookup: try number / string / fallback ─────────
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

// ── Initialization ─────────────────────────────────────────────────────
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

// ── Switch layer ─────────────────────────────────────────────────────
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
    return true;
}

// ── Expand sub-module (SVG onclick="expandSubModule(<bid>)") ─────────
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

// ── Breadcrumb ─────────────────────────────────────────────────────
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

// ── Bottom stats ────────────────────────────────────────────────────
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
        '<span>Current SVG: ' + svgKb + ' KB</span>';
}

function escapeHtml(text) {
    if (text === null || text === undefined) return '';
    return String(text)
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;');
}

// ── Startup ──────────────────────────────────────────────────────
if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', init);
} else {
    init();
}
"##
}

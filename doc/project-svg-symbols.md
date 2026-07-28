# Project SVG Symbols

MCC can replace a component's built-in body with a project-local SVG symbol. Pins, pin labels,
electrical anchors, layout, and routing remain controlled by MCC.

## Project layout

Create a `symbols/manifest.toml` file below the project root:

```toml
schema_version = 1

[[symbols]]
class = "USB.MINI_B"
file = "usb-mini-b.svg"
```

The `class` value must match the component class name. The `file` path is relative to the
`symbols/` directory and must not contain `..` or escape through a symlink. Missing or rejected
symbols fall back to MCC's built-in renderer.

## Supported SVG subset

Every file must be UTF-8, use the `.svg` extension, contain a positive `viewBox`, and stay below
256 KiB. The supported elements are:

```text
svg, g, path, rect, circle, ellipse, line, polyline, polygon
```

Use presentation attributes such as `fill`, `stroke`, `stroke-width`, and shape coordinates.
MCC deliberately rejects active or externally resolved SVG features, including:

- scripts, styles, XML declarations, DOCTYPE, comments, and entities;
- event attributes such as `onload`;
- `href`, `use`, external URLs, embedded data, and `foreignObject`;
- CSS classes, element IDs, filters, masks, and clip paths;
- text nodes and arbitrary fonts.

This strict subset keeps generated HTML self-contained and prevents project assets from injecting
active content into the visualization webview.

## Rendering contract

The symbol's `viewBox` is scaled into the component body with `xMidYMid meet`, preserving its
aspect ratio. Keep a small margin inside the source `viewBox`; do not draw pins or labels into the
SVG because MCC overlays them from the circuit model.

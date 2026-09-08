// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Power / ground label render
//!
//! §5⑤ (classification-retirement-design): this legacy shape only renders
//! `Symbol::Unknown` + `BoxKind::PowerLabel` boxes. Since §5③, real rail boxes
//! always carry `Symbol::PowerRail` and render via [`super::power_rail::PowerRailShape`]
//! (which is already is_ground-driven); a box that reaches here has **no declared
//! role to key on**, so it draws one neutral style — name-keyed ground/supply
//! coloring is name resolution (ruling ③) and is removed. No declaration → no
//! guessed ground look.

use crate::vector::graph::McVecBox;

use super::shape::BoxShape;

pub struct PowerLabelShape;

impl BoxShape for PowerLabelShape {
    fn render(&self, b: &McVecBox) -> String {
        let cx = b.x + b.w / 2.0;
        let cy = b.y + b.h / 2.0;
        // Neutral fallback style (no declared role available at this layer).
        let (fill, stroke, text_col) = ("#F4F6F6", "#7F8C8D", "#566573");

        format!(
            r##"  <g class="comp power-label" data-id="{id}">
    <rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" rx="4"
          fill="{fill}" stroke="{stroke}" stroke-width="1.2"/>
    <text x="{cx:.1}" y="{cy:.1}" text-anchor="middle" dominant-baseline="central"
          font-size="11" font-weight="700" fill="{tc}">{name}</text>
  </g>
"##,
            id = b.id,
            x = b.x,
            y = b.y,
            w = b.w,
            h = b.h,
            cx = cx,
            cy = cy,
            fill = fill,
            stroke = stroke,
            tc = text_col,
            name = b.name,
        )
    }
}

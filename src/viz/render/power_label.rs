// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Power / ground label render

use crate::vector::graph::McVecBox;

use super::shape::BoxShape;

pub struct PowerLabelShape;

impl BoxShape for PowerLabelShape {
    fn render(&self, b: &McVecBox) -> String {
        let cx = b.x + b.w / 2.0;
        let cy = b.y + b.h / 2.0;
        let u = b.name.to_uppercase();
        let is_gnd = u.contains("GND") || u.contains("VSS");
        let (fill, stroke, text_col) = if is_gnd {
            ("#FDEDEC", "#C0392B", "#922B21")
        } else {
            ("#EAFAF1", "#27AE60", "#1E8449")
        };

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

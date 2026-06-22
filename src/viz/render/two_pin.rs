// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Two-pin part render (R / C / L / D etc.)

use crate::vector::graph::McVecBox;

use super::shape::BoxShape;

pub struct TwoPinShape;

impl BoxShape for TwoPinShape {
    fn render(&self, b: &McVecBox) -> String {
        let cx = b.x + b.w / 2.0;
        let cy = b.y + b.h / 2.0;
        let cls = b.class_name.to_uppercase();
        let color = if cls.contains("CAP") {
            "#2471A3"
        } else if cls.contains("IND") {
            "#7D3C98"
        } else {
            "#333"
        };

        format!(
            r##"  <g class="comp two-pin" data-id="{id}">
    <rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" rx="3"
          fill="#fff" stroke="{col}" stroke-width="1.2"/>
    <text x="{cx:.1}" y="{cy:.1}" text-anchor="middle" dominant-baseline="central"
          font-size="11" font-weight="500" fill="{col}">{name}</text>
  </g>
"##,
            id = b.id,
            x = b.x,
            y = b.y,
            w = b.w,
            h = b.h,
            cx = cx,
            cy = cy,
            col = color,
            name = b.name,
        )
    }
}

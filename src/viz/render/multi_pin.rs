// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Multi-pin IC render

use crate::vector::graph::McVecBox;

use super::shape::BoxShape;

pub struct MultiPinShape;

impl BoxShape for MultiPinShape {
    fn render(&self, b: &McVecBox) -> String {
        let cx = b.x + b.w / 2.0;
        let cy = b.y + b.h / 2.0;
        format!(
            r##"  <g class="comp multi-pin" data-id="{id}">
    <rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" rx="4"
          fill="#EDE7F6" stroke="#5E35B1" stroke-width="1.2"/>
    <text x="{cx:.1}" y="{t1:.1}" text-anchor="middle"
          font-size="13" font-weight="600" fill="#311B92">{name}</text>
    <text x="{cx:.1}" y="{t2:.1}" text-anchor="middle"
          font-size="9" fill="#7E57C2">{cls} {p}p</text>
  </g>
"##,
            id = b.id,
            x = b.x,
            y = b.y,
            w = b.w,
            h = b.h,
            cx = cx,
            t1 = cy - 5.0,
            name = b.name,
            t2 = cy + 10.0,
            cls = b.class_name,
            p = b.pin_count,
        )
    }
}

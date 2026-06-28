// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Two-pin part render (R / C / L / D etc.)

use crate::vector::graph::McVecBox;

use super::shape::BoxShape;

pub struct TwoPinShape;

impl BoxShape for TwoPinShape {
    fn render(&self, b: &McVecBox) -> String {
        let cls = b.class_name.to_uppercase();
        let color = if cls.contains("CAP") {
            "#2471A3"
        } else if cls.contains("IND") {
            "#7D3C98"
        } else {
            "#333"
        };

        let name_label = format!(
            r##"    <text x="{:.1}" y="{:.1}" text-anchor="start"
          font-size="11" font-weight="500" fill="{col}">{name}</text>
"##,
            b.x,
            b.y - 14.0,
            col = color,
            name = b.name,
        );
        let cls_label = if !b.class_name.is_empty() {
            format!(
                r##"    <text x="{:.1}" y="{:.1}" text-anchor="start"
          font-size="8" fill="#999">{cls}</text>
"##,
                b.x,
                b.y - 2.0,
                cls = b.class_name,
            )
        } else {
            String::new()
        };

        format!(
            r##"  <g class="comp two-pin" data-id="{id}">
{name_label}{cls_label}    <rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" rx="3"
          fill="#fff" stroke="{col}" stroke-width="1.2"/>
  </g>
"##,
            id = b.id,
            name_label = name_label,
            cls_label = cls_label,
            x = b.x,
            y = b.y,
            w = b.w,
            h = b.h,
            col = color,
        )
    }
}

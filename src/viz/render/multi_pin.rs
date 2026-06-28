// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Multi-pin IC render

use crate::vector::graph::McVecBox;

use super::shape::BoxShape;

pub struct MultiPinShape;

impl BoxShape for MultiPinShape {
    fn render(&self, b: &McVecBox) -> String {
        let name_label = format!(
            r##"    <text x="{:.1}" y="{:.1}" text-anchor="start"
          font-size="13" font-weight="600" fill="#311B92">{name}</text>
"##,
            b.x,
            b.y - 14.0,
            name = b.name,
        );
        let cls_label = if !b.class_name.is_empty() {
            format!(
                r##"    <text x="{:.1}" y="{:.1}" text-anchor="start"
          font-size="9" fill="#7E57C2">{cls}</text>
"##,
                b.x,
                b.y - 2.0,
                cls = b.class_name,
            )
        } else {
            String::new()
        };

        format!(
            r##"  <g class="comp multi-pin" data-id="{id}">
{name_label}{cls_label}    <rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" rx="4"
          fill="#EDE7F6" stroke="#5E35B1" stroke-width="1.2"/>
  </g>
"##,
            id = b.id,
            name_label = name_label,
            cls_label = cls_label,
            x = b.x,
            y = b.y,
            w = b.w,
            h = b.h,
        )
    }
}

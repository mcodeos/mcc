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

        // Virtual instantiation view (mcd docs-mc 16-export-viz §6): suppress
        // the fabricated instance name (`u_1`); the class-name label below
        // identifies the part instead. Mirrors ic.rs / multi_pin.rs.
        let name_label = if b.suppress_instance_name {
            String::new()
        } else {
            format!(
                r##"    <text x="{:.1}" y="{:.1}" text-anchor="start"
          font-size="11" font-weight="500" fill="{col}">{name}</text>
"##,
                b.x,
                b.y - 14.0,
                col = color,
                name = b.name,
            )
        };
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::{BoxKind, IoSummary, Symbol};

    fn mk_box(name: &str, class_name: &str, suppress: bool) -> McVecBox {
        let mut b = McVecBox::new_v2(
            1,
            name.into(),
            class_name.into(),
            BoxKind::TwoPin,
            Symbol::Unknown,
            None,
            None,
            2,
            IoSummary::new(),
            name.to_string(),
            Vec::new(),
        );
        b.x = 10.0;
        b.y = 20.0;
        b.w = 40.0;
        b.h = 16.0;
        b.suppress_instance_name = suppress;
        b
    }

    #[test]
    fn real_two_pin_shows_instance_and_class_name() {
        let b = mk_box("J1", "CONN", false);
        let svg = TwoPinShape.render(&b);
        assert!(svg.contains(">J1</text>"), "{svg}");
        assert!(svg.contains(">CONN</text>"), "{svg}");
    }

    #[test]
    fn virtual_two_pin_hides_fabricated_instance_name() {
        // mcd docs-mc 16-export-viz §6: a virtually instantiated part (wrapper
        // `u_1`) must not leak its fabricated instance name — the class name
        // identifies the part instead. This is the `Symbol::Unknown` /
        // `BoxKind::TwoPin` fallback path (e.g. parameterized-pin connectors).
        let b = mk_box("u_1", "WTB.MOLEX_KK", true);
        let svg = TwoPinShape.render(&b);
        assert!(!svg.contains("u_1"), "instance name must not render: {svg}");
        assert!(
            svg.contains(">WTB.MOLEX_KK</text>"),
            "class name should render: {svg}"
        );
    }
}

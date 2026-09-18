// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The difference between two readings of one segment.
//!
//! `show stage <seg>` answers "what does this world look like". This answers
//! "what changed between these two worlds", in the projection family's `change`
//! shape (`{type, kind, id, delta}`), so a review -- human or automatic -- can
//! state what it expects and have the difference checked against that.
//!
//! # The alignment law is per class, and per segment
//!
//! The obvious rule is "align on `canon_key`, and an item without one cannot be
//! aligned". That rule is wrong, and measurably so: on `hbl`, 97 `metrics` items
//! and 10 `segment` items all publish `canon_key: null`, yet they are perfectly
//! alignable. A metric's identity is `(family, field, layer)`; an edge segment's
//! is the pair of endpoint paths. So each class states its own key -- and since
//! the classes differ from segment to segment, so does the table. A **[`Law`]**
//! is that table, one per segment, and they sit next to each other below so the
//! question "what is a key here" is answered in one place.
//!
//! "Unaligned" means "this class's key function produced nothing", never "fall
//! back to something weaker".
//!
//! # What is not an identity
//!
//! `key` (`D39`), `point` (`PointId`), `nid`, `_net<k>` and `index` are
//! build-local ordinals -- `D{}` is literally the `InstTable` row number, so
//! inserting one instance shifts every one of them. They are never keys here,
//! and they are excluded from content comparison, because comparing whole items
//! would make a single insertion read as "everything changed".
//!
//! # Scope
//!
//! This module produces the data and its text face. The command that names the
//! two worlds and carries the answer is `mcc diff` (`cmds::diff`); this module
//! holds no opinion about how an operand is spelled or loaded.
//!
//! Nothing here builds a `StageView`: a difference belongs to two worlds, and a
//! `StageView` carries one `world_ver`. Naming both is the envelope's job, and
//! it does it by adding one key rather than a second envelope (`cmds::diff`
//! carries side B and the non-row parts under `stage.diff`).
//!
//! # Which segments have a law
//!
//! `stage.viz`, `stage.p2` and `stage.vec` do. `stage.p1` does not, and the
//! reason is not effort: it publishes **no items at all** (`items: []`, with its
//! counts block reading `stmts: 0` -- the statement items the design gives it
//! are still owed). A difference over two empty readings reports zero changes,
//! and zero changes is indistinguishable from two identical readings on the
//! readout face. A law whose every answer is "nothing differed" is worse than no
//! law, so the segment waits for its items.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Value};

use crate::stages::{net_origin, NetOrigin, StageSeg, StageView};
use crate::viz::stability::report::StabilityReport;

/// Separator between key components. `\u{1}` cannot appear in a canonical path
/// or a def ident, so the encoding stays injective.
const SEP: char = '\u{1}';

/// Separator between the members of one end's path list.
const LIST: char = '\u{2}';

/// The `view` name a difference of two `stage.viz` readings publishes.
///
/// Same shape as `join`'s names (`join.p2->vec`): the family first, then the
/// two things being compared. It is not a `StageSeg`, which is why the command
/// cannot call `StageView::new` -- there is no segment whose counts these are.
pub const DIFF_VIZ_VIEW: &str = "diff.stage.viz";

/// The same name for the flat electrical truth.
pub const DIFF_P2_VIEW: &str = "diff.stage.p2";

/// The same name for the vector graph.
pub const DIFF_VEC_VIEW: &str = "diff.stage.vec";

/// How many unchanged boxes a reading needs before the share of them that moved
/// is worth reading at all: below this, "most of them moved" is one or two
/// boxes and says nothing.
pub const LOCALITY_MIN_SAMPLE: usize = 4;

/// One class's alignment rule within a segment's law.
pub struct ClassSpec {
    /// The `class` value this rule is for. An item of a class no rule names
    /// takes no part in the difference.
    pub class: &'static str,
    /// The alignment key, without the class prefix the core adds. `None` means
    /// "this item cannot be aligned across builds".
    pub key: fn(&Value) -> Option<String>,
    /// The fields compared for a matched pair, by a whitelist -- everything not
    /// listed is either the key or a build-local ordinal.
    pub content: fn(&Value) -> &'static [&'static str],
    /// The human-readable handle printed on a change row. For reading, not for
    /// matching -- matching already happened on [`ClassSpec::key`].
    pub id: fn(&Value) -> Value,
}

/// One segment's alignment law: which classes it has, and how each is keyed.
pub struct Law {
    /// The `view` name a difference in this law publishes.
    pub view: &'static str,
    /// The identity of this table, as a name a saved reading can carry.
    ///
    /// A difference is only well defined under **one** key table, and until
    /// this existed nothing said which one a reading had been aligned under:
    /// the table was chosen by `--view` out of the running binary, so two
    /// readings compared by a third build were compared under a table neither
    /// of them had named. The value is **declared**, not derived -- the same
    /// kind of version as the drawing contract's ([`StageView::
    /// carrying_drawing_contract`]) -- and whoever changes a class's key
    /// function, or the class set, bumps the trailing number.
    pub key_table: &'static str,
    /// The classes, one rule each.
    pub classes: &'static [ClassSpec],
    /// Nets read off the items that reference them, per side, split into the
    /// ones with a cross-build key and a count of the references to those
    /// without. Only a segment whose nets are *not* items has this reading.
    ///
    /// It is an `Option` rather than an empty result for the same reason
    /// [`Law::box_stability`] is: a segment with no such references has no such
    /// reading, and "0 pins on nameless nets" is a claim it cannot make.
    pub net_refs: Option<fn(&[Value]) -> (BTreeSet<String>, usize)>,
    /// Whether this segment draws boxes, and so whether a difference of two of
    /// its readings has a stability summary at all.
    pub box_stability: bool,
}

/// The difference between two readings of one segment.
#[derive(Debug, Clone, Default)]
pub struct StageDiff {
    /// The `change` items, sorted by `(kind, id)`.
    pub changes: Vec<Value>,
    /// Items whose class key function produced nothing, plus any key collision.
    /// Reported rather than guessed at: falling back to `key` or `name` is the
    /// name-as-identity move the view model forbids.
    pub unaligned: Vec<Value>,
    /// How many references sit on a net that has no cross-build key, on each
    /// side. A count and not a list, deliberately: listing them would claim an
    /// identity they do not have. `None` for a segment that does not read nets
    /// off references.
    pub nameless_net_pins: Option<(usize, usize)>,
    /// The M12 stability summary. `None` for a segment that draws no boxes.
    pub stability: Option<StabilityReport>,
}

impl Law {
    /// Compare two readings of this segment.
    pub fn diff(&self, a: &[Value], b: &[Value]) -> StageDiff {
        let mut unaligned = Vec::new();
        let left = self.index(a, "a", &mut unaligned);
        let right = self.index(b, "b", &mut unaligned);

        let mut changes = Vec::new();
        let mut routed_layers: BTreeSet<String> = BTreeSet::new();
        let mut stability = StabilityReport::default();

        for (key, va) in &left {
            match right.get(key) {
                None => {
                    note_route(&mut routed_layers, va);
                    changes.push(json!({
                        "type": "remove",
                        "kind": class(va),
                        "id": self.id_of(va),
                    }));
                }
                Some(vb) => {
                    let fields = self.content_of(va);
                    if let Some(delta) = delta_of(va, vb, fields) {
                        note_route(&mut routed_layers, va);
                        changes.push(json!({
                            "type": "modify",
                            "kind": class(va),
                            "id": self.id_of(va),
                            "delta": delta,
                        }));
                    }
                    if self.box_stability && class(va) == "box" {
                        let content = delta_of(va, vb, BOX_CONTENT).is_none();
                        if content {
                            stability.unchanged_boxes_total += 1;
                            let at = delta_of(va, vb, &["at"]);
                            if at.is_some() {
                                stability.unchanged_boxes_moved += 1;
                                let d = shift(va, vb);
                                if d > stability.max_unchanged_box_delta {
                                    stability.max_unchanged_box_delta = d;
                                }
                            }
                        }
                    }
                }
            }
        }
        for (key, vb) in &right {
            if !left.contains_key(key) {
                note_route(&mut routed_layers, vb);
                changes.push(json!({
                    "type": "add",
                    "kind": class(vb),
                    "id": self.id_of(vb),
                }));
            }
        }

        // Nets, for a segment that reads them off the items referencing them.
        let mut nameless_net_pins = None;
        if let Some(refs) = self.net_refs {
            let (an, a_nameless) = refs(a);
            let (bn, b_nameless) = refs(b);
            for n in an.difference(&bn) {
                changes.push(json!({ "type": "remove", "kind": "net", "id": n }));
            }
            for n in bn.difference(&an) {
                changes.push(json!({ "type": "add", "kind": "net", "id": n }));
            }
            nameless_net_pins = Some((a_nameless, b_nameless));
        }

        changes.sort_by(|x, y| {
            let kx = (
                x["kind"].as_str().unwrap_or(""),
                x["id"].as_str().unwrap_or(""),
            );
            let ky = (
                y["kind"].as_str().unwrap_or(""),
                y["id"].as_str().unwrap_or(""),
            );
            kx.cmp(&ky)
        });

        let stability = if self.box_stability {
            let mut st = stability;
            st.route_hashes_changed = routed_layers.len();
            // Declarative, and the constants are named rather than folded into
            // the arithmetic: this is a judgement about locality, not a
            // measurement.
            st.locality_warning = st.unchanged_boxes_total >= LOCALITY_MIN_SAMPLE
                && st.unchanged_boxes_moved * 2 > st.unchanged_boxes_total;
            Some(st)
        } else {
            None
        };

        StageDiff {
            changes,
            unaligned,
            nameless_net_pins,
            stability,
        }
    }

    /// The class rule for an item, if this segment has one.
    fn rule(&self, item: &Value) -> Option<&ClassSpec> {
        let c = class(item);
        self.classes.iter().find(|r| r.class == c)
    }

    /// The key, with the class folded in so two classes cannot collide.
    fn key_of(&self, item: &Value) -> Option<String> {
        let rule = self.rule(item)?;
        (rule.key)(item).map(|k| format!("{}{SEP}{k}", rule.class))
    }

    fn content_of(&self, item: &Value) -> &'static [&'static str] {
        match self.rule(item) {
            Some(r) => (r.content)(item),
            None => &[],
        }
    }

    fn id_of(&self, item: &Value) -> Value {
        match self.rule(item) {
            Some(r) => (r.id)(item),
            None => Value::Null,
        }
    }

    fn index(
        &self,
        items: &[Value],
        side: &str,
        unaligned: &mut Vec<Value>,
    ) -> BTreeMap<String, Value> {
        let mut map: BTreeMap<String, Value> = BTreeMap::new();
        for it in items {
            if self.rule(it).is_none() {
                continue;
            }
            let Some(k) = self.key_of(it) else {
                unaligned.push(json!({
                    "side": side,
                    "class": class(it),
                    "kind": it.get("kind").cloned().unwrap_or(Value::Null),
                    "reason": "no-key",
                    "item": it,
                }));
                continue;
            };
            if map.contains_key(&k) {
                // Two items claiming one key is a real shape (two routes
                // between the same pins), and O9 says mark it rather than let
                // first-arrival win.
                unaligned.push(json!({
                    "side": side,
                    "class": class(it),
                    "reason": "duplicate-key",
                    "id": self.id_of(it),
                }));
                continue;
            }
            map.insert(k, it.clone());
        }
        map
    }
}

// The `stage.viz` law

/// The law of a segment, or `None` where the segment has none.
///
/// The one place the segment → table mapping lives, so a caller that reads a
/// segment off a name (`show stage`, `diff`) and one that needs the table
/// cannot drift: a segment with no law is a segment whose items are not
/// alignable, and that answer must not depend on who is asking.
pub fn law_for(seg: StageSeg) -> Option<&'static Law> {
    match seg {
        StageSeg::P1 => None,
        StageSeg::P2 => Some(&P2_LAW),
        StageSeg::Vec => Some(&VEC_LAW),
        StageSeg::Viz => Some(&VIZ_LAW),
    }
}

/// The drawn circuit: boxes, pins, layers, the segments between them, the
/// metrics each layer reports, and the nets its pins are on.
pub const VIZ_LAW: Law = Law {
    view: DIFF_VIZ_VIEW,
    key_table: "stage.viz.keys.1",
    classes: &[
        ClassSpec {
            class: "box",
            key: canon_path_key,
            content: viz_content,
            id: canon_path_id,
        },
        ClassSpec {
            class: "layer",
            key: canon_path_key,
            content: viz_content,
            id: canon_path_id,
        },
        ClassSpec {
            class: "pin",
            key: canon_path_key,
            content: viz_content,
            id: canon_path_id,
        },
        ClassSpec {
            class: "metrics",
            key: metrics_key,
            content: viz_content,
            id: metrics_id,
        },
        ClassSpec {
            class: "segment",
            key: segment_key,
            content: viz_content,
            id: segment_id,
        },
    ],
    net_refs: Some(net_refs),
    box_stability: true,
};

/// The canonical **path** of an instance item, which is the alignment key.
///
/// The path alone, deliberately, and not with the def folded in. Two reasons,
/// both concrete:
///
/// * The path is already unique within a reading (measured: 63 boxes, 175 pins,
///   7 layers, no repeats), so the def adds nothing to alignment.
/// * Folding the def in would make a **module replacement** read as a delete
///   plus an add, when the projection family has a name for it
///   (`module-replace`) that only a `modify` can carry. The def is compared as
///   content instead -- see [`field_value`].
///
/// `def.uri` is also a *source path*, so it differs between two builds of the
/// same source tree in different directories. That would be a total false
/// churn, and it is why the def is compared by `ident` and not by uri.
fn canon_path_key(item: &Value) -> Option<String> {
    canon_path_of(item)
}

fn canon_path_id(item: &Value) -> Value {
    item.get("canon_key")
        .and_then(|ck| ck.get("path"))
        .cloned()
        .unwrap_or(Value::Null)
}

fn canon_path_of(item: &Value) -> Option<String> {
    let ck = item.get("canon_key")?;
    if ck.is_null() {
        return None;
    }
    let path = ck.get("path").and_then(Value::as_str)?;
    if path.is_empty() {
        return None;
    }
    Some(path.to_string())
}

/// A metric's `canon_key` is always null, but `(family, field, layer)` is its
/// identity. `layer` is null throughout the current emission; it is in the key
/// anyway so a future per-layer reading cannot collide.
fn metrics_key(item: &Value) -> Option<String> {
    let f = item.get("family").and_then(Value::as_str)?;
    Some(format!(
        "{f}{SEP}{}{SEP}{}",
        item.get("field").and_then(Value::as_str).unwrap_or(""),
        item.get("layer").and_then(Value::as_str).unwrap_or("")
    ))
}

fn metrics_id(item: &Value) -> Value {
    item.get("path").cloned().unwrap_or(Value::Null)
}

/// Ruling O9: a segment is not issued a number, so its key is the ordered pair
/// of its endpoints' canonical paths.
///
/// An edge has ends and no coordinates; a wire has coordinates and no ends. That
/// is why the two take different keys rather than one: for a wire, geometry *is*
/// the identity, since there is nothing else to key on.
///
/// The endpoint paths arrive already sorted by `(path, point)` from `edge_end`;
/// the sort here is what keeps the difference depending on that, because a key
/// built from an unsorted list would read one route with its lanes in another
/// order as a delete plus an add.
fn segment_key(item: &Value) -> Option<String> {
    match item.get("kind").and_then(Value::as_str) {
        Some("edge") => {
            let from = end_paths(item.get("from")?)?;
            let to = end_paths(item.get("to")?)?;
            Some(format!(
                "edge{SEP}{}{SEP}{}",
                from.join(&LIST.to_string()),
                to.join(&LIST.to_string())
            ))
        }
        // A wire segment is keyed on where it runs, because it has no ends.
        Some("wire") => {
            let from = coord(item.get("from_at")?)?;
            let to = coord(item.get("to_at")?)?;
            Some(format!(
                "wire{SEP}{}{SEP}{from}{SEP}{to}",
                item.get("net").and_then(Value::as_str).unwrap_or("")
            ))
        }
        _ => None,
    }
}

fn segment_id(item: &Value) -> Value {
    match item.get("kind").and_then(Value::as_str) {
        Some("edge") => {
            let f = end_paths(item.get("from").unwrap_or(&Value::Null)).unwrap_or_default();
            let t = end_paths(item.get("to").unwrap_or(&Value::Null)).unwrap_or_default();
            Value::String(format!("{} -> {}", f.join(","), t.join(",")))
        }
        _ => {
            let f = coord(item.get("from_at").unwrap_or(&Value::Null)).unwrap_or_default();
            let t = coord(item.get("to_at").unwrap_or(&Value::Null)).unwrap_or_default();
            Value::String(format!("{f} -> {t}"))
        }
    }
}

/// The fields compared for a matched pair of `stage.viz` items.
///
/// `loc` is excluded on purpose: it carries the **source path**, so two builds
/// of an identical source from different directories would differ in every
/// item. This difference is about the drawing, and `loc` is the way back to the
/// source, not part of what is being compared.
///
/// `"def"` is a virtual field -- see [`field_value`].
fn viz_content(item: &Value) -> &'static [&'static str] {
    match (class(item), item.get("kind").and_then(Value::as_str)) {
        ("box", _) => &[
            "def",
            "name",
            "class_name",
            "kind",
            "size",
            "pins",
            "anchors",
            "at",
            "layer",
        ],
        ("pin", _) => &[
            "def", "num", "name", "io", "side", "offset", "at", "anchors", "box", "layer", "net",
        ],
        ("layer", _) => &[
            "def", "name", "style", "parent", "boxes", "nets", "edges", "segments", "canvas",
            "audited", "reports",
        ],
        ("metrics", _) => &["value"],
        // An edge has ends but no coordinates; a wire the reverse. The ends are
        // compared as **sorted paths**, exactly as the key builds them -- the
        // emitter sorts them, and comparing the raw arrays would read one route
        // with its lanes in another order as a changed route under an unchanged
        // key.
        ("segment", Some("edge")) => &[
            "edge_kind",
            "lanes",
            "trunk",
            "ret",
            "from_paths",
            "to_paths",
            "net",
        ],
        ("segment", _) => &["net", "from_at", "to_at", "length"],
        _ => &[],
    }
}

// The `stage.p2` law

/// The flat electrical truth: the instances, the points on them, the bus
/// members and labels, and the nets.
///
/// This segment's nets are **items of their own** -- unlike the drawing, where
/// a net is only ever a reference from a pin. That is why [`net_members`] can
/// key them by member set, and why this law needs no `net_refs`: the blind spot
/// the drawing has (a pin says which net it is on only when that net has an
/// authored name) does not exist here, because a net row carries its own
/// members whatever it is called.
pub const P2_LAW: Law = Law {
    view: DIFF_P2_VIEW,
    key_table: "stage.p2.keys.1",
    classes: &[
        ClassSpec {
            class: "instance",
            key: canon_path_key,
            content: p2_content,
            id: path_or_net_id,
        },
        ClassSpec {
            class: "bus",
            key: canon_path_key,
            content: p2_content,
            id: path_or_net_id,
        },
        ClassSpec {
            class: "label",
            key: canon_path_key,
            content: p2_content,
            id: path_or_net_id,
        },
        ClassSpec {
            class: "point",
            key: canon_path_key,
            content: p2_content,
            id: path_or_net_id,
        },
        ClassSpec {
            class: "net",
            key: net_members,
            content: p2_content,
            id: path_or_net_id,
        },
    ],
    net_refs: None,
    box_stability: false,
};

/// The fields compared for a matched pair of `stage.p2` items.
///
/// * `def` resolves to the def **ident** (a virtual field), so a build from
///   another directory does not read as a module replacement.
/// * `key` (`D{n}`) and `point` (`N{n}:{k}`) are build-local ordinals, and `loc`
///   is a source path (and, for this segment, sparse).
/// * A point's `net` is a **bare name**, and for an `_net<k>` net that name is
///   numbered within its scope -- measured on `hbl`, `_net0` names four
///   different nets in four scopes. Comparing it would read a renumbering, which
///   an insertion causes, as every point in that scope having moved. The
///   membership it states is not lost: it is exactly what the net row's member
///   list carries, and that list is the net row's key.
fn p2_content(item: &Value) -> &'static [&'static str] {
    match class(item) {
        "instance" | "bus" | "label" | "point" => &["def", "class_name"],
        // A net's own **name**, and only when the emitter says the name is
        // authored (`key` non-null). An anonymous one is numbered per scope, so
        // comparing it has the same defect as comparing a point's net.
        "net" => {
            if has_authored_name(item) {
                &["net"]
            } else {
                &[]
            }
        }
        _ => &[],
    }
}

/// The handle printed on a change row: an item's canonical path, or for a net
/// the authored name followed by its members.
///
/// Shared by every law whose items carry a `path` -- which is all of them but
/// the drawing's (its items spell their path inside `canon_key`), and a net row
/// anywhere is the exception this handles.
///
/// A net's handle carries the members because they **are** its identity here, and
/// a handle that is not the identity cannot tell two rows apart. Measured: the
/// insert probe emits `remove net:GND` and `add net:GND` -- the same name on both
/// sides, because the net did not change its name, it changed its members. Two
/// rows the reader cannot tell apart is the one thing the header rule forbids for
/// two worlds differing in a single character, and it is the same defect here.
fn path_or_net_id(item: &Value) -> Value {
    if class(item) != "net" {
        return item.get("path").cloned().unwrap_or(Value::Null);
    }
    // Joined with `", "` and not with the key's `LIST` separator: that one is a
    // control character, chosen so the encoding stays injective, and printing it
    // would make four members read as one word. The design bars ANSI from a
    // readout for the same reason -- a reader has to be able to see what it
    // says.
    let members = net_member_paths(item).unwrap_or_default().join(", ");
    match item.get("key") {
        Some(k) if !k.is_null() => {
            Value::String(format!("{} {members}", k.as_str().unwrap_or_default()))
        }
        // A net with no authored name prints the members alone: there is no name
        // to lead with, and the members are still what it is.
        _ => Value::String(members),
    }
}

fn has_authored_name(item: &Value) -> bool {
    item.get("key").map(|k| !k.is_null()).unwrap_or(false)
}

/// A net's members, sorted and joined: its alignment key.
///
/// The member list is what the emitter itself names as the thing to compare
/// ("an anonymous one has no key and is matched by member-set overlap, so its
/// member list is the thing a consumer compares"), and it is a list of
/// **canonical paths**, so it survives a rebuild.
///
/// The emitter's `key` is deliberately not used instead, even when it is there:
/// measured on `hbl`, 32 of the 59 net rows carry a name but only 25 names are
/// distinct -- seven of them (`net:GND`, `net:VCC_1V2`, ...) are each carried by
/// two scopes -- so that field is a *name* rather than a key within a reading,
/// and keying on it would leave 14 rows in seven `duplicate-key` pairs. The name
/// is compared as content, which is what makes a rename a `modify` rather than a
/// delete plus an add.
///
/// The sort is idempotent with the emitter's (it already sorts), and it is here
/// so that the key is a function of the member *set*: an emitter that stopped
/// sorting would otherwise silently change what a difference means.
fn net_members(item: &Value) -> Option<String> {
    Some(net_member_paths(item)?.join(&LIST.to_string()))
}

/// The members of a net, sorted.
///
/// The sort is idempotent with the emitter's (it already sorts), and it is here
/// so that the key is a function of the member *set*: an emitter that stopped
/// sorting would otherwise silently change what a difference means.
fn net_member_paths(item: &Value) -> Option<Vec<String>> {
    let arr = item.get("members")?.as_array()?;
    if arr.is_empty() {
        // A net with no points has nothing to be keyed on. The design names
        // this blind spot ("a net that is declared but carries no pins"); it
        // lands in `unaligned` rather than being given a synthesised key.
        return None;
    }
    let mut out = Vec::with_capacity(arr.len());
    for m in arr {
        let p = m.as_str()?;
        if p.is_empty() {
            return None;
        }
        out.push(p.to_string());
    }
    out.sort();
    Some(out)
}

// The `stage.vec` law

/// The vector graph: the layers, the boxes in them, the endpoints on their
/// nets, the trunks, the nets themselves, and the projection's own log.
///
/// This is the first segment whose objects are **not all instances** (design
/// §2.4): a layer is a `bid`, a box an instance, an endpoint a pin, a net a
/// member set, and a trunk owns no id at all. So this table is the first with
/// three different key functions in it, one per kind of identity, rather than
/// one path key with a net-shaped exception.
///
/// Its nets are items of their own -- keyed on the member set, exactly as
/// `stage.p2` keys the same nets -- so like that segment it reads no nets off
/// references and publishes no `nameless_net_pins`. And it draws no boxes in the
/// drawing sense: a `box` here has no position at all, so there is nothing for a
/// stability summary to measure and the law makes no such claim.
pub const VEC_LAW: Law = Law {
    view: DIFF_VEC_VIEW,
    key_table: "stage.vec.keys.1",
    classes: &[
        ClassSpec {
            class: "box",
            key: canon_path_key,
            content: vec_content,
            id: path_or_net_id,
        },
        ClassSpec {
            class: "layer",
            key: canon_path_key,
            content: vec_content,
            id: path_or_net_id,
        },
        ClassSpec {
            class: "endpoint",
            key: canon_path_key,
            content: vec_content,
            id: path_or_net_id,
        },
        ClassSpec {
            class: "net",
            key: net_members,
            content: vec_content,
            id: path_or_net_id,
        },
        ClassSpec {
            class: "trunk",
            key: item_path_key,
            content: vec_content,
            id: path_or_net_id,
        },
        ClassSpec {
            class: "projection",
            key: projection_key,
            content: vec_content,
            id: path_or_net_id,
        },
    ],
    net_refs: None,
    box_stability: false,
};

/// The fields compared for a matched pair of `stage.vec` items.
///
/// `key` (`D{n}`) and `point` (`N{n}:{k}`) are build-local ordinals here as
/// everywhere, and `loc` is a source path; none of the three is compared. Three
/// exclusions are this segment's own:
///
/// * An **endpoint's `net`** is the net's bare **name**, and on `hbl` 84 of the
///   186 endpoints sit on a name the builder minted (`_net<k>`). Comparing it
///   would read a renumbering -- which any insertion causes -- as that endpoint
///   having moved. The membership it states is not lost: it is exactly what the
///   net row's member list carries, and that list is the net row's key. This is
///   the same exclusion `stage.p2` makes for a point's `net`, for the same
///   measured reason.
/// * A **trunk's `lanes`** are compared as a **set** ([`lane_pairs`]), because
///   the lane index is assigned either from the source's bracket lane or, when
///   the source gave none, from the member's position in the trunk.
/// * A **projection record's `note`** is prose that embeds a builder id
///   (`port_group_id`, resolved from the block structure), so comparing it would
///   compare an ordinal assigned during construction. The row's identity and its
///   rule are compared; its prose is not.
fn vec_content(item: &Value) -> &'static [&'static str] {
    match class(item) {
        "box" => &["def", "name", "class_name", "kind", "pins", "layer"],
        "layer" => &["def", "name", "style", "boxes", "nets", "root"],
        "endpoint" => &["def", "pin", "io", "layer"],
        // A net's own name, only when the emitter says the name is the
        // source's -- the same test `stage.p2` makes, and the `origin`
        // classification with it, since `source` / `segment` / `anonymous` is
        // a three-valued reading and not a name.
        "net" => {
            if has_authored_name(item) {
                &["name", "origin", "kind", "role", "endpoints", "layer"]
            } else {
                &["origin", "kind", "role", "endpoints", "layer"]
            }
        }
        "trunk" => &["kind", "op", "dir", "lane_pairs"],
        // The per-layer row's whole substance is the count pair; an action
        // record's is its rule (its `path` is its key).
        "projection" => {
            if is_projection_layer_row(item) {
                &["before", "after"]
            } else {
                &["rule"]
            }
        }
        _ => &[],
    }
}

/// The alignment key of an item whose identity is the `path` the emitter gave
/// it. A trunk is the one class here that owns no id at all: design §2.4 says
/// so in as many words -- "a trunk owns no id. Its name and its lanes are what
/// it is" -- and the path is that name qualified by its layer. Measured unique
/// within the reading on every board measured (16 of 16 on `hbl`, 16 of 16 on
/// `hbl1`, 39 of 39 on `hs`); two trunks of one name in one layer is not a shape
/// the emitter produces, and if it ever does, the core reports the collision
/// rather than letting first arrival win.
fn item_path_key(item: &Value) -> Option<String> {
    let p = item.get("path").and_then(Value::as_str)?;
    if p.is_empty() {
        return None;
    }
    Some(p.to_string())
}

/// A projection row's key.
///
/// The emitter puts two structurally different rows under this one class: one
/// per layer, carrying the net count before and after the projection, and one
/// per action it took. The count pair is what tells them apart -- only the
/// per-layer row carries it ([`is_projection_layer_row`]).
///
/// A per-layer row's identity is its layer. An action record's is the layer, the
/// net and the endpoint it acted on, published as fields of their own for this
/// reason: the row's identity cannot be stated by splitting the `path` the
/// emitter also publishes, because recovering structure from a formatted string
/// is what this project forbids everywhere else.
///
/// **A net name the builder minted is refused, not keyed.** `net_origin` states
/// the rule in its own words -- a minted name is unique to this build's
/// segmentation, so "keying on it would claim a stability the name does not
/// have". Such a row lands in `unaligned`, the same answer the drawing gives a
/// pin whose `canon_key` is null. Measured: 4 of 58 records on `hbl`, 0 of 62 on
/// `hbl1`, 2 of 124 on `hs`.
fn projection_key(item: &Value) -> Option<String> {
    let layer = field_str(item, "layer")?;
    if is_projection_layer_row(item) {
        return Some(format!("nets{SEP}{layer}"));
    }
    let net = field_str(item, "net")?;
    if net_origin(net) != NetOrigin::Source {
        return None;
    }
    let endpoint = item.get("endpoint").and_then(Value::as_str).unwrap_or("");
    Some(format!("act{SEP}{layer}{SEP}{net}{SEP}{endpoint}"))
}

/// Whether a `projection` row is the per-layer one.
///
/// Structural, not a spelling test: the emitter sets the count pair on that row
/// and sets both to null on an action record. A row with `before: 0` is still a
/// per-layer row -- the test is the pair's presence, not its value.
fn is_projection_layer_row(item: &Value) -> bool {
    item.get("before").map(|v| !v.is_null()).unwrap_or(false)
}

/// A non-empty string field, or `None`.
fn field_str<'a>(item: &'a Value, name: &str) -> Option<&'a str> {
    let s = item.get(name)?.as_str()?;
    if s.is_empty() {
        return None;
    }
    Some(s)
}

// Shared helpers

/// Note the layer a changed segment belongs to, for the layer-level reroute
/// count. A segment that appeared or vanished counts as much as one that
/// changed: all three mean the layer's routing is not what it was.
fn note_route(layers: &mut BTreeSet<String>, item: &Value) {
    if class(item) != "segment" {
        return;
    }
    if let Some(l) = item.get("layer").and_then(Value::as_str) {
        layers.insert(l.to_string());
    }
}

/// Content fields of a box, excluding its position -- position is what
/// `unchanged_boxes_moved` is about, so a box that only moved is still
/// "unchanged" as far as this list is concerned.
const BOX_CONTENT: &[&str] = &[
    "def",
    "name",
    "class_name",
    "kind",
    "size",
    "pins",
    "anchors",
    "layer",
];

fn class(item: &Value) -> &str {
    item.get("class").and_then(Value::as_str).unwrap_or("")
}

/// One end's canonical paths, sorted. `None` when any member has no path -- an
/// end the table could not resolve, which O9 marks rather than joins.
fn end_paths(v: &Value) -> Option<Vec<String>> {
    let arr = v.as_array()?;
    if arr.is_empty() {
        return None;
    }
    let mut out = Vec::with_capacity(arr.len());
    for e in arr {
        let p = e.get("path").and_then(Value::as_str)?;
        if p.is_empty() {
            return None;
        }
        out.push(p.to_string());
    }
    out.sort();
    Some(out)
}

fn coord(v: &Value) -> Option<String> {
    let a = v.as_array()?;
    if a.len() != 2 {
        return None;
    }
    Some(format!("{},{}", a[0].as_f64()?, a[1].as_f64()?))
}

/// Read one compared field.
///
/// Four names are virtual:
///
/// * `"def"` resolves to the def **ident** rather than the whole `def` object,
///   so a build from a different directory (a different `def.uri`) does not read
///   as a module replacement.
/// * `"from_paths"` / `"to_paths"` resolve to a segment's sorted endpoint paths,
///   so the comparison agrees with the key about what a lane reorder is.
/// * `"lane_pairs"` resolves a trunk's lanes to a **set** of members, dropping
///   the lane index. The producer assigns that index either from the source's
///   bracket lane or, when the source gave none, from "the member's position
///   within the trunk" (`vector/builder/visit.rs`) -- a position in this build's
///   append order, the same family as `key` and `index`. Comparing the raw array
///   would make one inserted member read as every later lane having changed.
fn field_value(item: &Value, f: &str) -> Value {
    match f {
        "def" => item
            .get("canon_key")
            .and_then(|c| c.get("def"))
            .and_then(|d| d.get("ident"))
            .cloned()
            .unwrap_or(Value::Null),
        "from_paths" => sorted_paths(item, "from"),
        "to_paths" => sorted_paths(item, "to"),
        "lane_pairs" => lane_pairs(item),
        _ => item.get(f).cloned().unwrap_or(Value::Null),
    }
}

/// A trunk's lanes as a set: each lane's member and its two pin paths, sorted,
/// with the lane index left out. See [`field_value`] for why the index is not a
/// member of the set.
fn lane_pairs(item: &Value) -> Value {
    let Some(arr) = item.get("lanes").and_then(Value::as_array) else {
        return Value::Null;
    };
    let mut out: Vec<Value> = arr
        .iter()
        .map(|l| {
            json!({
                "member": l.get("member").cloned().unwrap_or(Value::Null),
                "left": l.get("left").cloned().unwrap_or(Value::Null),
                "right": l.get("right").cloned().unwrap_or(Value::Null),
            })
        })
        .collect();
    // Sorted by the rendered object, which is a total order because the map is
    // key-sorted -- `serde_json` is built without `preserve_order`.
    out.sort_by_key(|v| v.to_string());
    Value::Array(out)
}

fn sorted_paths(item: &Value, side: &str) -> Value {
    match item.get(side).and_then(end_paths) {
        Some(ps) => Value::Array(ps.into_iter().map(Value::String).collect()),
        None => Value::Null,
    }
}

/// `Some(delta)` when any listed field differs, `None` when none does.
fn delta_of(a: &Value, b: &Value, fields: &[&str]) -> Option<Value> {
    let mut m = serde_json::Map::new();
    for f in fields {
        let va = field_value(a, f);
        let vb = field_value(b, f);
        if va != vb {
            m.insert((*f).to_string(), json!({ "from": va, "to": vb }));
        }
    }
    if m.is_empty() {
        None
    } else {
        Some(Value::Object(m))
    }
}

/// How far a box moved, in drawing units.
fn shift(a: &Value, b: &Value) -> f64 {
    let p = |v: &Value| -> Option<(f64, f64)> {
        let arr = v.get("at")?.as_array()?;
        Some((arr.first()?.as_f64()?, arr.get(1)?.as_f64()?))
    };
    match (p(a), p(b)) {
        (Some((x0, y0)), Some((x1, y1))) => ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt(),
        _ => 0.0,
    }
}

/// The nets referenced by pins, split into the ones that have a cross-build key
/// and a count of the pins on the ones that do not.
fn net_refs(items: &[Value]) -> (BTreeSet<String>, usize) {
    let mut named = BTreeSet::new();
    let mut nameless = 0usize;
    for it in items {
        if class(it) != "pin" {
            continue;
        }
        match it.get("net").and_then(Value::as_str) {
            Some(n) if !n.is_empty() => {
                named.insert(n.to_string());
            }
            // `net: null` with a `nid` present is a segment-minted or anonymous
            // net: it exists, but it has no name to be keyed on.
            _ => {
                if it.get("nid").map(|v| !v.is_null()).unwrap_or(false) {
                    nameless += 1;
                }
            }
        }
    }
    (named, nameless)
}

// The text face

/// Render a difference for a human: a header naming the two worlds, a counts
/// line, then the rows.
///
/// It obeys the four prohibitions every readout in this family obeys (design
/// §5.3): no ANSI, no box drawing, no tab-delimited columns, and a missing value
/// printed as `-`. It is a **readout**, not a gate -- a non-empty difference is
/// the answer, not a failure, so nothing here can fail the run.
///
/// The header prints both `world_ver` tokens in full. They are tokens meant to
/// be compared, and two worlds that differ only in one character must not be
/// made to look alike (the same reason `StageView::header_line` prints them
/// whole).
///
/// The `nameless` pair is printed apart from the change counts, and only by a
/// segment that has one: it is how many references sit on nets with no
/// cross-build key, one number per side. A self-difference of the drawing prints
/// `73/73` here and zeros above, and that pairing is the honest reading -- there
/// are pins this comparison cannot speak about, and nothing about them changed.
pub fn render_diff_text(view: &str, a: &StageView, b: &StageView, d: &StageDiff) -> String {
    let mut out = Vec::new();
    out.push(format!("{view} {}", token_line(a, b)));
    let mut counts = format!(
        "# remove {}  add {}  modify {}  unaligned {}  changes {}",
        count_of(d, "remove"),
        count_of(d, "add"),
        count_of(d, "modify"),
        d.unaligned.len(),
        d.changes.len(),
    );
    if let Some((an, bn)) = d.nameless_net_pins {
        counts.push_str(&format!("  nameless {an}/{bn}"));
    }
    out.push(counts);
    for c in &d.changes {
        out.push(format!(
            "{:<8}  {:<9}  {:<40}  {}",
            c["type"].as_str().unwrap_or("-"),
            c["kind"].as_str().unwrap_or("-"),
            c["id"].as_str().unwrap_or("-"),
            delta_cell(c),
        ));
    }
    for u in &d.unaligned {
        out.push(format!(
            "{:<8}  {:<9}  {:<40}  {}",
            "unaligned",
            u["class"].as_str().unwrap_or("-"),
            u["id"].as_str().unwrap_or("-"),
            u["reason"].as_str().unwrap_or("-"),
        ));
    }
    out.join("\n")
}

/// Which two worlds this is a difference of, and whether either was
/// fingerprintable at all. A missing token prints as `-`, like every other
/// missing value in a readout.
fn token_line(a: &StageView, b: &StageView) -> String {
    format!(
        "top={}  a={}  b={}",
        a.top,
        a.world_ver.as_deref().unwrap_or("-"),
        b.world_ver.as_deref().unwrap_or("-"),
    )
}

fn count_of(d: &StageDiff, t: &str) -> usize {
    d.changes.iter().filter(|c| c["type"] == t).count()
}

/// Which fields a `modify` changed, comma-joined.
///
/// The **names** and not the values: the values are already in the envelope, and
/// a readout whose lines change width with the data is the thing the fixed-width
/// rule exists to prevent. A change with no delta prints `-`.
fn delta_cell(c: &Value) -> String {
    let Some(m) = c.get("delta").and_then(Value::as_object) else {
        return "-".to_string();
    };
    if m.is_empty() {
        return "-".to_string();
    }
    let mut keys: Vec<&str> = m.keys().map(String::as_str).collect();
    keys.sort_unstable();
    keys.join(",")
}

// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Drawing-side core data structures
//!
//! - [`vec`]   —— [`McVec`] (endpoint vector)
//! - [`net`]   —— [`McVecNet`] (electrical net) + [`ConnectionType`] (topology type)
//! - [`block`] —— [`McVecBlock`] (hierarchical block)

pub mod block;
pub mod net;
pub mod netshape;
pub mod trunk;
pub mod vec;

// Top-level exports, users can write `use crate::vector::model::McVec;`
/// Arrow direction type (semantic/common.rs), unified with the former
/// vector-layer `PairDir` (vec-dianlu.md §8.9.7-F).
pub use crate::semantic::common::ConnDir;
pub use block::McVecBlock;
pub use net::{BoundaryInfo, ConnectionType, McVecNet, RailClass, RailSpec};
pub use netshape::{GroupRole, LaneRef, NetShape, ShapeStats};
pub use trunk::{MemberLane, PathSegment, Trunk, TrunkEnd, TrunkKind, TrunkRef};
pub use vec::McVec;

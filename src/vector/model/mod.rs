// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Drawing-side core data structures
//!
//! - [`vec`]   —— [`McVec`] (endpoint vector)
//! - [`net`]   —— [`McVecNet`] (electrical net) + [`ConnectionType`] (topology type)
//! - [`block`] —— [`McVecBlock`] (hierarchical block)

pub mod block;
pub mod link;
pub mod net;
pub mod netshape;
pub mod vec;

// Top-level exports, users can write `use crate::vector::model::McVec;`
pub use block::McVecBlock;
pub use link::{LinkEnd, LinkKind, LinkRef, MemberLane, PortLink};
pub use net::{BoundaryInfo, ConnectionType, McVecNet, RailClass, RailSpec};
pub use netshape::{GroupRole, LaneRef, NetShape, PairDir, ShapeStats};
pub use vec::McVec;

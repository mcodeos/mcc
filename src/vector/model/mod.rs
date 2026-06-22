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
pub mod vec;

// Top-level exports, users can write `use crate::vector::model::McVec;`
pub use block::McVecBlock;
pub use net::{ConnectionType, McVecNet};
pub use vec::McVec;

//! Architecture canvas engine support: auto-layout.
//!
//! The canvas data shapes (graph + layout overlay) live in
//! `mewcode_protocol::canvas`. This module provides the engine-side
//! auto-layout: given a [`mewcode_protocol::canvas::Graph`] and any
//! already-pinned node positions, [`layout::auto_layout`] fills in
//! the rest as a deterministic row-major grid.

pub mod io;
pub mod layout;

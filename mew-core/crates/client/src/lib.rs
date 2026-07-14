//! mewcode terminal UI ([ratatui](https://docs.rs/ratatui/latest/ratatui/)).

#![forbid(unsafe_code)]

pub mod config;
pub mod net;
pub mod runtime;

pub use config::ClientConfig;
pub use runtime::run;

//! mewcode terminal UI (ratatui).

#![forbid(unsafe_code)]

pub mod config;
pub mod net;
pub mod runtime;

pub use config::ClientConfig;
pub use runtime::run;

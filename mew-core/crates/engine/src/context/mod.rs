//! Model context sources: transcript history and durable memory.

mod history;
mod memory;

pub use history::{HistoryStrategy, text_of};
pub use memory::MemoryStore;

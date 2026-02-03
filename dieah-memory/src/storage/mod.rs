//! Storage backends for dieah-memory

mod jsonl;
mod sqlite;
pub mod vector;

pub use jsonl::JsonlStorage;
pub use sqlite::SqliteStorage;
pub use vector::{SearchResult, VectorStorage};

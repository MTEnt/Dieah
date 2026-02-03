//! # Dieah Memory
//!
//! A production-ready memory and context management system for AI agents.
//!
//! ## Architecture
//!
//! The memory system has multiple layers:
//! - **Layer 1: Conversation History** - JSONL append-only logs
//! - **Layer 2: Learned Corrections** - Vector-indexed memories from user feedback
//! - **Layer 3: Topic Context** - Per-topic metadata and context
//! - **Layer 4: Agent Preferences** - Per-agent learned behaviors
//!
//! ## Usage
//!
//! ```rust,ignore
//! use dieah_memory::{MemoryStore, Config};
//!
//! let config = Config::default();
//! let store = MemoryStore::new(config).await?;
//!
//! // Store a message
//! store.append_message(agent_id, topic_id, message).await?;
//!
//! // Store a learned correction
//! store.save_memory(memory).await?;
//!
//! // Retrieve relevant context for a query
//! let context = store.retrieve_context(query, agent_id, topic_id).await?;
//! ```

pub mod config;
pub mod embedding;
pub mod error;
pub mod memory;
pub mod message;
pub mod retrieval;
pub mod storage;

pub use config::Config;
pub use error::{Error, Result};
pub use memory::{Memory, MemoryScope, MemoryStore};
pub use message::{Message, Role};
pub use retrieval::RetrievalContext;

//! Memory types for learned corrections and preferences

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config::Config;
use crate::error::Result;
use crate::storage::{JsonlStorage, SqliteStorage, VectorStorage};

/// Scope of a memory
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemoryScope {
    /// Global memory - applies to all agents and topics
    Global,

    /// Agent-specific memory - applies to a specific agent
    Agent,

    /// Topic-specific memory - applies to a specific topic within an agent
    Topic,

    /// Personal memory - user-specific preferences
    Personal,
}

impl std::fmt::Display for MemoryScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MemoryScope::Global => write!(f, "global"),
            MemoryScope::Agent => write!(f, "agent"),
            MemoryScope::Topic => write!(f, "topic"),
            MemoryScope::Personal => write!(f, "personal"),
        }
    }
}

/// Type of memory
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryType {
    /// A correction the user made to agent behavior
    Correction,

    /// A learned preference
    Preference,

    /// A fact or piece of knowledge
    Fact,

    /// A workflow or process
    Workflow,

    /// A constraint or rule
    Constraint,
}

impl std::fmt::Display for MemoryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MemoryType::Correction => write!(f, "correction"),
            MemoryType::Preference => write!(f, "preference"),
            MemoryType::Fact => write!(f, "fact"),
            MemoryType::Workflow => write!(f, "workflow"),
            MemoryType::Constraint => write!(f, "constraint"),
        }
    }
}

/// A learned memory that persists across conversations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    /// Unique memory ID
    pub id: Uuid,

    /// Scope of this memory
    pub scope: MemoryScope,

    /// Type of memory
    pub memory_type: MemoryType,

    /// Agent ID (if scope is Agent or Topic)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,

    /// Topic ID (if scope is Topic)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic_id: Option<String>,

    /// The memory content - what was learned
    pub content: String,

    /// Context that triggered this memory (original conversation snippet)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,

    /// Tags for categorization
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    /// Embedding vector (populated after embedding)
    #[serde(skip)]
    pub embedding: Option<Vec<f32>>,

    /// When the memory was created
    pub created_at: DateTime<Utc>,

    /// When the memory was last used
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<DateTime<Utc>>,

    /// How many times this memory has been retrieved
    #[serde(default)]
    pub retrieval_count: u32,

    /// Whether this memory is active
    #[serde(default = "default_true")]
    pub active: bool,
}

fn default_true() -> bool {
    true
}

impl Memory {
    /// Create a new global memory
    pub fn global(memory_type: MemoryType, content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            scope: MemoryScope::Global,
            memory_type,
            agent_id: None,
            topic_id: None,
            content: content.into(),
            context: None,
            tags: Vec::new(),
            embedding: None,
            created_at: Utc::now(),
            last_used_at: None,
            retrieval_count: 0,
            active: true,
        }
    }

    /// Create a new agent-scoped memory
    pub fn for_agent(
        agent_id: impl Into<String>,
        memory_type: MemoryType,
        content: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            scope: MemoryScope::Agent,
            memory_type,
            agent_id: Some(agent_id.into()),
            topic_id: None,
            content: content.into(),
            context: None,
            tags: Vec::new(),
            embedding: None,
            created_at: Utc::now(),
            last_used_at: None,
            retrieval_count: 0,
            active: true,
        }
    }

    /// Create a new topic-scoped memory
    pub fn for_topic(
        agent_id: impl Into<String>,
        topic_id: impl Into<String>,
        memory_type: MemoryType,
        content: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            scope: MemoryScope::Topic,
            memory_type,
            agent_id: Some(agent_id.into()),
            topic_id: Some(topic_id.into()),
            content: content.into(),
            context: None,
            tags: Vec::new(),
            embedding: None,
            created_at: Utc::now(),
            last_used_at: None,
            retrieval_count: 0,
            active: true,
        }
    }

    /// Add context to the memory
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }

    /// Add tags to the memory
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Set the embedding
    pub fn with_embedding(mut self, embedding: Vec<f32>) -> Self {
        self.embedding = Some(embedding);
        self
    }

    /// Mark the memory as used
    pub fn mark_used(&mut self) {
        self.last_used_at = Some(Utc::now());
        self.retrieval_count += 1;
    }
}

/// The main memory store that coordinates all storage backends
pub struct MemoryStore {
    config: Config,
    sqlite: SqliteStorage,
    vector: VectorStorage,
    jsonl: JsonlStorage,
}

impl MemoryStore {
    /// Create a new memory store
    pub async fn new(config: Config) -> Result<Self> {
        config.ensure_dirs()?;

        let sqlite = SqliteStorage::new(&config)?;
        let vector = VectorStorage::new(&config).await?;
        let jsonl = JsonlStorage::new(&config)?;

        Ok(Self {
            config,
            sqlite,
            vector,
            jsonl,
        })
    }

    /// Get the configuration
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Get the SQLite storage
    pub fn sqlite(&self) -> &SqliteStorage {
        &self.sqlite
    }

    /// Get the vector storage
    pub fn vector(&self) -> &VectorStorage {
        &self.vector
    }

    /// Get the JSONL storage
    pub fn jsonl(&self) -> &JsonlStorage {
        &self.jsonl
    }

    /// Save a memory to all relevant stores
    pub async fn save_memory(&self, memory: Memory) -> Result<Memory> {
        // Save to SQLite for metadata
        self.sqlite.save_memory(&memory)?;

        // Save to vector store if we have an embedding
        if memory.embedding.is_some() {
            self.vector.upsert_memory(&memory).await?;
        }

        Ok(memory)
    }

    /// Get a memory by ID
    pub fn get_memory(&self, id: Uuid) -> Result<Option<Memory>> {
        self.sqlite.get_memory(id)
    }

    /// List memories with optional filters
    pub fn list_memories(
        &self,
        scope: Option<MemoryScope>,
        agent_id: Option<&str>,
        topic_id: Option<&str>,
        active_only: bool,
    ) -> Result<Vec<Memory>> {
        self.sqlite.list_memories(scope, agent_id, topic_id, active_only)
    }

    /// Delete a memory
    pub async fn delete_memory(&self, id: Uuid) -> Result<()> {
        self.sqlite.delete_memory(id)?;
        self.vector.delete_memory(id).await?;
        Ok(())
    }

    /// Deactivate a memory (soft delete)
    pub fn deactivate_memory(&self, id: Uuid) -> Result<()> {
        self.sqlite.set_memory_active(id, false)
    }

    /// Reactivate a memory
    pub fn reactivate_memory(&self, id: Uuid) -> Result<()> {
        self.sqlite.set_memory_active(id, true)
    }
}

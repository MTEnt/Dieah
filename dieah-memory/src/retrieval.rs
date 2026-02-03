//! Context retrieval for RAG-style memory injection

use crate::config::Config;
use crate::embedding::EmbeddingService;
use crate::error::Result;
use crate::memory::{Memory, MemoryStore};
use crate::message::Message;
use crate::storage::vector::SearchResult;

/// Retrieved context ready for injection into prompts
#[derive(Debug, Clone)]
pub struct RetrievalContext {
    /// Relevant memories from the vector store
    pub memories: Vec<RetrievedMemory>,
    
    /// Recent conversation history
    pub recent_messages: Vec<Message>,
    
    /// Total tokens in this context
    pub total_tokens: u32,
}

impl RetrievalContext {
    /// Create an empty context
    pub fn empty() -> Self {
        Self {
            memories: Vec::new(),
            recent_messages: Vec::new(),
            total_tokens: 0,
        }
    }

    /// Check if context is empty
    pub fn is_empty(&self) -> bool {
        self.memories.is_empty() && self.recent_messages.is_empty()
    }

    /// Format the context for injection into a prompt
    pub fn format_for_prompt(&self) -> String {
        let mut parts = Vec::new();

        if !self.memories.is_empty() {
            parts.push("## Relevant Memories\n".to_string());
            for memory in &self.memories {
                parts.push(format!(
                    "- [{}] {}\n",
                    memory.memory_type, memory.content
                ));
            }
        }

        if !self.recent_messages.is_empty() {
            parts.push("\n## Recent Conversation Context\n".to_string());
            for msg in &self.recent_messages {
                parts.push(format!("{}: {}\n", msg.role, msg.content));
            }
        }

        parts.join("")
    }
}

/// A memory that was retrieved with its relevance score
#[derive(Debug, Clone)]
pub struct RetrievedMemory {
    pub id: uuid::Uuid,
    pub content: String,
    pub scope: String,
    pub memory_type: String,
    pub score: f32,
}

impl From<SearchResult> for RetrievedMemory {
    fn from(result: SearchResult) -> Self {
        Self {
            id: result.id,
            content: result.content,
            scope: result.scope,
            memory_type: result.memory_type,
            score: result.score,
        }
    }
}

/// Retrieval engine for fetching relevant context
pub struct RetrievalEngine {
    embedding_service: EmbeddingService,
    config: Config,
}

impl RetrievalEngine {
    /// Create a new retrieval engine
    pub fn new(config: Config) -> Result<Self> {
        let embedding_service = EmbeddingService::new(&config)?;
        Ok(Self {
            embedding_service,
            config,
        })
    }

    /// Retrieve context for a query
    pub async fn retrieve(
        &self,
        store: &MemoryStore,
        query: &str,
        agent_id: Option<&str>,
        topic_id: Option<&str>,
        max_recent_messages: usize,
    ) -> Result<RetrievalContext> {
        // Generate embedding for the query
        let query_embedding = self.embedding_service.embed(query).await?;

        // Search for relevant memories
        let mut memories = Vec::new();

        // Search global memories first
        let global_results = store
            .vector()
            .search(
                &query_embedding,
                self.config.max_retrieval_results / 2,
                self.config.min_similarity_score,
                Some("global"),
                None,
            )
            .await?;
        memories.extend(global_results.into_iter().map(RetrievedMemory::from));

        // Search agent-specific memories if agent_id provided
        if let Some(aid) = agent_id {
            let agent_results = store
                .vector()
                .search(
                    &query_embedding,
                    self.config.max_retrieval_results / 2,
                    self.config.min_similarity_score,
                    Some("agent"),
                    Some(aid),
                )
                .await?;
            memories.extend(agent_results.into_iter().map(RetrievedMemory::from));
        }

        // Sort by score and deduplicate
        memories.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        memories.truncate(self.config.max_retrieval_results);

        // Get recent messages if topic provided
        let recent_messages = if let (Some(aid), Some(tid)) = (agent_id, topic_id) {
            store.jsonl().read_last_n(aid, tid, max_recent_messages)?
        } else {
            Vec::new()
        };

        // Calculate total tokens (rough estimate)
        let memory_tokens: u32 = memories
            .iter()
            .map(|m| (m.content.len() / 4) as u32)
            .sum();
        let message_tokens: u32 = recent_messages.iter().map(|m| m.tokens).sum();

        Ok(RetrievalContext {
            memories,
            recent_messages,
            total_tokens: memory_tokens + message_tokens,
        })
    }

    /// Embed and save a memory
    pub async fn embed_and_save(&self, store: &MemoryStore, mut memory: Memory) -> Result<Memory> {
        // Generate embedding for the memory content
        let embedding = self.embedding_service.embed(&memory.content).await?;
        memory.embedding = Some(embedding);

        // Save to store
        store.save_memory(memory).await
    }

    /// Detect if a message contains a correction
    pub fn detect_correction(&self, user_message: &str, assistant_message: &str) -> Option<String> {
        let correction_indicators = [
            "no,",
            "no that's",
            "that's wrong",
            "that's not",
            "actually,",
            "actually ",
            "incorrect",
            "not quite",
            "you're wrong",
            "wrong,",
            "nope,",
            "i meant",
            "what i meant",
            "let me clarify",
            "to clarify",
            "correction:",
            "i should have said",
            "remember that",
            "don't forget",
            "always ",
            "never ",
            "make sure to",
            "please remember",
        ];

        let user_lower = user_message.to_lowercase();
        
        for indicator in correction_indicators {
            if user_lower.starts_with(indicator) || user_lower.contains(&format!(" {}", indicator)) {
                // This looks like a correction, extract the key insight
                return Some(format!(
                    "User corrected: \"{}\"\nOriginal context: \"{}\"",
                    user_message,
                    assistant_message.chars().take(200).collect::<String>()
                ));
            }
        }

        None
    }

    /// Suggest saving a memory from a correction
    pub fn suggest_memory_from_correction(
        &self,
        user_message: &str,
        agent_id: &str,
    ) -> Option<Memory> {
        // Simple heuristic: if the message contains correction-like patterns
        let correction_patterns = [
            ("always ", "preference"),
            ("never ", "constraint"),
            ("remember ", "fact"),
            ("don't forget", "fact"),
            ("i prefer", "preference"),
            ("i like", "preference"),
            ("i don't like", "preference"),
            ("make sure", "workflow"),
            ("when you", "workflow"),
        ];

        let user_lower = user_message.to_lowercase();
        
        for (pattern, memory_type) in correction_patterns {
            if user_lower.contains(pattern) {
                let mtype = match memory_type {
                    "preference" => crate::memory::MemoryType::Preference,
                    "constraint" => crate::memory::MemoryType::Constraint,
                    "fact" => crate::memory::MemoryType::Fact,
                    "workflow" => crate::memory::MemoryType::Workflow,
                    _ => crate::memory::MemoryType::Correction,
                };
                
                return Some(Memory::for_agent(agent_id, mtype, user_message));
            }
        }

        None
    }
}

/// Context budget manager for tracking token usage
pub struct ContextBudget {
    pub limit: u32,
    pub used: u32,
    pub warning_threshold: f32,
    pub critical_threshold: f32,
}

impl ContextBudget {
    /// Create a new context budget
    pub fn new(limit: u32, warning_threshold: f32, critical_threshold: f32) -> Self {
        Self {
            limit,
            used: 0,
            warning_threshold,
            critical_threshold,
        }
    }

    /// Add tokens to the budget
    pub fn add(&mut self, tokens: u32) {
        self.used += tokens;
    }

    /// Get utilization percentage
    pub fn utilization(&self) -> f32 {
        self.used as f32 / self.limit as f32
    }

    /// Check if at warning level
    pub fn is_warning(&self) -> bool {
        self.utilization() >= self.warning_threshold
    }

    /// Check if at critical level
    pub fn is_critical(&self) -> bool {
        self.utilization() >= self.critical_threshold
    }

    /// Get remaining tokens
    pub fn remaining(&self) -> u32 {
        self.limit.saturating_sub(self.used)
    }

    /// Get status string
    pub fn status(&self) -> &'static str {
        if self.is_critical() {
            "critical"
        } else if self.is_warning() {
            "warning"
        } else {
            "ok"
        }
    }
}

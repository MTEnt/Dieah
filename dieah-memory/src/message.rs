//! Message types for conversation history

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Role of a message sender
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Role::System => write!(f, "system"),
            Role::User => write!(f, "user"),
            Role::Assistant => write!(f, "assistant"),
            Role::Tool => write!(f, "tool"),
        }
    }
}

/// A single message in a conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Unique message ID
    pub id: Uuid,

    /// Agent this message belongs to
    pub agent_id: String,

    /// Topic/thread this message belongs to
    pub topic_id: String,

    /// Role of the sender
    pub role: Role,

    /// Message content
    pub content: String,

    /// Token count for this message
    pub tokens: u32,

    /// Timestamp when the message was created
    pub timestamp: DateTime<Utc>,

    /// Optional metadata (tool calls, thinking, etc.)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<MessageMetadata>,
}

impl Message {
    /// Create a new message
    pub fn new(
        agent_id: impl Into<String>,
        topic_id: impl Into<String>,
        role: Role,
        content: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            agent_id: agent_id.into(),
            topic_id: topic_id.into(),
            role,
            content: content.into(),
            tokens: 0, // Will be calculated later
            timestamp: Utc::now(),
            metadata: None,
        }
    }

    /// Set the token count
    pub fn with_tokens(mut self, tokens: u32) -> Self {
        self.tokens = tokens;
        self
    }

    /// Set metadata
    pub fn with_metadata(mut self, metadata: MessageMetadata) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

/// Optional metadata for a message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageMetadata {
    /// Tool calls made in this message
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,

    /// Model's thinking/reasoning (if captured)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,

    /// Model used for this response
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// Whether this message triggered a memory save
    #[serde(default)]
    pub triggered_memory: bool,
}

/// A tool call within a message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Tool call ID
    pub id: String,

    /// Tool name
    pub name: String,

    /// Tool input (as JSON)
    pub input: serde_json::Value,

    /// Tool output (as JSON)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<serde_json::Value>,

    /// Status of the tool call
    pub status: ToolStatus,
}

/// Status of a tool call
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolStatus {
    Pending,
    Running,
    Success,
    Error,
}

/// Summary of token usage for a topic
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Total tokens used
    pub total: u32,

    /// Tokens used by system messages
    pub system: u32,

    /// Tokens used by user messages
    pub user: u32,

    /// Tokens used by assistant messages
    pub assistant: u32,

    /// Tokens used by tool messages
    pub tool: u32,

    /// Context limit for this agent/model
    pub limit: u32,

    /// Utilization percentage (0.0 - 1.0)
    pub utilization: f32,
}

impl TokenUsage {
    /// Create a new token usage summary
    pub fn new(limit: u32) -> Self {
        Self {
            total: 0,
            system: 0,
            user: 0,
            assistant: 0,
            tool: 0,
            limit,
            utilization: 0.0,
        }
    }

    /// Add tokens for a role
    pub fn add(&mut self, role: Role, tokens: u32) {
        self.total += tokens;
        match role {
            Role::System => self.system += tokens,
            Role::User => self.user += tokens,
            Role::Assistant => self.assistant += tokens,
            Role::Tool => self.tool += tokens,
        }
        self.utilization = self.total as f32 / self.limit as f32;
    }

    /// Check if we're at warning threshold
    pub fn is_warning(&self, threshold: f32) -> bool {
        self.utilization >= threshold
    }

    /// Check if we're at critical threshold
    pub fn is_critical(&self, threshold: f32) -> bool {
        self.utilization >= threshold
    }
}

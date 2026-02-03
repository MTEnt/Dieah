//! Configuration for dieah-memory

use std::path::PathBuf;

/// Configuration for the memory system
#[derive(Debug, Clone)]
pub struct Config {
    /// Base directory for all storage
    pub data_dir: PathBuf,

    /// Embedding model name (for reference, actual model set in embedding.rs)
    pub embedding_model: String,

    /// Embedding dimensions (384 for all-MiniLM-L6-v2)
    pub embedding_dimensions: usize,

    /// Maximum number of results to return from retrieval
    pub max_retrieval_results: usize,

    /// Minimum similarity score for retrieval (0.0 - 1.0)
    pub min_similarity_score: f32,

    /// Context window warning threshold (0.0 - 1.0)
    pub context_warning_threshold: f32,

    /// Context window critical threshold (0.0 - 1.0)
    pub context_critical_threshold: f32,

    /// HTTP server port
    pub server_port: u16,
}

impl Default for Config {
    fn default() -> Self {
        let data_dir = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("dieah-memory");

        Self {
            data_dir,
            embedding_model: "all-MiniLM-L6-v2".to_string(),
            embedding_dimensions: 384, // MiniLM-L6-v2 outputs 384-dim vectors
            max_retrieval_results: 10,
            min_similarity_score: 0.7,
            context_warning_threshold: 0.8,
            context_critical_threshold: 0.95,
            server_port: 8420,
        }
    }
}

impl Config {
    /// Create a new config with a custom data directory
    pub fn with_data_dir(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
            ..Default::default()
        }
    }

    /// Get the path to the SQLite database
    pub fn sqlite_path(&self) -> PathBuf {
        self.data_dir.join("metadata.db")
    }

    /// Get the path to the vector database
    pub fn vector_db_path(&self) -> PathBuf {
        self.data_dir.join("vectors")
    }

    /// Get the path to conversation logs for an agent/topic
    pub fn conversation_log_path(&self, agent_id: &str, topic_id: &str) -> PathBuf {
        self.data_dir
            .join("conversations")
            .join(agent_id)
            .join(format!("{}.jsonl", topic_id))
    }

    /// Ensure all required directories exist
    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.data_dir)?;
        std::fs::create_dir_all(self.data_dir.join("conversations"))?;
        std::fs::create_dir_all(self.vector_db_path())?;
        Ok(())
    }
}

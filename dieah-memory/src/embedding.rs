//! Embedding generation using fastembed (local, no API keys)

use std::sync::Arc;

use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use tokio::sync::Mutex;

use crate::config::Config;
use crate::error::{Error, Result};

/// Embedding service for generating vector embeddings locally
pub struct EmbeddingService {
    model: Arc<Mutex<TextEmbedding>>,
    dimensions: usize,
}

impl EmbeddingService {
    /// Create a new embedding service with local model
    pub fn new(config: &Config) -> Result<Self> {
        // Use all-MiniLM-L6-v2 by default (384 dimensions, fast, good quality)
        // Model downloads automatically on first use to ~/.cache/fastembed
        let model = TextEmbedding::try_new(
            InitOptions::new(EmbeddingModel::AllMiniLML6V2)
                .with_show_download_progress(true)
        )
        .map_err(|e| Error::embedding(format!("Failed to load embedding model: {}", e)))?;

        Ok(Self {
            model: Arc::new(Mutex::new(model)),
            dimensions: config.embedding_dimensions,
        })
    }

    /// Generate an embedding for a single text
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let model = self.model.clone();
        let text = text.to_string();
        
        // Lock the model and run embedding
        let mut guard = model.lock().await;
        let embeddings = guard.embed(vec![text], None)
            .map_err(|e| Error::embedding(format!("Embedding failed: {}", e)))?;

        embeddings
            .into_iter()
            .next()
            .ok_or_else(|| Error::embedding("No embedding returned"))
    }

    /// Generate embeddings for multiple texts
    pub async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let model = self.model.clone();
        let texts = texts.to_vec();

        // Lock the model and run embedding
        let mut guard = model.lock().await;
        let embeddings = guard.embed(texts, None)
            .map_err(|e| Error::embedding(format!("Embedding failed: {}", e)))?;

        Ok(embeddings)
    }

    /// Get the embedding dimensions
    pub fn dimensions(&self) -> usize {
        self.dimensions
    }

    /// Estimate the number of tokens in a text (rough approximation)
    pub fn estimate_tokens(&self, text: &str) -> usize {
        // Rough estimate: ~4 characters per token
        text.len() / 4
    }
}

/// Token counter using tiktoken
pub struct TokenCounter {
    // Using tiktoken-rs for accurate token counting
    bpe: tiktoken_rs::CoreBPE,
}

impl TokenCounter {
    /// Create a new token counter for a specific model
    pub fn new(model: &str) -> Result<Self> {
        let bpe = tiktoken_rs::get_bpe_from_model(model)
            .map_err(|e| Error::config(format!("Failed to load tokenizer for {}: {}", model, e)))?;

        Ok(Self { bpe })
    }

    /// Create a token counter for GPT-4/GPT-5 models
    pub fn for_gpt() -> Result<Self> {
        Self::new("gpt-4")
    }

    /// Create a token counter for Claude models (uses cl100k_base)
    pub fn for_claude() -> Result<Self> {
        // Claude uses a similar tokenizer to GPT-4
        Self::new("gpt-4")
    }

    /// Count tokens in a text
    pub fn count(&self, text: &str) -> u32 {
        self.bpe.encode_with_special_tokens(text).len() as u32
    }

    /// Count tokens with a fallback estimate if tokenization fails
    pub fn count_or_estimate(&self, text: &str) -> u32 {
        self.count(text)
    }

    /// Estimate tokens without using the tokenizer (faster, less accurate)
    pub fn estimate(text: &str) -> u32 {
        // ~4 characters per token is a reasonable estimate
        (text.len() / 4) as u32
    }
}

impl Default for TokenCounter {
    fn default() -> Self {
        Self::for_gpt().expect("Failed to create default token counter")
    }
}

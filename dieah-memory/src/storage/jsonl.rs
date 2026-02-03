//! JSONL storage for conversation history

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Seek, SeekFrom, Write};
use std::path::PathBuf;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::message::Message;

/// JSONL storage backend for conversation logs
pub struct JsonlStorage {
    base_path: PathBuf,
}

impl JsonlStorage {
    /// Create a new JSONL storage
    pub fn new(config: &Config) -> Result<Self> {
        let base_path = config.data_dir.join("conversations");
        std::fs::create_dir_all(&base_path)?;
        
        Ok(Self { base_path })
    }

    /// Get the path to the log file for a topic
    fn log_path(&self, agent_id: &str, topic_id: &str) -> PathBuf {
        let agent_dir = self.base_path.join(agent_id);
        agent_dir.join(format!("{}.jsonl", topic_id))
    }

    /// Ensure the directory exists for a topic
    fn ensure_dir(&self, agent_id: &str) -> Result<()> {
        let agent_dir = self.base_path.join(agent_id);
        std::fs::create_dir_all(&agent_dir)?;
        Ok(())
    }

    /// Append a message to the log
    pub fn append(&self, message: &Message) -> Result<u64> {
        self.ensure_dir(&message.agent_id)?;
        
        let path = self.log_path(&message.agent_id, &message.topic_id);
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        
        // Get current offset before writing
        let offset = file.seek(SeekFrom::End(0))?;
        
        // Write JSON line
        let json = serde_json::to_string(message)?;
        writeln!(file, "{}", json)?;
        
        Ok(offset)
    }

    /// Read all messages for a topic
    pub fn read_all(&self, agent_id: &str, topic_id: &str) -> Result<Vec<Message>> {
        let path = self.log_path(agent_id, topic_id);
        
        if !path.exists() {
            return Ok(Vec::new());
        }
        
        let file = File::open(&path)?;
        let reader = BufReader::new(file);
        
        let mut messages = Vec::new();
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let message: Message = serde_json::from_str(&line)?;
            messages.push(message);
        }
        
        Ok(messages)
    }

    /// Read the last N messages for a topic
    pub fn read_last_n(&self, agent_id: &str, topic_id: &str, n: usize) -> Result<Vec<Message>> {
        let all = self.read_all(agent_id, topic_id)?;
        let start = all.len().saturating_sub(n);
        Ok(all[start..].to_vec())
    }

    /// Read a message at a specific offset
    pub fn read_at_offset(&self, agent_id: &str, topic_id: &str, offset: u64) -> Result<Message> {
        let path = self.log_path(agent_id, topic_id);
        
        let mut file = File::open(&path)?;
        file.seek(SeekFrom::Start(offset))?;
        
        let reader = BufReader::new(file);
        let line = reader.lines().next()
            .ok_or_else(|| Error::not_found("No message at offset"))??;
        
        let message: Message = serde_json::from_str(&line)?;
        Ok(message)
    }

    /// Count messages in a topic
    pub fn count(&self, agent_id: &str, topic_id: &str) -> Result<usize> {
        let path = self.log_path(agent_id, topic_id);
        
        if !path.exists() {
            return Ok(0);
        }
        
        let file = File::open(&path)?;
        let reader = BufReader::new(file);
        
        Ok(reader.lines().filter(|l| l.as_ref().map(|s| !s.trim().is_empty()).unwrap_or(false)).count())
    }

    /// Get total tokens for a topic
    pub fn total_tokens(&self, agent_id: &str, topic_id: &str) -> Result<u32> {
        let messages = self.read_all(agent_id, topic_id)?;
        Ok(messages.iter().map(|m| m.tokens).sum())
    }

    /// Search messages by content (simple substring match)
    pub fn search(&self, agent_id: &str, topic_id: &str, query: &str) -> Result<Vec<Message>> {
        let messages = self.read_all(agent_id, topic_id)?;
        let query_lower = query.to_lowercase();
        
        Ok(messages
            .into_iter()
            .filter(|m| m.content.to_lowercase().contains(&query_lower))
            .collect())
    }

    /// Export a topic to a single JSON file
    pub fn export_topic(&self, agent_id: &str, topic_id: &str, output_path: &PathBuf) -> Result<()> {
        let messages = self.read_all(agent_id, topic_id)?;
        
        let file = File::create(output_path)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, &messages)?;
        
        Ok(())
    }

    /// Import messages from a JSON file
    pub fn import_topic(&self, agent_id: &str, topic_id: &str, input_path: &PathBuf) -> Result<usize> {
        self.ensure_dir(agent_id)?;
        
        let file = File::open(input_path)?;
        let reader = BufReader::new(file);
        let messages: Vec<Message> = serde_json::from_reader(reader)?;
        
        let count = messages.len();
        for mut message in messages {
            // Override agent/topic with the target
            message.agent_id = agent_id.to_string();
            message.topic_id = topic_id.to_string();
            self.append(&message)?;
        }
        
        Ok(count)
    }

    /// List all topics for an agent
    pub fn list_topics(&self, agent_id: &str) -> Result<Vec<String>> {
        let agent_dir = self.base_path.join(agent_id);
        
        if !agent_dir.exists() {
            return Ok(Vec::new());
        }
        
        let mut topics = Vec::new();
        for entry in std::fs::read_dir(&agent_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                if let Some(stem) = path.file_stem() {
                    topics.push(stem.to_string_lossy().to_string());
                }
            }
        }
        
        Ok(topics)
    }

    /// List all agents
    pub fn list_agents(&self) -> Result<Vec<String>> {
        if !self.base_path.exists() {
            return Ok(Vec::new());
        }
        
        let mut agents = Vec::new();
        for entry in std::fs::read_dir(&self.base_path)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                agents.push(entry.file_name().to_string_lossy().to_string());
            }
        }
        
        Ok(agents)
    }

    /// Delete a topic's conversation log
    pub fn delete_topic(&self, agent_id: &str, topic_id: &str) -> Result<()> {
        let path = self.log_path(agent_id, topic_id);
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }

    /// Get file size for a topic
    pub fn file_size(&self, agent_id: &str, topic_id: &str) -> Result<u64> {
        let path = self.log_path(agent_id, topic_id);
        if path.exists() {
            Ok(std::fs::metadata(&path)?.len())
        } else {
            Ok(0)
        }
    }
}

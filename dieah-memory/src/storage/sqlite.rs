//! SQLite storage for metadata and memory records

use rusqlite::{params, Connection, OptionalExtension};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::memory::{Memory, MemoryScope, MemoryType};

/// SQLite storage backend
pub struct SqliteStorage {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteStorage {
    /// Create a new SQLite storage
    pub fn new(config: &Config) -> Result<Self> {
        let conn = Connection::open(config.sqlite_path())?;
        
        // Initialize schema
        conn.execute_batch(include_str!("schema.sql"))?;
        
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Save a memory record
    pub fn save_memory(&self, memory: &Memory) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| Error::storage(e.to_string()))?;
        
        conn.execute(
            r#"
            INSERT INTO memories (
                id, scope, memory_type, agent_id, topic_id, content, context,
                tags, created_at, last_used_at, retrieval_count, active
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ON CONFLICT(id) DO UPDATE SET
                content = excluded.content,
                context = excluded.context,
                tags = excluded.tags,
                last_used_at = excluded.last_used_at,
                retrieval_count = excluded.retrieval_count,
                active = excluded.active
            "#,
            params![
                memory.id.to_string(),
                memory.scope.to_string(),
                memory.memory_type.to_string(),
                memory.agent_id,
                memory.topic_id,
                memory.content,
                memory.context,
                serde_json::to_string(&memory.tags)?,
                memory.created_at.to_rfc3339(),
                memory.last_used_at.map(|dt| dt.to_rfc3339()),
                memory.retrieval_count,
                memory.active,
            ],
        )?;
        
        Ok(())
    }

    /// Get a memory by ID
    pub fn get_memory(&self, id: Uuid) -> Result<Option<Memory>> {
        let conn = self.conn.lock().map_err(|e| Error::storage(e.to_string()))?;
        
        let result = conn.query_row(
            r#"
            SELECT id, scope, memory_type, agent_id, topic_id, content, context,
                   tags, created_at, last_used_at, retrieval_count, active
            FROM memories WHERE id = ?1
            "#,
            params![id.to_string()],
            |row| {
                Ok(MemoryRow {
                    id: row.get(0)?,
                    scope: row.get(1)?,
                    memory_type: row.get(2)?,
                    agent_id: row.get(3)?,
                    topic_id: row.get(4)?,
                    content: row.get(5)?,
                    context: row.get(6)?,
                    tags: row.get(7)?,
                    created_at: row.get(8)?,
                    last_used_at: row.get(9)?,
                    retrieval_count: row.get(10)?,
                    active: row.get(11)?,
                })
            },
        ).optional()?;
        
        result.map(|row| row.into_memory()).transpose()
    }

    /// List memories with optional filters
    pub fn list_memories(
        &self,
        scope: Option<MemoryScope>,
        agent_id: Option<&str>,
        topic_id: Option<&str>,
        active_only: bool,
    ) -> Result<Vec<Memory>> {
        let conn = self.conn.lock().map_err(|e| Error::storage(e.to_string()))?;
        
        let mut sql = String::from(
            r#"
            SELECT id, scope, memory_type, agent_id, topic_id, content, context,
                   tags, created_at, last_used_at, retrieval_count, active
            FROM memories WHERE 1=1
            "#
        );
        
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        
        if let Some(s) = scope {
            sql.push_str(" AND scope = ?");
            params_vec.push(Box::new(s.to_string()));
        }
        
        if let Some(aid) = agent_id {
            sql.push_str(" AND agent_id = ?");
            params_vec.push(Box::new(aid.to_string()));
        }
        
        if let Some(tid) = topic_id {
            sql.push_str(" AND topic_id = ?");
            params_vec.push(Box::new(tid.to_string()));
        }
        
        if active_only {
            sql.push_str(" AND active = 1");
        }
        
        sql.push_str(" ORDER BY created_at DESC");
        
        let mut stmt = conn.prepare(&sql)?;
        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        
        let rows = stmt.query_map(params_refs.as_slice(), |row| {
            Ok(MemoryRow {
                id: row.get(0)?,
                scope: row.get(1)?,
                memory_type: row.get(2)?,
                agent_id: row.get(3)?,
                topic_id: row.get(4)?,
                content: row.get(5)?,
                context: row.get(6)?,
                tags: row.get(7)?,
                created_at: row.get(8)?,
                last_used_at: row.get(9)?,
                retrieval_count: row.get(10)?,
                active: row.get(11)?,
            })
        })?;
        
        let mut memories = Vec::new();
        for row in rows {
            memories.push(row?.into_memory()?);
        }
        
        Ok(memories)
    }

    /// Delete a memory
    pub fn delete_memory(&self, id: Uuid) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| Error::storage(e.to_string()))?;
        conn.execute("DELETE FROM memories WHERE id = ?1", params![id.to_string()])?;
        Ok(())
    }

    /// Set memory active status
    pub fn set_memory_active(&self, id: Uuid, active: bool) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| Error::storage(e.to_string()))?;
        conn.execute(
            "UPDATE memories SET active = ?1 WHERE id = ?2",
            params![active, id.to_string()],
        )?;
        Ok(())
    }

    /// Update memory retrieval stats
    pub fn mark_memory_used(&self, id: Uuid) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| Error::storage(e.to_string()))?;
        conn.execute(
            r#"
            UPDATE memories 
            SET last_used_at = datetime('now'), retrieval_count = retrieval_count + 1
            WHERE id = ?1
            "#,
            params![id.to_string()],
        )?;
        Ok(())
    }

    /// Save an agent configuration
    pub fn save_agent(&self, agent: &AgentRecord) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| Error::storage(e.to_string()))?;
        
        conn.execute(
            r#"
            INSERT INTO agents (id, name, model, context_limit, color, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                model = excluded.model,
                context_limit = excluded.context_limit,
                color = excluded.color
            "#,
            params![
                agent.id,
                agent.name,
                agent.model,
                agent.context_limit,
                agent.color,
                agent.created_at.to_rfc3339(),
            ],
        )?;
        
        Ok(())
    }

    /// Get an agent by ID
    pub fn get_agent(&self, id: &str) -> Result<Option<AgentRecord>> {
        let conn = self.conn.lock().map_err(|e| Error::storage(e.to_string()))?;
        
        conn.query_row(
            "SELECT id, name, model, context_limit, color, created_at FROM agents WHERE id = ?1",
            params![id],
            |row| {
                Ok(AgentRecord {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    model: row.get(2)?,
                    context_limit: row.get(3)?,
                    color: row.get(4)?,
                    created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(5)?)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|_| chrono::Utc::now()),
                })
            },
        ).optional().map_err(Error::from)
    }

    /// List all agents
    pub fn list_agents(&self) -> Result<Vec<AgentRecord>> {
        let conn = self.conn.lock().map_err(|e| Error::storage(e.to_string()))?;
        
        let mut stmt = conn.prepare(
            "SELECT id, name, model, context_limit, color, created_at FROM agents ORDER BY name"
        )?;
        
        let rows = stmt.query_map([], |row| {
            Ok(AgentRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                model: row.get(2)?,
                context_limit: row.get(3)?,
                color: row.get(4)?,
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(5)?)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now()),
            })
        })?;
        
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Error::from)
    }

    /// Save a topic
    pub fn save_topic(&self, topic: &TopicRecord) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| Error::storage(e.to_string()))?;
        
        conn.execute(
            r#"
            INSERT INTO topics (id, agent_id, name, created_at, last_message_at, message_count, token_count)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                last_message_at = excluded.last_message_at,
                message_count = excluded.message_count,
                token_count = excluded.token_count
            "#,
            params![
                topic.id,
                topic.agent_id,
                topic.name,
                topic.created_at.to_rfc3339(),
                topic.last_message_at.map(|dt| dt.to_rfc3339()),
                topic.message_count,
                topic.token_count,
            ],
        )?;
        
        Ok(())
    }

    /// List topics for an agent
    pub fn list_topics(&self, agent_id: &str) -> Result<Vec<TopicRecord>> {
        let conn = self.conn.lock().map_err(|e| Error::storage(e.to_string()))?;
        
        let mut stmt = conn.prepare(
            r#"
            SELECT id, agent_id, name, created_at, last_message_at, message_count, token_count
            FROM topics WHERE agent_id = ?1 ORDER BY last_message_at DESC NULLS LAST
            "#
        )?;
        
        let rows = stmt.query_map(params![agent_id], |row| {
            let last_message_at: Option<String> = row.get(4)?;
            Ok(TopicRecord {
                id: row.get(0)?,
                agent_id: row.get(1)?,
                name: row.get(2)?,
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(3)?)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now()),
                last_message_at: last_message_at.and_then(|s| {
                    chrono::DateTime::parse_from_rfc3339(&s)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .ok()
                }),
                message_count: row.get(5)?,
                token_count: row.get(6)?,
            })
        })?;
        
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Error::from)
    }
}

/// Intermediate struct for reading from SQLite
struct MemoryRow {
    id: String,
    scope: String,
    memory_type: String,
    agent_id: Option<String>,
    topic_id: Option<String>,
    content: String,
    context: Option<String>,
    tags: String,
    created_at: String,
    last_used_at: Option<String>,
    retrieval_count: u32,
    active: bool,
}

impl MemoryRow {
    fn into_memory(self) -> Result<Memory> {
        let scope = match self.scope.as_str() {
            "global" => MemoryScope::Global,
            "agent" => MemoryScope::Agent,
            "topic" => MemoryScope::Topic,
            "personal" => MemoryScope::Personal,
            _ => return Err(Error::storage(format!("Unknown scope: {}", self.scope))),
        };
        
        let memory_type = match self.memory_type.as_str() {
            "correction" => MemoryType::Correction,
            "preference" => MemoryType::Preference,
            "fact" => MemoryType::Fact,
            "workflow" => MemoryType::Workflow,
            "constraint" => MemoryType::Constraint,
            _ => return Err(Error::storage(format!("Unknown memory type: {}", self.memory_type))),
        };
        
        Ok(Memory {
            id: Uuid::parse_str(&self.id).map_err(|e| Error::storage(e.to_string()))?,
            scope,
            memory_type,
            agent_id: self.agent_id,
            topic_id: self.topic_id,
            content: self.content,
            context: self.context,
            tags: serde_json::from_str(&self.tags)?,
            embedding: None,
            created_at: chrono::DateTime::parse_from_rfc3339(&self.created_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .map_err(|e| Error::storage(e.to_string()))?,
            last_used_at: self.last_used_at.and_then(|s| {
                chrono::DateTime::parse_from_rfc3339(&s)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .ok()
            }),
            retrieval_count: self.retrieval_count,
            active: self.active,
        })
    }
}

/// Agent record stored in SQLite
#[derive(Debug, Clone)]
pub struct AgentRecord {
    pub id: String,
    pub name: String,
    pub model: String,
    pub context_limit: u32,
    pub color: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Topic record stored in SQLite
#[derive(Debug, Clone)]
pub struct TopicRecord {
    pub id: String,
    pub agent_id: String,
    pub name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_message_at: Option<chrono::DateTime<chrono::Utc>>,
    pub message_count: u32,
    pub token_count: u32,
}

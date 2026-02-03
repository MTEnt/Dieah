-- Dieah Memory Schema

-- Agents table
CREATE TABLE IF NOT EXISTS agents (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    model TEXT NOT NULL,
    context_limit INTEGER NOT NULL DEFAULT 128000,
    color TEXT NOT NULL DEFAULT '#6366F1',
    created_at TEXT NOT NULL
);

-- Topics table
CREATE TABLE IF NOT EXISTS topics (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL,
    last_message_at TEXT,
    message_count INTEGER NOT NULL DEFAULT 0,
    token_count INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (agent_id) REFERENCES agents(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_topics_agent ON topics(agent_id);

-- Memories table (learned corrections, preferences, facts)
CREATE TABLE IF NOT EXISTS memories (
    id TEXT PRIMARY KEY,
    scope TEXT NOT NULL CHECK (scope IN ('global', 'agent', 'topic', 'personal')),
    memory_type TEXT NOT NULL CHECK (memory_type IN ('correction', 'preference', 'fact', 'workflow', 'constraint')),
    agent_id TEXT,
    topic_id TEXT,
    content TEXT NOT NULL,
    context TEXT,
    tags TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL,
    last_used_at TEXT,
    retrieval_count INTEGER NOT NULL DEFAULT 0,
    active INTEGER NOT NULL DEFAULT 1,
    FOREIGN KEY (agent_id) REFERENCES agents(id) ON DELETE CASCADE,
    FOREIGN KEY (topic_id) REFERENCES topics(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_memories_scope ON memories(scope);
CREATE INDEX IF NOT EXISTS idx_memories_agent ON memories(agent_id);
CREATE INDEX IF NOT EXISTS idx_memories_topic ON memories(topic_id);
CREATE INDEX IF NOT EXISTS idx_memories_active ON memories(active);

-- Message index (lightweight reference to JSONL files)
CREATE TABLE IF NOT EXISTS message_index (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    topic_id TEXT NOT NULL,
    role TEXT NOT NULL,
    tokens INTEGER NOT NULL,
    timestamp TEXT NOT NULL,
    -- Offset in JSONL file for fast seeking
    file_offset INTEGER NOT NULL,
    FOREIGN KEY (agent_id) REFERENCES agents(id) ON DELETE CASCADE,
    FOREIGN KEY (topic_id) REFERENCES topics(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_messages_topic ON message_index(topic_id);
CREATE INDEX IF NOT EXISTS idx_messages_timestamp ON message_index(timestamp);

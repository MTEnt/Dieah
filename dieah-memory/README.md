# Dieah Memory

Production-ready memory and context management system for AI agents.

## Overview

Dieah Memory provides a multi-layer context system inspired by [OpenAI's in-house data agent](https://openai.com/index/inside-our-in-house-data-agent/):

1. **Conversation History** - JSONL append-only logs for full conversation persistence
2. **Learned Memories** - Vector-indexed corrections, preferences, and facts
3. **Topic Context** - Per-topic metadata and recent history
4. **Agent Preferences** - Per-agent learned behaviors

## Features

- **Persistent Storage**: All messages saved to JSONL files
- **Vector Search**: Semantic search using LanceDB for memory retrieval
- **Token Counting**: Accurate token counting via tiktoken-rs
- **Context Budgets**: Real-time token usage tracking with warnings
- **RAG Retrieval**: Automatic relevant context injection
- **Self-Learning**: Detect corrections and save as memories
- **HTTP API**: REST API for integration with Dieah UI

## Installation

```bash
# Build the library and server
cargo build --release

# Run the server
cargo run --release --bin dieah-memory-server
```

## Environment Variables

```bash
# Required for embeddings
OPENAI_API_KEY=sk-...

# Optional
RUST_LOG=info  # Logging level
```

## API Endpoints

### Health Check
```
GET /health
```

### Memories

```
GET  /memories              # List memories (with filters)
POST /memories              # Create memory
GET  /memories/:id          # Get memory by ID
DELETE /memories/:id        # Delete memory
```

### Retrieval

```
POST /retrieve              # Retrieve relevant context for a query
```

Request:
```json
{
  "query": "How do I configure the database?",
  "agent_id": "asimov",
  "topic_id": "project-setup",
  "max_recent_messages": 10
}
```

### Messages

```
POST /messages                           # Append message to conversation
GET  /messages/:agent_id/:topic_id       # Get messages for a topic
```

### Token Management

```
POST /tokens/count                       # Count tokens in text
GET  /tokens/budget/:agent_id/:topic_id  # Get token budget for topic
```

### Agents & Topics

```
GET /agents                              # List all agents
GET /agents/:agent_id/topics             # List topics for an agent
```

## Data Storage

By default, data is stored in `~/.local/share/dieah-memory/`:

```
dieah-memory/
├── metadata.db           # SQLite database
├── vectors/              # LanceDB vector store
└── conversations/
    ├── asimov/
    │   ├── topic-123.jsonl
    │   └── topic-456.jsonl
    └── multivac/
        └── topic-789.jsonl
```

## Memory Types

| Type | Description |
|------|-------------|
| `correction` | User corrected agent behavior |
| `preference` | User stated preference |
| `fact` | Learned fact or knowledge |
| `workflow` | Process or procedure |
| `constraint` | Rule or limitation |

## Memory Scopes

| Scope | Description |
|-------|-------------|
| `global` | Applies to all agents and topics |
| `agent` | Applies to a specific agent |
| `topic` | Applies to a specific topic |
| `personal` | User-specific preferences |

## Usage Example

```rust
use dieah_memory::{Config, MemoryStore, Memory, MemoryType};
use dieah_memory::retrieval::RetrievalEngine;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize
    let config = Config::default();
    let store = MemoryStore::new(config.clone()).await?;
    let retrieval = RetrievalEngine::new(config)?;

    // Save a memory
    let memory = Memory::for_agent("asimov", MemoryType::Preference, 
        "User prefers concise responses");
    retrieval.embed_and_save(&store, memory).await?;

    // Retrieve context for a query
    let context = retrieval.retrieve(
        &store,
        "Give me a summary",
        Some("asimov"),
        Some("project-x"),
        10,
    ).await?;

    println!("Retrieved {} memories", context.memories.len());
    println!("{}", context.format_for_prompt());

    Ok(())
}
```

## License

MIT

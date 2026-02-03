//! Dieah Memory Server
//!
//! HTTP API for the memory system.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;

use dieah_memory::{
    config::Config,
    embedding::TokenCounter,
    memory::{Memory, MemoryScope, MemoryStore, MemoryType},
    message::{Message, Role},
    retrieval::{ContextBudget, RetrievalEngine},
};

/// Application state shared across handlers
struct AppState {
    store: MemoryStore,
    retrieval: RetrievalEngine,
    token_counter: TokenCounter,
}

type SharedState = Arc<RwLock<AppState>>;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Load configuration
    let config = Config::default();
    tracing::info!("Starting Dieah Memory Server on port {}", config.server_port);
    tracing::info!("Data directory: {:?}", config.data_dir);

    // Initialize components
    let store = MemoryStore::new(config.clone()).await?;
    let retrieval = RetrievalEngine::new(config.clone())?;
    let token_counter = TokenCounter::for_gpt()?;

    let state = Arc::new(RwLock::new(AppState {
        store,
        retrieval,
        token_counter,
    }));

    // Build router
    let app = Router::new()
        // Health check
        .route("/health", get(health))
        // Memory CRUD
        .route("/memories", get(list_memories).post(create_memory))
        .route("/memories/:id", get(get_memory).delete(delete_memory))
        // Retrieval
        .route("/retrieve", post(retrieve_context))
        // Messages
        .route("/messages", post(append_message))
        .route("/messages/:agent_id/:topic_id", get(get_messages))
        // Token counting
        .route("/tokens/count", post(count_tokens))
        .route("/tokens/budget/:agent_id/:topic_id", get(get_token_budget))
        // Agents and topics
        .route("/agents", get(list_agents))
        .route("/agents/:agent_id/topics", get(list_topics))
        // Add CORS
        .layer(CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any))
        .with_state(state);

    let port = config.server_port;
    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port)).await?;
    tracing::info!("Server listening on http://127.0.0.1:{}", port);

    axum::serve(listener, app).await?;

    Ok(())
}

// === Handlers ===

async fn health() -> &'static str {
    "ok"
}

// --- Memory handlers ---

#[derive(Debug, Deserialize)]
struct ListMemoriesQuery {
    scope: Option<String>,
    agent_id: Option<String>,
    topic_id: Option<String>,
    active_only: Option<bool>,
}

async fn list_memories(
    State(state): State<SharedState>,
    Query(query): Query<ListMemoriesQuery>,
) -> Result<Json<Vec<MemoryResponse>>, StatusCode> {
    let state = state.read().await;

    let scope = query.scope.and_then(|s| match s.as_str() {
        "global" => Some(MemoryScope::Global),
        "agent" => Some(MemoryScope::Agent),
        "topic" => Some(MemoryScope::Topic),
        "personal" => Some(MemoryScope::Personal),
        _ => None,
    });

    let memories = state
        .store
        .list_memories(
            scope,
            query.agent_id.as_deref(),
            query.topic_id.as_deref(),
            query.active_only.unwrap_or(true),
        )
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(memories.into_iter().map(MemoryResponse::from).collect()))
}

#[derive(Debug, Deserialize)]
struct CreateMemoryRequest {
    scope: String,
    memory_type: String,
    agent_id: Option<String>,
    topic_id: Option<String>,
    content: String,
    context: Option<String>,
    tags: Option<Vec<String>>,
}

async fn create_memory(
    State(state): State<SharedState>,
    Json(req): Json<CreateMemoryRequest>,
) -> Result<Json<MemoryResponse>, StatusCode> {
    let state = state.write().await;

    let scope = match req.scope.as_str() {
        "global" => MemoryScope::Global,
        "agent" => MemoryScope::Agent,
        "topic" => MemoryScope::Topic,
        "personal" => MemoryScope::Personal,
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    let memory_type = match req.memory_type.as_str() {
        "correction" => MemoryType::Correction,
        "preference" => MemoryType::Preference,
        "fact" => MemoryType::Fact,
        "workflow" => MemoryType::Workflow,
        "constraint" => MemoryType::Constraint,
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    let mut memory = match scope {
        MemoryScope::Global => Memory::global(memory_type, req.content),
        MemoryScope::Agent => {
            let agent_id = req.agent_id.ok_or(StatusCode::BAD_REQUEST)?;
            Memory::for_agent(agent_id, memory_type, req.content)
        }
        MemoryScope::Topic => {
            let agent_id = req.agent_id.ok_or(StatusCode::BAD_REQUEST)?;
            let topic_id = req.topic_id.ok_or(StatusCode::BAD_REQUEST)?;
            Memory::for_topic(agent_id, topic_id, memory_type, req.content)
        }
        MemoryScope::Personal => Memory::global(memory_type, req.content),
    };

    if let Some(context) = req.context {
        memory = memory.with_context(context);
    }

    if let Some(tags) = req.tags {
        memory = memory.with_tags(tags);
    }

    // Embed and save
    let memory = state
        .retrieval
        .embed_and_save(&state.store, memory)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(MemoryResponse::from(memory)))
}

async fn get_memory(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<Json<MemoryResponse>, StatusCode> {
    let state = state.read().await;

    let uuid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;

    let memory = state
        .store
        .get_memory(uuid)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(MemoryResponse::from(memory)))
}

async fn delete_memory(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let state = state.read().await;

    let uuid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;

    state
        .store
        .delete_memory(uuid)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::NO_CONTENT)
}

// --- Retrieval handlers ---

#[derive(Debug, Deserialize)]
struct RetrieveRequest {
    query: String,
    agent_id: Option<String>,
    topic_id: Option<String>,
    max_recent_messages: Option<usize>,
}

#[derive(Debug, Serialize)]
struct RetrieveResponse {
    memories: Vec<RetrievedMemoryResponse>,
    recent_messages: Vec<MessageResponse>,
    total_tokens: u32,
    formatted_context: String,
}

#[derive(Debug, Serialize)]
struct RetrievedMemoryResponse {
    id: String,
    content: String,
    scope: String,
    memory_type: String,
    score: f32,
}

async fn retrieve_context(
    State(state): State<SharedState>,
    Json(req): Json<RetrieveRequest>,
) -> Result<Json<RetrieveResponse>, StatusCode> {
    let state = state.read().await;

    let context = state
        .retrieval
        .retrieve(
            &state.store,
            &req.query,
            req.agent_id.as_deref(),
            req.topic_id.as_deref(),
            req.max_recent_messages.unwrap_or(10),
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(RetrieveResponse {
        memories: context
            .memories
            .iter()
            .map(|m| RetrievedMemoryResponse {
                id: m.id.to_string(),
                content: m.content.clone(),
                scope: m.scope.clone(),
                memory_type: m.memory_type.clone(),
                score: m.score,
            })
            .collect(),
        recent_messages: context
            .recent_messages
            .iter()
            .map(MessageResponse::from)
            .collect(),
        total_tokens: context.total_tokens,
        formatted_context: context.format_for_prompt(),
    }))
}

// --- Message handlers ---

#[derive(Debug, Deserialize)]
struct AppendMessageRequest {
    agent_id: String,
    topic_id: String,
    role: String,
    content: String,
}

async fn append_message(
    State(state): State<SharedState>,
    Json(req): Json<AppendMessageRequest>,
) -> Result<Json<MessageResponse>, StatusCode> {
    let state = state.read().await;

    let role = match req.role.as_str() {
        "system" => Role::System,
        "user" => Role::User,
        "assistant" => Role::Assistant,
        "tool" => Role::Tool,
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    let tokens = state.token_counter.count(&req.content);

    let message = Message::new(req.agent_id, req.topic_id, role, req.content).with_tokens(tokens);

    state
        .store
        .jsonl()
        .append(&message)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(MessageResponse::from(&message)))
}

#[derive(Debug, Deserialize)]
struct GetMessagesQuery {
    limit: Option<usize>,
}

async fn get_messages(
    State(state): State<SharedState>,
    Path((agent_id, topic_id)): Path<(String, String)>,
    Query(query): Query<GetMessagesQuery>,
) -> Result<Json<Vec<MessageResponse>>, StatusCode> {
    let state = state.read().await;

    let messages = if let Some(limit) = query.limit {
        state
            .store
            .jsonl()
            .read_last_n(&agent_id, &topic_id, limit)
    } else {
        state.store.jsonl().read_all(&agent_id, &topic_id)
    }
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(messages.iter().map(MessageResponse::from).collect()))
}

// --- Token handlers ---

#[derive(Debug, Deserialize)]
struct CountTokensRequest {
    text: String,
}

#[derive(Debug, Serialize)]
struct CountTokensResponse {
    tokens: u32,
}

async fn count_tokens(
    State(state): State<SharedState>,
    Json(req): Json<CountTokensRequest>,
) -> Json<CountTokensResponse> {
    let state = state.read().await;
    let tokens = state.token_counter.count(&req.text);
    Json(CountTokensResponse { tokens })
}

#[derive(Debug, Serialize)]
struct TokenBudgetResponse {
    used: u32,
    limit: u32,
    remaining: u32,
    utilization: f32,
    status: String,
}

async fn get_token_budget(
    State(state): State<SharedState>,
    Path((agent_id, topic_id)): Path<(String, String)>,
) -> Result<Json<TokenBudgetResponse>, StatusCode> {
    let state = state.read().await;

    let total_tokens = state
        .store
        .jsonl()
        .total_tokens(&agent_id, &topic_id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // TODO: Get actual limit from agent config
    let limit = 128000u32;

    let mut budget = ContextBudget::new(
        limit,
        state.store.config().context_warning_threshold,
        state.store.config().context_critical_threshold,
    );
    budget.add(total_tokens);

    Ok(Json(TokenBudgetResponse {
        used: budget.used,
        limit: budget.limit,
        remaining: budget.remaining(),
        utilization: budget.utilization(),
        status: budget.status().to_string(),
    }))
}

// --- Agent/Topic handlers ---

async fn list_agents(
    State(state): State<SharedState>,
) -> Result<Json<Vec<String>>, StatusCode> {
    let state = state.read().await;
    let agents = state
        .store
        .jsonl()
        .list_agents()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(agents))
}

async fn list_topics(
    State(state): State<SharedState>,
    Path(agent_id): Path<String>,
) -> Result<Json<Vec<String>>, StatusCode> {
    let state = state.read().await;
    let topics = state
        .store
        .jsonl()
        .list_topics(&agent_id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(topics))
}

// === Response types ===

#[derive(Debug, Serialize)]
struct MemoryResponse {
    id: String,
    scope: String,
    memory_type: String,
    agent_id: Option<String>,
    topic_id: Option<String>,
    content: String,
    context: Option<String>,
    tags: Vec<String>,
    created_at: String,
    last_used_at: Option<String>,
    retrieval_count: u32,
    active: bool,
}

impl From<Memory> for MemoryResponse {
    fn from(m: Memory) -> Self {
        Self {
            id: m.id.to_string(),
            scope: m.scope.to_string(),
            memory_type: m.memory_type.to_string(),
            agent_id: m.agent_id,
            topic_id: m.topic_id,
            content: m.content,
            context: m.context,
            tags: m.tags,
            created_at: m.created_at.to_rfc3339(),
            last_used_at: m.last_used_at.map(|dt| dt.to_rfc3339()),
            retrieval_count: m.retrieval_count,
            active: m.active,
        }
    }
}

#[derive(Debug, Serialize)]
struct MessageResponse {
    id: String,
    agent_id: String,
    topic_id: String,
    role: String,
    content: String,
    tokens: u32,
    timestamp: String,
}

impl From<&Message> for MessageResponse {
    fn from(m: &Message) -> Self {
        Self {
            id: m.id.to_string(),
            agent_id: m.agent_id.clone(),
            topic_id: m.topic_id.clone(),
            role: m.role.to_string(),
            content: m.content.clone(),
            tokens: m.tokens,
            timestamp: m.timestamp.to_rfc3339(),
        }
    }
}

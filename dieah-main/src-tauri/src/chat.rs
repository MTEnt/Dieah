use std::collections::HashMap;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use tauri::{AppHandle, Emitter, State};
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;
use url::Url;

use crate::state::{AppState, GatewayCommand, GatewayHandle};

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GatewayConnectOptions {
  pub url: String,
  pub token: Option<String>,
  pub password: Option<String>,
  pub client_name: Option<String>,
  pub client_version: Option<String>,
  pub platform: Option<String>,
  pub mode: Option<String>,
  pub instance_id: Option<String>,
  pub role: Option<String>,
  pub scopes: Option<Vec<String>>,
  pub user_agent: Option<String>,
  pub locale: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatHistoryPayload {
  pub session_key: String,
  pub limit: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSendPayload {
  pub session_key: String,
  pub message: String,
  pub thinking: Option<String>,
  pub deliver: Option<bool>,
  pub attachments: Option<Vec<Value>>,
  pub timeout_ms: Option<u64>,
  pub idempotency_key: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatAbortPayload {
  pub session_key: String,
  pub run_id: Option<String>,
}

#[tauri::command]
pub async fn gateway_connect(
  app: AppHandle,
  state: State<'_, AppState>,
  options: GatewayConnectOptions,
) -> Result<Value, String> {
  let (tx, rx) = mpsc::channel(64);
  let (ready_tx, ready_rx) = oneshot::channel();

  {
    let mut guard = state.gateway.lock().await;
    if let Some(existing) = guard.as_ref() {
      if !existing.tx.is_closed() {
        return Err("gateway already connected".to_string());
      }
    }
    *guard = Some(GatewayHandle { tx: tx.clone() });
  }

  tauri::async_runtime::spawn(gateway_task(app, options, rx, ready_tx));

  match ready_rx.await {
    Ok(Ok(payload)) => Ok(payload),
    Ok(Err(err)) => Err(err),
    Err(_) => Err("gateway handshake failed".to_string()),
  }
}

#[tauri::command]
pub async fn gateway_disconnect(state: State<'_, AppState>) -> Result<(), String> {
  let mut guard = state.gateway.lock().await;
  if let Some(handle) = guard.take() {
    let _ = handle.tx.send(GatewayCommand::Disconnect).await;
  }
  Ok(())
}

#[tauri::command]
pub async fn gateway_request(
  state: State<'_, AppState>,
  method: String,
  params: Option<Value>,
) -> Result<Value, String> {
  let handle = {
    let guard = state.gateway.lock().await;
    guard
      .as_ref()
      .cloned()
      .ok_or_else(|| "gateway not connected".to_string())?
  };

  let (respond_to, response) = oneshot::channel();
  handle
    .tx
    .send(GatewayCommand::Request {
      method,
      params,
      respond_to,
    })
    .await
    .map_err(|_| "gateway request channel closed".to_string())?;

  response
    .await
    .map_err(|_| "gateway response dropped".to_string())?
}

#[tauri::command]
pub async fn chat_history(
  state: State<'_, AppState>,
  payload: ChatHistoryPayload,
) -> Result<Value, String> {
  let mut params = Map::new();
  params.insert("sessionKey".to_string(), Value::String(payload.session_key));
  if let Some(limit) = payload.limit {
    params.insert(
      "limit".to_string(),
      Value::Number(serde_json::Number::from(limit as u64)),
    );
  }
  gateway_request(
    state,
    "chat.history".to_string(),
    Some(Value::Object(params)),
  )
  .await
}

#[tauri::command]
pub async fn chat_send(state: State<'_, AppState>, payload: ChatSendPayload) -> Result<Value, String> {
  let mut params = Map::new();
  params.insert("sessionKey".to_string(), Value::String(payload.session_key));
  params.insert("message".to_string(), Value::String(payload.message));
  if let Some(thinking) = payload.thinking {
    params.insert("thinking".to_string(), Value::String(thinking));
  }
  if let Some(deliver) = payload.deliver {
    params.insert("deliver".to_string(), Value::Bool(deliver));
  }
  if let Some(attachments) = payload.attachments {
    params.insert("attachments".to_string(), Value::Array(attachments));
  }
  if let Some(timeout_ms) = payload.timeout_ms {
    params.insert(
      "timeoutMs".to_string(),
      Value::Number(serde_json::Number::from(timeout_ms)),
    );
  }
  let idempotency_key = payload
    .idempotency_key
    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
  params.insert("idempotencyKey".to_string(), Value::String(idempotency_key));

  gateway_request(
    state,
    "chat.send".to_string(),
    Some(Value::Object(params)),
  )
  .await
}

#[tauri::command]
pub async fn chat_abort(
  state: State<'_, AppState>,
  payload: ChatAbortPayload,
) -> Result<Value, String> {
  let mut params = Map::new();
  params.insert("sessionKey".to_string(), Value::String(payload.session_key));
  if let Some(run_id) = payload.run_id {
    params.insert("runId".to_string(), Value::String(run_id));
  }
  gateway_request(
    state,
    "chat.abort".to_string(),
    Some(Value::Object(params)),
  )
  .await
}

async fn gateway_task(
  app: AppHandle,
  options: GatewayConnectOptions,
  mut rx: mpsc::Receiver<GatewayCommand>,
  ready_tx: oneshot::Sender<Result<Value, String>>,
) {
  let _ = app.emit(
    "gateway-status",
    json!({ "status": "connecting", "url": options.url }),
  );

  let url = match Url::parse(&options.url) {
    Ok(url) => url,
    Err(err) => {
      let _ = ready_tx.send(Err(format!("invalid gateway url: {}", err)));
      let _ = app.emit(
        "gateway-status",
        json!({ "status": "error", "reason": "invalid url" }),
      );
      return;
    }
  };

  let connect_params = build_connect_params(&options);

  let (ws_stream, _) = match tokio_tungstenite::connect_async(url.as_str()).await {
    Ok(pair) => pair,
    Err(err) => {
      let _ = ready_tx.send(Err(format!("failed to connect: {}", err)));
      let _ = app.emit(
        "gateway-status",
        json!({ "status": "error", "reason": "connect failed" }),
      );
      return;
    }
  };

  let (mut write, mut read) = ws_stream.split();
  let mut pending: HashMap<String, oneshot::Sender<Result<Value, String>>> = HashMap::new();
  let mut connect_sent = false;
  let mut connect_request_id: Option<String> = None;
  let mut ready_tx = Some(ready_tx);
  let connect_timer = tokio::time::sleep(Duration::from_millis(750));
  tokio::pin!(connect_timer);

  let _ = app.emit("gateway-status", json!({ "status": "open" }));

  loop {
    tokio::select! {
      _ = &mut connect_timer, if !connect_sent => {
        if let Some(id) = send_connect(&mut write, &connect_params, &mut pending, ready_tx.take()).await {
          connect_sent = true;
          connect_request_id = Some(id);
        }
      }
      Some(cmd) = rx.recv() => {
        match cmd {
          GatewayCommand::Request { method, params, respond_to } => {
            let is_connect = method == "connect";
            if is_connect && connect_sent {
              let _ = respond_to.send(Err("gateway connect already in progress".to_string()));
              continue;
            }
            if let Some(id) = send_request(&mut write, method, params, &mut pending, respond_to).await {
              if connect_request_id.is_none() && is_connect {
                connect_request_id = Some(id);
                connect_sent = true;
              }
            }
          }
          GatewayCommand::Disconnect => {
            let _ = write.send(Message::Close(None)).await;
            break;
          }
        }
      }
      Some(msg) = read.next() => {
        match msg {
          Ok(Message::Text(text)) => {
            handle_incoming(&app, &text, &mut pending, &connect_params, &mut connect_sent, &mut connect_request_id, &mut write, &mut ready_tx).await;
          }
          Ok(Message::Close(frame)) => {
            let reason = frame.as_ref().map(|f| f.reason.to_string()).unwrap_or_default();
            let _ = app.emit("gateway-status", json!({ "status": "closed", "reason": reason }));
            break;
          }
          Ok(_) => {}
          Err(err) => {
            let _ = app.emit("gateway-status", json!({ "status": "error", "reason": err.to_string() }));
            break;
          }
        }
      }
      else => {
        break;
      }
    }
  }

  for (_, sender) in pending.drain() {
    let _ = sender.send(Err("gateway disconnected".to_string()));
  }

  let _ = app.emit("gateway-status", json!({ "status": "disconnected" }));
  if let Some(ready_tx) = ready_tx {
    let _ = ready_tx.send(Err("gateway disconnected before handshake".to_string()));
  }
}

async fn handle_incoming(
  app: &AppHandle,
  text: &str,
  pending: &mut HashMap<String, oneshot::Sender<Result<Value, String>>>,
  connect_params: &Value,
  connect_sent: &mut bool,
  connect_request_id: &mut Option<String>,
  write: &mut futures_util::stream::SplitSink<WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>, Message>,
  ready_tx: &mut Option<oneshot::Sender<Result<Value, String>>>,
) {
  let parsed: Value = match serde_json::from_str(text) {
    Ok(value) => value,
    Err(_) => return,
  };

  let frame_type = parsed.get("type").and_then(Value::as_str).unwrap_or_default();
  match frame_type {
    "event" => {
      let event_name = parsed.get("event").and_then(Value::as_str).unwrap_or("");
      if event_name == "connect.challenge" && !*connect_sent {
        if let Some(id) = send_connect(write, connect_params, pending, ready_tx.take()).await {
          *connect_sent = true;
          *connect_request_id = Some(id);
        }
        return;
      }
      let _ = app.emit("gateway-event", parsed.clone());
      if event_name == "chat" {
        if let Some(payload) = parsed.get("payload") {
          let _ = app.emit("chat-event", payload.clone());
        }
      }
      if event_name == "agent" {
        if let Some(payload) = parsed.get("payload") {
          let _ = app.emit("agent-event", payload.clone());
        }
      }
    }
    "res" => {
      let id = match parsed.get("id").and_then(Value::as_str) {
        Some(id) => id.to_string(),
        None => return,
      };
      let ok = parsed.get("ok").and_then(Value::as_bool).unwrap_or(false);
      let payload = parsed.get("payload").cloned().unwrap_or(Value::Null);
      let error_message = parsed
        .get("error")
        .and_then(|err| err.get("message"))
        .and_then(Value::as_str)
        .map(|s| s.to_string())
        .unwrap_or_else(|| "request failed".to_string());

      if let Some(sender) = pending.remove(&id) {
        if ok {
          let _ = sender.send(Ok(payload.clone()));
        } else {
          let _ = sender.send(Err(error_message.clone()));
        }
      }

      if connect_request_id.as_ref() == Some(&id) {
        let _ = app.emit("gateway-hello", payload.clone());
        if let Some(tx) = ready_tx.take() {
          if ok {
            let _ = tx.send(Ok(payload));
          } else {
            let _ = tx.send(Err(error_message));
          }
        }
      }
    }
    _ => {}
  }
}

async fn send_connect(
  write: &mut futures_util::stream::SplitSink<WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>, Message>,
  connect_params: &Value,
  pending: &mut HashMap<String, oneshot::Sender<Result<Value, String>>>,
  ready_tx: Option<oneshot::Sender<Result<Value, String>>>,
) -> Option<String> {
  send_request_with_ready(
    write,
    "connect".to_string(),
    Some(connect_params.clone()),
    pending,
    ready_tx,
  )
  .await
}

async fn send_request(
  write: &mut futures_util::stream::SplitSink<WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>, Message>,
  method: String,
  params: Option<Value>,
  pending: &mut HashMap<String, oneshot::Sender<Result<Value, String>>>,
  respond_to: oneshot::Sender<Result<Value, String>>,
) -> Option<String> {
  send_request_with_ready(write, method, params, pending, Some(respond_to)).await
}

async fn send_request_with_ready(
  write: &mut futures_util::stream::SplitSink<WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>, Message>,
  method: String,
  params: Option<Value>,
  pending: &mut HashMap<String, oneshot::Sender<Result<Value, String>>>,
  respond_to: Option<oneshot::Sender<Result<Value, String>>>,
) -> Option<String> {
  let id = uuid::Uuid::new_v4().to_string();
  let mut frame = Map::new();
  frame.insert("type".to_string(), Value::String("req".to_string()));
  frame.insert("id".to_string(), Value::String(id.clone()));
  frame.insert("method".to_string(), Value::String(method.clone()));
  if let Some(params) = params {
    frame.insert("params".to_string(), params);
  }
  let frame_value = Value::Object(frame);
  let payload = frame_value.to_string();
  if method == "connect" {
    println!("[DEBUG] Connect frame: {}", payload);
  }
  if let Err(err) = write.send(Message::Text(payload)).await {
    if let Some(respond_to) = respond_to {
      let _ = respond_to.send(Err(format!("send failed: {}", err)));
    }
    return None;
  }
  if let Some(respond_to) = respond_to {
    pending.insert(id.clone(), respond_to);
  }
  Some(id)
}

fn build_connect_params(options: &GatewayConnectOptions) -> Value {
  let allowed_ids = [
    "webchat-ui",
    "openclaw-control-ui",
    "webchat",
    "cli",
    "gateway-client",
    "openclaw-macos",
    "openclaw-ios",
    "openclaw-android",
    "node-host",
    "test",
    "fingerprint",
    "openclaw-probe",
  ];
  let allowed_modes = ["webchat", "cli", "ui", "backend", "node", "probe", "test"];

  let raw_id = options
    .client_name
    .clone()
    .unwrap_or_else(|| "webchat-ui".to_string())
    .to_lowercase();
  let client_id = if allowed_ids.contains(&raw_id.as_str()) {
    raw_id
  } else {
    "webchat-ui".to_string()
  };

  let raw_mode = options
    .mode
    .clone()
    .unwrap_or_else(|| "webchat".to_string())
    .to_lowercase();
  let client_mode = if allowed_modes.contains(&raw_mode.as_str()) {
    raw_mode
  } else {
    "webchat".to_string()
  };

  let client = json!({
    "id": client_id,
    "displayName": "Dieah",
    "version": options.client_version.clone().unwrap_or_else(|| "dev".to_string()),
    "platform": options.platform.clone().unwrap_or_else(|| "desktop".to_string()),
    "mode": client_mode,
  });
  let mut client = client
    .as_object()
    .cloned()
    .unwrap_or_else(Map::new);
  if let Some(instance_id) = options.instance_id.clone() {
    client.insert("instanceId".to_string(), Value::String(instance_id));
  }

  let role = options.role.clone().unwrap_or_else(|| "operator".to_string());
  let scopes = options.scopes.clone().unwrap_or_else(|| {
    vec![
      "operator.admin".to_string(),
      "operator.approvals".to_string(),
      "operator.pairing".to_string(),
    ]
  });

  let mut params = Map::new();
  params.insert(
    "minProtocol".to_string(),
    Value::Number(serde_json::Number::from(3u64)),
  );
  params.insert(
    "maxProtocol".to_string(),
    Value::Number(serde_json::Number::from(3u64)),
  );
  params.insert("client".to_string(), Value::Object(client));
  params.insert("role".to_string(), Value::String(role));
  params.insert("scopes".to_string(), Value::Array(scopes.into_iter().map(Value::String).collect()));
  params.insert("caps".to_string(), Value::Array(Vec::new()));

  let token = options
    .token
    .clone()
    .map(|t| t.trim().to_string())
    .filter(|t| !t.is_empty());
  let password = options
    .password
    .clone()
    .map(|t| t.trim().to_string())
    .filter(|t| !t.is_empty());
  if token.is_some() || password.is_some() {
    let mut auth = Map::new();
    if let Some(token) = token {
      auth.insert("token".to_string(), Value::String(token));
    }
    if let Some(password) = password {
      auth.insert("password".to_string(), Value::String(password));
    }
    params.insert("auth".to_string(), Value::Object(auth));
  }

  if let Some(user_agent) = options.user_agent.clone() {
    params.insert("userAgent".to_string(), Value::String(user_agent));
  }
  if let Some(locale) = options.locale.clone() {
    params.insert("locale".to_string(), Value::String(locale));
  }

  let params_json = Value::Object(params.clone());
  println!(
    "[DEBUG] Connect params: {}",
    serde_json::to_string_pretty(&params_json).unwrap_or_else(|_| "<failed to serialize>".to_string())
  );
  params_json
}

use serde_json::Value;
use tokio::sync::{mpsc, oneshot, Mutex};

#[derive(Clone)]
pub struct GatewayHandle {
  pub tx: mpsc::Sender<GatewayCommand>,
}

pub enum GatewayCommand {
  Request {
    method: String,
    params: Option<Value>,
    respond_to: oneshot::Sender<Result<Value, String>>,
  },
  Disconnect,
}

pub struct AppState {
  pub gateway: Mutex<Option<GatewayHandle>>,
}

impl Default for AppState {
  fn default() -> Self {
    Self {
      gateway: Mutex::new(None),
    }
  }
}

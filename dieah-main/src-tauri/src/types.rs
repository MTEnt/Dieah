use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct ProviderStatus {
  pub name: String,
  pub cli_found: bool,
}

#[derive(Serialize)]
pub struct OpenClawDetection {
  pub found: bool,
  pub path: Option<String>,
  pub version: Option<String>,
  pub config_dir: Option<String>,
  pub skills_paths: Vec<String>,
  pub providers: Vec<ProviderStatus>,
  pub log: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ProviderAuth {
  pub name: String,
  pub method: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AppSettings {
  pub workspace_path: Option<String>,
  pub skills_path: Option<String>,
  pub heartbeat_enabled: bool,
  pub heartbeat_interval_minutes: u32,
  pub provider_auth: Vec<ProviderAuth>,
  #[serde(default)]
  pub memory_enabled: bool,
  #[serde(default)]
  pub memory_url: Option<String>,
  #[serde(default)]
  pub memory_max_recent_messages: u32,
}

#[derive(Serialize)]
pub struct PathValidation {
  pub workspace_exists: bool,
  pub skills_exists: bool,
}

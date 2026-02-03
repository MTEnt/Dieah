use std::env;
use std::fs;
use std::path::PathBuf;

use crate::types::{AppSettings, PathValidation, ProviderAuth};

fn settings_path() -> PathBuf {
  let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
  PathBuf::from(home).join(".dieah").join("settings.json")
}

fn default_settings() -> AppSettings {
  AppSettings {
    workspace_path: None,
    skills_path: None,
    heartbeat_enabled: true,
    heartbeat_interval_minutes: 30,
    provider_auth: vec![
      ProviderAuth {
        name: "Claude".to_string(),
        method: "cli".to_string(),
      },
      ProviderAuth {
        name: "Gemini".to_string(),
        method: "cli".to_string(),
      },
      ProviderAuth {
        name: "Codex".to_string(),
        method: "cli".to_string(),
      },
    ],
    memory_enabled: true,
    memory_url: Some("http://127.0.0.1:8420".to_string()),
    memory_max_recent_messages: 10,
  }
}

fn load_settings() -> AppSettings {
  let path = settings_path();
  if let Ok(data) = fs::read_to_string(&path) {
    if let Ok(settings) = serde_json::from_str::<AppSettings>(&data) {
      return settings;
    }
  }
  default_settings()
}

fn save_settings_to_disk(settings: &AppSettings) -> Result<(), String> {
  let path = settings_path();
  if let Some(parent) = path.parent() {
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
  }
  let data = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
  fs::write(&path, data).map_err(|e| e.to_string())?;
  Ok(())
}

#[tauri::command]
pub fn get_settings() -> AppSettings {
  load_settings()
}

#[tauri::command]
pub fn save_settings(settings: AppSettings) -> Result<AppSettings, String> {
  save_settings_to_disk(&settings)?;
  Ok(settings)
}

#[tauri::command]
pub fn validate_paths(workspace_path: String, skills_path: String) -> PathValidation {
  PathValidation {
    workspace_exists: PathBuf::from(workspace_path).exists(),
    skills_exists: PathBuf::from(skills_path).exists(),
  }
}

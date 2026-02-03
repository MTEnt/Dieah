use std::env;
use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;
use url::Url;

use crate::types::{OpenClawDetection, ProviderStatus};

#[derive(serde::Serialize)]
pub struct CliStatus {
  pub ok: bool,
  pub output: Value,
}

#[derive(serde::Serialize)]
pub struct CliRunResult {
  pub ok: bool,
  pub message: String,
}

#[derive(serde::Serialize)]
pub struct GatewayInfo {
  pub url: String,
  pub port: u16,
  pub auth_mode: Option<String>,
  pub token: Option<String>,
  pub password: Option<String>,
  pub source: Vec<String>,
}

#[derive(serde::Serialize)]
pub struct ProfileEntry {
  pub name: String,
  pub config_path: Option<String>,
  pub state_dir: Option<String>,
  pub is_default: bool,
}

#[derive(serde::Serialize)]
pub struct ProfilePaths {
  pub profile: String,
  pub config_path: Option<String>,
  pub workspace_path: Option<String>,
  pub skills_path: Option<String>,
  pub source: Vec<String>,
}

#[derive(serde::Serialize)]
pub struct DashboardTokenInfo {
  pub url: Option<String>,
  pub token: Option<String>,
  pub source: Vec<String>,
}

fn find_in_path(binary: &str) -> Option<PathBuf> {
  let path_var = env::var_os("PATH")?;
  for path in env::split_paths(&path_var) {
    let candidate = path.join(binary);
    if candidate.is_file() {
      return Some(candidate);
    }
  }
  None
}

fn command_output(path: &PathBuf, args: &[&str]) -> Option<String> {
  let output = Command::new(path).args(args).output().ok()?;
  if !output.status.success() {
    return None;
  }
  let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
  if stdout.is_empty() {
    None
  } else {
    Some(stdout)
  }
}

fn normalize_profile(profile: Option<&str>) -> Option<String> {
  profile
    .map(|p| p.trim().to_string())
    .filter(|p| !p.is_empty() && p != "default")
}

fn profile_root_dir(profile: Option<&str>) -> Option<PathBuf> {
  let home = env::var("HOME").ok()?;
  let name = profile.unwrap_or("default");
  let dir = if name == "default" {
    ".openclaw".to_string()
  } else {
    format!(".openclaw-{}", name)
  };
  Some(PathBuf::from(home).join(dir))
}

fn apply_profile_arg(cmd: &mut Command, profile: Option<&str>) {
  if let Some(profile) = normalize_profile(profile) {
    cmd.args(["--profile", profile.as_str()]);
  }
}

fn parse_json_from_output(stdout: &str, stderr: &str) -> Option<Value> {
  let raw = stdout.trim();
  if !raw.is_empty() {
    if let Ok(value) = serde_json::from_str::<Value>(raw) {
      return Some(value);
    }
    for line in raw.lines().rev() {
      let line = line.trim();
      if line.is_empty() {
        continue;
      }
      if let Ok(value) = serde_json::from_str::<Value>(line) {
        return Some(value);
      }
    }
    if let (Some(start), Some(end)) = (raw.find('{'), raw.rfind('}')) {
      if end > start {
        if let Ok(value) = serde_json::from_str::<Value>(&raw[start..=end]) {
          return Some(value);
        }
      }
    }
    if let (Some(start), Some(end)) = (raw.find('['), raw.rfind(']')) {
      if end > start {
        if let Ok(value) = serde_json::from_str::<Value>(&raw[start..=end]) {
          return Some(value);
        }
      }
    }
  }
  let raw_err = stderr.trim();
  if !raw_err.is_empty() {
    if let Ok(value) = serde_json::from_str::<Value>(raw_err) {
      return Some(value);
    }
    for line in raw_err.lines().rev() {
      let line = line.trim();
      if line.is_empty() {
        continue;
      }
      if let Ok(value) = serde_json::from_str::<Value>(line) {
        return Some(value);
      }
    }
  }
  None
}

fn trim_url(raw: &str) -> String {
  let mut s = raw
    .trim()
    .trim_matches(|c: char| c == '"' || c == '\'' || c == ',' || c == ')' || c == ']')
    .to_string();
  while s.ends_with(|c: char| c == '.' || c == ',' || c == ')' || c == '"' || c == '\'' || c == ';') {
    s.pop();
  }
  s
}

fn extract_first_url(text: &str) -> Option<String> {
  for token in text.split_whitespace() {
    if token.starts_with("http://") || token.starts_with("https://") {
      return Some(trim_url(token));
    }
  }
  for line in text.lines() {
    if let Some(start) = line.find("http://") {
      return Some(trim_url(&line[start..]));
    }
    if let Some(start) = line.find("https://") {
      return Some(trim_url(&line[start..]));
    }
  }
  None
}

fn token_from_url(raw: &str) -> Option<String> {
  let url = Url::parse(raw).ok()?;
  for (key, value) in url.query_pairs() {
    let key = key.to_lowercase();
    if key == "token" || key.ends_with("token") {
      let token = value.trim().to_string();
      if !token.is_empty() {
        return Some(token);
      }
    }
  }
  None
}

#[tauri::command]
pub fn detect_openclaw() -> OpenClawDetection {
  let mut log = Vec::new();

  let openclaw_path = find_in_path("openclaw");
  let found = openclaw_path.is_some();
  if found {
    log.push("ok openclaw found".to_string());
  } else {
    log.push("openclaw not found".to_string());
  }

  let version = openclaw_path
    .as_ref()
    .and_then(|path| command_output(path, &["--version"]))
    .or_else(|| None);

  if let Some(v) = &version {
    log.push(format!("version: {}", v));
  }

  let home = env::var("HOME").ok();
  let config_dir = home.as_ref().map(|h| format!("{}/.openclaw", h));
  if let Some(cfg) = &config_dir {
    log.push(format!("config: {}", cfg));
  }

  let mut skills_paths = Vec::new();
  if let Ok(cwd) = env::current_dir() {
    skills_paths.push(format!("{}", cwd.join("skills").display()));
  } else {
    skills_paths.push("./skills".to_string());
  }
  if let Some(h) = &home {
    skills_paths.push(format!("{}/.openclaw/skills", h));
  } else {
    skills_paths.push("~/.openclaw/skills".to_string());
  }

  log.push(format!(
    "skills: {} (workspace) + {}",
    skills_paths.get(0).cloned().unwrap_or_else(|| "./skills".to_string()),
    skills_paths.get(1).cloned().unwrap_or_else(|| "~/.openclaw/skills".to_string())
  ));

  let providers = ["Claude", "Gemini", "Codex"]
    .iter()
    .map(|name| {
      let bin = name.to_lowercase();
      let cli_found = find_in_path(&bin).is_some();
      ProviderStatus {
        name: name.to_string(),
        cli_found,
      }
    })
    .collect::<Vec<_>>();

  OpenClawDetection {
    found,
    path: openclaw_path.map(|p| p.display().to_string()),
    version,
    config_dir,
    skills_paths,
    providers,
    log,
  }
}

fn config_get_json(openclaw_path: &PathBuf, profile: Option<&str>, key: &str) -> Option<Value> {
  let mut cmd = Command::new(openclaw_path);
  apply_profile_arg(&mut cmd, profile);
  let output = cmd
    .args(["config", "get", key, "--json"])
    .output()
    .ok()?;
  if !output.status.success() {
    return None;
  }
  let stdout = String::from_utf8_lossy(&output.stdout);
  let stderr = String::from_utf8_lossy(&output.stderr);
  parse_json_from_output(&stdout, &stderr)
}

#[tauri::command]
pub fn openclaw_gateway_info(profile: Option<String>) -> Result<GatewayInfo, String> {
  let openclaw_path = find_in_path("openclaw").ok_or_else(|| "openclaw not found".to_string())?;
  let profile = normalize_profile(profile.as_deref());
  let mut source = Vec::new();
  source.push(format!(
    "profile: {}",
    profile.clone().unwrap_or_else(|| "default".to_string())
  ));

  let env_port = if profile.is_some() {
    None
  } else {
    env::var("OPENCLAW_GATEWAY_PORT").ok()
  };
  let port = if let Some(raw) = env_port.as_ref() {
    raw.parse::<u16>().ok()
  } else {
    None
  };
  let port = match port {
    Some(port) => {
      source.push("port: env OPENCLAW_GATEWAY_PORT".to_string());
      port
    }
    None => {
      let config_port = config_get_json(&openclaw_path, profile.as_deref(), "gateway.port")
        .and_then(|v| v.as_u64())
        .and_then(|v| u16::try_from(v).ok());
      if let Some(port) = config_port {
        source.push("port: config gateway.port".to_string());
        port
      } else {
        source.push("port: default 18789".to_string());
        18789
      }
    }
  };

  let env_token = if profile.is_some() {
    None
  } else {
    env::var("OPENCLAW_GATEWAY_TOKEN")
      .ok()
      .map(|s| s.trim().to_string())
      .filter(|s| !s.is_empty())
  };
  let env_password = if profile.is_some() {
    None
  } else {
    env::var("OPENCLAW_GATEWAY_PASSWORD")
      .ok()
      .map(|s| s.trim().to_string())
      .filter(|s| !s.is_empty())
  };
  let config_mode = config_get_json(&openclaw_path, profile.as_deref(), "gateway.auth.mode")
    .and_then(|v| v.as_str().map(|s| s.to_string()));
  let config_token = config_get_json(&openclaw_path, profile.as_deref(), "gateway.auth.token")
    .and_then(|v| v.as_str().map(|s| s.trim().to_string()))
    .filter(|s| !s.is_empty());
  let config_password = config_get_json(&openclaw_path, profile.as_deref(), "gateway.auth.password")
    .and_then(|v| v.as_str().map(|s| s.trim().to_string()))
    .filter(|s| !s.is_empty());

  let auth_mode = config_mode.clone();
  if env_token.is_some() {
    source.push("auth: env OPENCLAW_GATEWAY_TOKEN".to_string());
  } else if config_token.is_some() {
    source.push("auth: config gateway.auth.token".to_string());
  }
  if env_password.is_some() {
    source.push("auth: env OPENCLAW_GATEWAY_PASSWORD".to_string());
  } else if config_password.is_some() {
    source.push("auth: config gateway.auth.password".to_string());
  }

  let token = env_token.or(config_token);
  let password = env_password.or(config_password);
  let url = format!("ws://127.0.0.1:{}", port);

  Ok(GatewayInfo {
    url,
    port,
    auth_mode,
    token,
    password,
    source,
  })
}

#[tauri::command]
pub fn openclaw_models_status(profile: Option<String>) -> Result<CliStatus, String> {
  let path = find_in_path("openclaw").ok_or_else(|| "openclaw not found in PATH".to_string())?;
  let profile = normalize_profile(profile.as_deref());
  let mut cmd = Command::new(path);
  apply_profile_arg(&mut cmd, profile.as_deref());
  let output = cmd
    .args(["models", "status", "--json"])
    .output()
    .map_err(|e| e.to_string())?;
  if !output.status.success() {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    return Err(if stderr.is_empty() {
      "openclaw models status failed".to_string()
    } else {
      stderr
    });
  }
  let stdout = String::from_utf8_lossy(&output.stdout);
  let stderr = String::from_utf8_lossy(&output.stderr);
  let json: Value =
    parse_json_from_output(&stdout, &stderr).ok_or_else(|| "failed to parse models status".to_string())?;
  Ok(CliStatus { ok: true, output: json })
}

#[tauri::command]
pub fn openclaw_models_list(profile: Option<String>) -> Result<CliStatus, String> {
  let path = find_in_path("openclaw").ok_or_else(|| "openclaw not found in PATH".to_string())?;
  let profile = normalize_profile(profile.as_deref());
  let mut cmd = Command::new(path);
  apply_profile_arg(&mut cmd, profile.as_deref());
  let output = cmd
    .args(["models", "list", "--json"])
    .output()
    .map_err(|e| e.to_string())?;
  if !output.status.success() {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    return Err(if stderr.is_empty() {
      "openclaw models list failed".to_string()
    } else {
      stderr
    });
  }
  let stdout = String::from_utf8_lossy(&output.stdout);
  let stderr = String::from_utf8_lossy(&output.stderr);
  let json: Value =
    parse_json_from_output(&stdout, &stderr).ok_or_else(|| "failed to parse models list".to_string())?;
  Ok(CliStatus { ok: true, output: json })
}

#[tauri::command]
pub fn openclaw_agents_list(profile: Option<String>) -> Result<CliStatus, String> {
  let path = find_in_path("openclaw").ok_or_else(|| "openclaw not found in PATH".to_string())?;
  let profile = normalize_profile(profile.as_deref());
  let mut cmd = Command::new(path);
  apply_profile_arg(&mut cmd, profile.as_deref());
  let output = cmd
    .args(["agents", "list", "--json"])
    .output()
    .map_err(|e| e.to_string())?;
  if !output.status.success() {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    return Err(if stderr.is_empty() {
      "openclaw agents list failed".to_string()
    } else {
      stderr
    });
  }
  let stdout = String::from_utf8_lossy(&output.stdout);
  let stderr = String::from_utf8_lossy(&output.stderr);
  let json: Value =
    parse_json_from_output(&stdout, &stderr).ok_or_else(|| "failed to parse agents list".to_string())?;
  Ok(CliStatus { ok: true, output: json })
}

#[tauri::command]
pub fn openclaw_run_in_terminal(commands: Vec<String>) -> Result<CliRunResult, String> {
  if commands.is_empty() {
    return Err("no commands provided".to_string());
  }
  let allowed_prefixes = ["openclaw ", "claude ", "gemini ", "codex "];
  for cmd in &commands {
    if !allowed_prefixes.iter().any(|prefix| cmd.trim_start().starts_with(prefix)) {
      return Err(format!("blocked command: {}", cmd));
    }
  }

  let joined = commands.join(" && ");
  #[cfg(target_os = "macos")]
  {
    let script = format!(
      "tell application \"Terminal\" to do script {}",
      serde_json::to_string(&joined).unwrap_or_else(|_| "\"\"".to_string())
    );
    let status = Command::new("osascript")
      .args(["-e", &script])
      .status()
      .map_err(|e| e.to_string())?;
    if !status.success() {
      return Err("failed to launch Terminal".to_string());
    }
    return Ok(CliRunResult {
      ok: true,
      message: "command launched in Terminal".to_string(),
    });
  }
  #[cfg(not(target_os = "macos"))]
  {
    let status = Command::new("sh")
      .args(["-lc", &joined])
      .status()
      .map_err(|e| e.to_string())?;
    if !status.success() {
      return Err("command failed".to_string());
    }
    Ok(CliRunResult {
      ok: true,
      message: "command executed".to_string(),
    })
  }
}

#[tauri::command]
pub fn openclaw_dashboard_token(profile: Option<String>) -> Result<DashboardTokenInfo, String> {
  let openclaw_path = find_in_path("openclaw").ok_or_else(|| "openclaw not found".to_string())?;
  let profile = normalize_profile(profile.as_deref());
  let mut cmd = Command::new(&openclaw_path);
  apply_profile_arg(&mut cmd, profile.as_deref());
  let output = cmd
    .args(["dashboard", "--no-open"])
    .output()
    .map_err(|e| e.to_string())?;
  if !output.status.success() {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    return Err(if stderr.is_empty() {
      "openclaw dashboard failed".to_string()
    } else {
      stderr
    });
  }
  let stdout = String::from_utf8_lossy(&output.stdout);
  let stderr = String::from_utf8_lossy(&output.stderr);
  let combined = format!("{}\n{}", stdout, stderr);
  let url = extract_first_url(&combined);
  let token = url.as_ref().and_then(|u| token_from_url(u));
  let mut source = Vec::new();
  source.push("auth: dashboard --no-open".to_string());
  if let Some(profile) = profile {
    source.push(format!("profile: {}", profile));
  }
  Ok(DashboardTokenInfo { url, token, source })
}

#[tauri::command]
pub fn openclaw_profiles() -> Result<Vec<ProfileEntry>, String> {
  let home = env::var("HOME").map_err(|_| "HOME not set".to_string())?;
  let mut profiles = Vec::new();
  let default_dir = PathBuf::from(&home).join(".openclaw");
  let default_config = default_dir.join("openclaw.json");
  profiles.push(ProfileEntry {
    name: "default".to_string(),
    config_path: default_config
      .exists()
      .then(|| default_config.display().to_string()),
    state_dir: default_dir.exists().then(|| default_dir.display().to_string()),
    is_default: true,
  });

  if let Ok(entries) = std::fs::read_dir(&home) {
    for entry in entries.flatten() {
      let name = entry.file_name().to_string_lossy().to_string();
      if !name.starts_with(".openclaw-") {
        continue;
      }
      let profile_name = name.trim_start_matches(".openclaw-").to_string();
      if profile_name.is_empty() {
        continue;
      }
      let dir = PathBuf::from(&home).join(&name);
      let config = dir.join("openclaw.json");
      profiles.push(ProfileEntry {
        name: profile_name,
        config_path: config.exists().then(|| config.display().to_string()),
        state_dir: dir.exists().then(|| dir.display().to_string()),
        is_default: false,
      });
    }
  }

  profiles.sort_by(|a, b| a.name.cmp(&b.name));
  Ok(profiles)
}

#[tauri::command]
pub fn openclaw_profile_paths(profile: Option<String>) -> Result<ProfilePaths, String> {
  let openclaw_path = find_in_path("openclaw").ok_or_else(|| "openclaw not found".to_string())?;
  let profile = normalize_profile(profile.as_deref());
  let profile_name = profile.clone().unwrap_or_else(|| "default".to_string());
  let mut source = Vec::new();
  source.push(format!("profile: {}", profile_name));

  let root = profile_root_dir(Some(profile_name.as_str()));
  let config_path = root
    .as_ref()
    .map(|dir| dir.join("openclaw.json"))
    .filter(|path| path.exists())
    .map(|path| path.display().to_string());
  if let Some(cfg) = &config_path {
    source.push(format!("config: {}", cfg));
  }

  let workspace_path = config_get_json(&openclaw_path, profile.as_deref(), "agents.defaults.workspace")
    .and_then(|v| v.as_str().map(|s| s.to_string()));
  if let Some(workspace) = &workspace_path {
    source.push(format!("workspace: {}", workspace));
  }

  let config_skills = config_get_json(&openclaw_path, profile.as_deref(), "skills.path")
    .and_then(|v| v.as_str().map(|s| s.to_string()));
  let workspace_skills = workspace_path.as_ref().and_then(|workspace| {
    let candidate = PathBuf::from(workspace).join("skills");
    candidate.exists().then(|| candidate.display().to_string())
  });
  let profile_skills = root.as_ref().and_then(|dir| {
    let candidate = dir.join("skills");
    candidate.exists().then(|| candidate.display().to_string())
  });
  let skills_path = config_skills.or(workspace_skills).or(profile_skills);
  if let Some(skills) = &skills_path {
    source.push(format!("skills: {}", skills));
  }

  Ok(ProfilePaths {
    profile: profile_name,
    config_path,
    workspace_path,
    skills_path,
    source,
  })
}

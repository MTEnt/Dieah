mod chat;
mod memory;
mod openclaw;
mod settings;
mod skills;
mod state;
mod types;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .manage(state::AppState::default())
    .invoke_handler(tauri::generate_handler![
      chat::gateway_connect,
      chat::gateway_disconnect,
      chat::gateway_request,
      chat::chat_history,
      chat::chat_send,
      chat::chat_abort,
      openclaw::detect_openclaw,
      openclaw::openclaw_gateway_info,
      openclaw::openclaw_dashboard_token,
      openclaw::openclaw_profiles,
      openclaw::openclaw_profile_paths,
      openclaw::openclaw_models_status,
      openclaw::openclaw_models_list,
      openclaw::openclaw_agents_list,
      openclaw::openclaw_run_in_terminal,
      settings::get_settings,
      settings::save_settings,
      settings::validate_paths
    ])
    .run(tauri::generate_context!())
    .expect("error while running Dieah");
}

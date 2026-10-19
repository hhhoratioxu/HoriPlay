#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use horiplay_core::InputSettings;
use horiplay_session::{Action, Config, Engine, Snapshot};
use tauri::Manager;
#[tauri::command]
fn snapshot(engine: tauri::State<'_, Engine>) -> Snapshot {
    engine.snapshot()
}
#[tauri::command]
async fn start_session(
    engine: tauri::State<'_, Engine>,
    config: Config,
    invite: Option<String>,
) -> Result<(), String> {
    engine
        .start(config, invite)
        .await
        .map_err(|e| e.to_string())
}
#[tauri::command]
async fn session_action(engine: tauri::State<'_, Engine>, action: Action) -> Result<(), String> {
    engine.action(action).await.map_err(|e| e.to_string())
}
#[tauri::command]
fn configure_input(
    engine: tauri::State<'_, Engine>,
    selected: Option<u32>,
    settings: InputSettings,
) -> Result<(), String> {
    engine
        .configure_input(selected, settings)
        .map_err(|e| e.to_string())
}
#[tauri::command]
fn rumble_test(engine: tauri::State<'_, Engine>, id: u32) -> Result<(), String> {
    engine.rumble(id).map_err(|e| e.to_string())
}
fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let name = if cfg!(windows) {
                "SDL3.dll"
            } else {
                "libSDL3.0.dylib"
            };
            let bundled = app.path().resource_dir()?.join(name);
            if bundled.is_file() {
                std::env::set_var("HORIPLAY_SDL3", bundled);
            }
            app.manage(Engine::new());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            snapshot,
            start_session,
            session_action,
            configure_input,
            rumble_test
        ])
        .run(tauri::generate_context!())
        .expect("HoriPlay desktop initialization failed");
}

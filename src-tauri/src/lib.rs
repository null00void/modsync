mod commands;
mod core;

use commands::friends::{add_friend, get_or_create_owner_identity, list_friends, remove_friend};
use commands::profile::{get_profile_summary, list_profiles};
use commands::sync::{execute_sync, fetch_friend_diff, push_profile};
use tauri_plugin_log::{Target, TargetKind};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                // Rolling file in the OS-standard app log directory, plus
                // stdout so `npm run tauri dev` still shows logs live.
                // KeepOne discards the previous run's log on startup --
                // plenty for "what just went wrong", no unbounded growth.
                .targets([
                    Target::new(TargetKind::LogDir { file_name: Some("modsync".into()) }),
                    Target::new(TargetKind::Stdout),
                ])
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepOne)
                .level(log::LevelFilter::Info)
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            list_profiles,
            get_profile_summary,
            get_or_create_owner_identity,
            list_friends,
            add_friend,
            remove_friend,
            push_profile,
            fetch_friend_diff,
            execute_sync,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

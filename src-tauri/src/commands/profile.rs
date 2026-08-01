use crate::core::mods_yml::{self, ModEntry};
use crate::core::r2modman_paths::{self, LocalProfile};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct ProfileSummary {
    pub profile: LocalProfile,
    pub community_slug: Option<String>,
    pub mods: Vec<ModEntry>,
}

#[tauri::command]
pub fn list_profiles() -> Result<Vec<LocalProfile>, String> {
    super::log_err(
        "list_profiles",
        r2modman_paths::list_local_profiles().map_err(|e| e.to_string()),
    )
}

#[tauri::command]
pub fn get_profile_summary(profile_path: String) -> Result<ProfileSummary, String> {
    super::log_err("get_profile_summary", (|| {
        let path = Path::new(&profile_path);
        let mods = mods_yml::read_mods_yml(path).map_err(|e| e.to_string())?;
        let community_slug = mods_yml::find_community_slug(&mods);

        // Re-derive the LocalProfile shell from the path so the frontend gets
        // a consistent shape whether it came from list_profiles or here.
        let profile_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let game_short_name = path
            .parent() // profiles/
            .and_then(|p| p.parent()) // <GameShortName>/
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        Ok(ProfileSummary {
            profile: LocalProfile {
                game_short_name,
                profile_name,
                path: profile_path,
            },
            community_slug,
            mods,
        })
    })())
}

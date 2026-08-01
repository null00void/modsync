use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize)]
pub struct LocalProfile {
    pub game_short_name: String,
    pub profile_name: String,
    pub path: String,
}

/// `dirs::config_dir()` resolves to `$XDG_CONFIG_HOME`/`~/.config` on
/// Linux and `%APPDATA%` (Roaming) on Windows -- confirmed to exactly
/// match r2modman's own base dir by reading its source directly rather
/// than assuming: r2modman resolves its data directory via Electron's
/// `app.getPath('appData')` (`src-electron/ipcListeners.ts`, handler
/// `get-appData-directory`), then joins `'r2modmanPlus-local'`
/// (`src/App.vue`). Electron's `app.getPath('appData')` is documented to
/// resolve to the same OS-conventional roaming-config directory
/// `dirs::config_dir()` targets on every platform Tauri supports, so this
/// one code path is correct on Linux, Windows, and macOS alike. Not yet
/// exercised against a literal Windows filesystem (no Windows machine in
/// this dev loop) -- worth a real spot-check once one's available, but
/// the path logic itself is no longer a guess.
pub fn r2modman_base_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("r2modmanPlus-local"))
}

/// Scans `<base>/<GameShortName>/profiles/<ProfileName>/` for every game
/// and profile that actually has a `mods.yml`, skipping stray/incomplete
/// directories (e.g. a profile mid-creation, or a game folder with no
/// profiles yet).
pub fn list_local_profiles() -> std::io::Result<Vec<LocalProfile>> {
    let mut result = Vec::new();
    let Some(base) = r2modman_base_dir() else {
        return Ok(result);
    };
    if !base.is_dir() {
        return Ok(result);
    }

    for game_entry in std::fs::read_dir(&base)? {
        let game_entry = game_entry?;
        if !game_entry.file_type()?.is_dir() {
            continue;
        }
        let game_short_name = game_entry.file_name().to_string_lossy().to_string();
        let profiles_dir = game_entry.path().join("profiles");
        if !profiles_dir.is_dir() {
            continue;
        }

        for profile_entry in std::fs::read_dir(&profiles_dir)? {
            let profile_entry = profile_entry?;
            if !profile_entry.file_type()?.is_dir() {
                continue;
            }
            if !profile_entry.path().join("mods.yml").is_file() {
                continue;
            }
            result.push(LocalProfile {
                game_short_name: game_short_name.clone(),
                profile_name: profile_entry.file_name().to_string_lossy().to_string(),
                path: profile_entry.path().to_string_lossy().to_string(),
            });
        }
    }

    Ok(result)
}

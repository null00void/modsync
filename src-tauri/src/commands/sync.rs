use crate::core::diff::{self, ExecutionPlan, SyncPlan};
use crate::core::mods_yml::{ModEntry, VersionNumber};
use crate::core::supabase_client::SyncedModEntry;
use crate::core::{identity, installer, lock, mods_yml, supabase_client};
use serde::Serialize;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

fn game_short_name_from_path(path: &Path) -> String {
    path.parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}

#[tauri::command]
pub async fn push_profile(app: AppHandle, profile_path: String) -> Result<String, String> {
    super::log_err("push_profile", async {
        let path = Path::new(&profile_path);
        let mods = mods_yml::read_mods_yml(path).map_err(|e| e.to_string())?;
        let community_slug = mods_yml::find_community_slug(&mods)
            .ok_or("could not determine this profile's Thunderstore community")?;

        let profile_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let game_short_name = game_short_name_from_path(path);

        let identity = identity::get_or_create_identity(&app, None).await?;
        let payload = supabase_client::build_push_payload(&mods);

        supabase_client::upsert_synced_profile(
            &identity,
            &game_short_name,
            &community_slug,
            &profile_name,
            &payload,
        )
        .await?;

        Ok(identity.share_code)
    }.await)
}

/// Fetches a friend's synced profile for the same game as the local
/// profile at `profile_path`, and returns the diff plan a sync would
/// execute. Read-only -- doesn't touch any files, just computes what
/// *would* change.
#[tauri::command]
pub async fn fetch_friend_diff(
    profile_path: String,
    friend_share_code: String,
) -> Result<SyncPlan, String> {
    super::log_err("fetch_friend_diff", async {
        let path = Path::new(&profile_path);
        let local_mods = mods_yml::read_mods_yml(path).map_err(|e| e.to_string())?;
        let game_short_name = game_short_name_from_path(path);

        let friend_profile = supabase_client::get_synced_profile(&friend_share_code, &game_short_name)
            .await?
            .ok_or_else(|| {
                format!(
                    "no synced profile found for that friend code and game ({game_short_name}) -- have they shared this game's profile yet?"
                )
            })?;

        Ok(diff::diff_against_friend(&local_mods, &friend_profile.mods))
    }.await)
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncProgressEvent {
    pub step: String,
    pub current: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncSummary {
    pub installed_or_updated: usize,
    pub disabled: usize,
    pub reenabled: usize,
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Builds a fresh mods.yml entry for a package that came from a friend's
/// payload. Some fields (description, gameVersion, etc.) aren't part of
/// the sync payload and get reasonable defaults -- r2modman only really
/// cares about name/version/enabled/dependencies for actually loading
/// the mod; the rest are cosmetic in its own UI.
fn build_mod_entry(f: &SyncedModEntry, community_slug: &str, enabled: bool) -> Result<ModEntry, String> {
    let version_number = VersionNumber::parse(&f.version)
        .ok_or_else(|| format!("could not parse version '{}' for {}", f.version, f.full_name))?;
    let package_name = f
        .full_name
        .strip_prefix(&format!("{}-", f.author_name))
        .unwrap_or(&f.full_name)
        .to_string();

    Ok(ModEntry {
        manifest_version: 1,
        name: f.full_name.clone(),
        author_name: f.author_name.clone(),
        website_url: format!(
            "https://thunderstore.io/c/{community_slug}/p/{}/{package_name}/",
            f.author_name
        ),
        display_name: f.display_name.clone(),
        description: String::new(),
        game_version: "0".to_string(),
        network_mode: "both".to_string(),
        package_type: "other".to_string(),
        install_mode: "managed".to_string(),
        installed_at_time: now_millis(),
        loaders: Vec::new(),
        dependencies: f.dependencies.clone(),
        incompatibilities: Vec::new(),
        optional_dependencies: Vec::new(),
        version_number,
        enabled,
        online_source: true,
    })
}

/// Core of a sync: downloads/installs anything new or changed, flips
/// enable state for the rest, and atomically rewrites mods.yml. Kept free
/// of `AppHandle`/Tauri so it can be exercised directly in tests -- the
/// `on_progress` callback stands in for the `sync-progress` event emit
/// the real command wraps this with.
///
/// Guarded for the whole operation by a [`lock::ProfileLock`], which
/// refuses to start a second concurrent sync against the same profile and
/// is released automatically (Drop) however this function returns. Takes
/// a `mods.yml.bak` snapshot before touching anything, and writes the
/// final `mods.yml` atomically (see [`mods_yml::write_mods_yml`]).
async fn apply_execution_plan(
    path: &Path,
    community_slug: &str,
    local_mods: Vec<ModEntry>,
    plan: &ExecutionPlan,
    client: &reqwest::Client,
    mut on_progress: impl FnMut(&str, usize, usize),
) -> Result<SyncSummary, String> {
    // Locked for the whole operation: refuses to start a second
    // concurrent sync against the same profile, and is released
    // automatically (Drop) however this function returns.
    let _lock = lock::ProfileLock::acquire(path)?;

    // Cheap insurance: if something goes wrong partway through, the
    // previous known-good mods.yml is one copy away.
    let backup_path = path.join("mods.yml.bak");
    let _ = std::fs::copy(path.join("mods.yml"), &backup_path);

    let mut entries: Vec<ModEntry> = local_mods;
    let cache_dir = path
        .parent() // profiles/
        .and_then(|p| p.parent()) // <GameShortName>/
        .map(|p| p.join("cache"))
        .ok_or("could not determine this game's cache directory")?;
    let staging_dir = cache_dir.join(".modsync-staging");

    let total = plan.to_install_or_update.len() + plan.to_disable.len() + plan.to_reenable.len();
    let mut current = 0usize;

    for f in &plan.to_install_or_update {
        current += 1;
        on_progress(&format!("Downloading {}", f.display_name), current, total);

        let package_name = f
            .full_name
            .strip_prefix(&format!("{}-", f.author_name))
            .unwrap_or(&f.full_name);
        let url = installer::download_url(&f.author_name, package_name, &f.version);
        let zip_path = staging_dir.join(format!("{}-{}.zip", f.full_name, f.version));
        installer::download_to(client, &url, &zip_path).await?;

        on_progress(&format!("Installing {}", f.display_name), current, total);
        let extract_dir = staging_dir.join(format!("{}-{}", f.full_name, f.version));
        installer::extract_zip(&zip_path, &extract_dir)?;
        installer::uninstall(path, &f.full_name)?;
        installer::install_extracted(&extract_dir, path, &f.full_name)?;
        installer::set_enabled(path, &f.full_name, f.enabled)?;

        let new_entry = build_mod_entry(f, community_slug, f.enabled)?;
        entries.retain(|e| e.name != f.full_name);
        entries.push(new_entry);
    }

    for full_name in &plan.to_disable {
        current += 1;
        on_progress(&format!("Disabling {full_name}"), current, total);
        installer::set_enabled(path, full_name, false)?;
        if let Some(e) = entries.iter_mut().find(|e| &e.name == full_name) {
            e.enabled = false;
        }
    }

    for full_name in &plan.to_reenable {
        current += 1;
        on_progress(&format!("Re-enabling {full_name}"), current, total);
        installer::set_enabled(path, full_name, true)?;
        if let Some(e) = entries.iter_mut().find(|e| &e.name == full_name) {
            e.enabled = true;
        }
    }

    mods_yml::write_mods_yml(path, &entries).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(&staging_dir);

    Ok(SyncSummary {
        installed_or_updated: plan.to_install_or_update.len(),
        disabled: plan.to_disable.len(),
        reenabled: plan.to_reenable.len(),
    })
}

/// Re-fetches the friend's payload fresh (rather than trusting a
/// possibly-stale plan the frontend already showed) since this is the
/// step that actually touches disk, then delegates to
/// [`apply_execution_plan`].
#[tauri::command]
pub async fn execute_sync(
    app: AppHandle,
    profile_path: String,
    friend_share_code: String,
) -> Result<SyncSummary, String> {
    let result = async {
        let path = Path::new(&profile_path).to_path_buf();
        let game_short_name = game_short_name_from_path(&path);

        let local_mods = mods_yml::read_mods_yml(&path).map_err(|e| e.to_string())?;
        let community_slug = mods_yml::find_community_slug(&local_mods)
            .ok_or("could not determine this profile's Thunderstore community")?;

        let friend_profile = supabase_client::get_synced_profile(&friend_share_code, &game_short_name)
            .await?
            .ok_or("no synced profile found for that friend code and game")?;

        let plan = diff::plan_execution(&local_mods, &friend_profile.mods);
        let client = reqwest::Client::new();

        apply_execution_plan(&path, &community_slug, local_mods, &plan, &client, |step, current, total| {
            let _ = app.emit(
                "sync-progress",
                SyncProgressEvent {
                    step: step.to_string(),
                    current,
                    total,
                },
            );
        })
        .await
    }
    .await;

    if let Ok(summary) = &result {
        log::info!(
            "execute_sync ok for {profile_path}: {} installed/updated, {} disabled, {} reenabled",
            summary.installed_or_updated,
            summary.disabled,
            summary.reenabled
        );
    }
    super::log_err("execute_sync", result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::supabase_client::SyncedModEntry;
    use std::fs;

    /// This machine's real REPO game root, if r2modman is actually
    /// installed here -- same convention as `parses_real_repo_profile` in
    /// mods_yml.rs: a live sanity check against real local state, skipped
    /// gracefully where that state doesn't exist.
    fn real_repo_root() -> Option<std::path::PathBuf> {
        let home = std::env::var("HOME").ok()?;
        let root = Path::new(&home).join(".config/r2modmanPlus-local/REPO");
        if root.join("profiles/Default/mods.yml").is_file() {
            Some(root)
        } else {
            None
        }
    }

    /// End-to-end re-verification of M6 (progress events, mods.yml.bak,
    /// atomic write, lock file) now that M5's installer bug is fixed:
    /// runs a real install against a throwaway profile, downloading an
    /// actual small package (Zehs-REPOLib) from Thunderstore, then a real
    /// disable and re-enable, checking every M6 guarantee after each
    /// step.
    #[tokio::test]
    async fn apply_execution_plan_end_to_end() {
        let Some(repo_root) = real_repo_root() else {
            eprintln!("skipping: no real r2modman REPO install on this machine");
            return;
        };

        let profile = repo_root.join("profiles/.modsync-m6-test");
        let _ = fs::remove_dir_all(&profile);
        fs::create_dir_all(&profile).unwrap();

        // Seed with just enough of a real mods.yml (one real BepInEx-
        // BepInExPack entry copied from Default) for community-slug
        // detection to work.
        let default_mods = mods_yml::read_mods_yml(&repo_root.join("profiles/Default")).unwrap();
        let bepinex = default_mods
            .iter()
            .find(|e| e.name == "BepInEx-BepInExPack")
            .cloned()
            .expect("Default profile should have BepInEx-BepInExPack installed");
        mods_yml::write_mods_yml(&profile, std::slice::from_ref(&bepinex)).unwrap();

        let client = reqwest::Client::new();
        let repolib = SyncedModEntry {
            full_name: "Zehs-REPOLib".to_string(),
            author_name: "Zehs".to_string(),
            display_name: "REPOLib".to_string(),
            version: "4.2.0".to_string(),
            enabled: true,
            dependencies: vec!["BepInEx-BepInExPack-5.4.2305".to_string()],
        };

        // --- Phase 1: fresh install ---
        let local_mods = mods_yml::read_mods_yml(&profile).unwrap();
        let community_slug = mods_yml::find_community_slug(&local_mods).unwrap();
        let install_plan = ExecutionPlan {
            to_install_or_update: vec![repolib.clone()],
            to_disable: Vec::new(),
            to_reenable: Vec::new(),
        };
        let mut progress_log = Vec::new();
        let summary = apply_execution_plan(
            &profile,
            &community_slug,
            local_mods,
            &install_plan,
            &client,
            |step, current, total| progress_log.push((step.to_string(), current, total)),
        )
        .await
        .unwrap();

        assert_eq!(summary.installed_or_updated, 1);
        assert_eq!(
            progress_log,
            vec![
                ("Downloading REPOLib".to_string(), 1, 1),
                ("Installing REPOLib".to_string(), 1, 1),
            ]
        );
        assert!(
            !profile.join(".modsync.lock").exists(),
            "lock must be released after the sync completes"
        );
        assert!(
            !profile.join("mods.yml.tmp").exists(),
            "atomic write must not leave a .tmp file behind"
        );
        assert_eq!(
            fs::read_to_string(profile.join("mods.yml.bak")).unwrap(),
            serde_yaml::to_string(std::slice::from_ref(&bepinex)).unwrap(),
            "backup should be a snapshot of the pre-sync mods.yml"
        );
        let after_install = mods_yml::read_mods_yml(&profile).unwrap();
        assert!(after_install.iter().any(|e| e.name == "Zehs-REPOLib" && e.enabled));
        assert!(
            profile.join("BepInEx/plugins/Zehs-REPOLib").is_dir(),
            "package should actually be installed on disk"
        );
        assert!(
            !repo_root.join("cache/.modsync-staging").exists(),
            "staging dir should be cleaned up after a sync"
        );

        // --- Phase 2: disable ---
        let disable_plan = ExecutionPlan {
            to_install_or_update: Vec::new(),
            to_disable: vec!["Zehs-REPOLib".to_string()],
            to_reenable: Vec::new(),
        };
        let mut progress_log = Vec::new();
        apply_execution_plan(
            &profile,
            &community_slug,
            after_install,
            &disable_plan,
            &client,
            |step, current, total| progress_log.push((step.to_string(), current, total)),
        )
        .await
        .unwrap();
        assert_eq!(progress_log, vec![("Disabling Zehs-REPOLib".to_string(), 1, 1)]);
        let after_disable = mods_yml::read_mods_yml(&profile).unwrap();
        assert!(!after_disable.iter().find(|e| e.name == "Zehs-REPOLib").unwrap().enabled);

        // --- Phase 3: re-enable ---
        let reenable_plan = ExecutionPlan {
            to_install_or_update: Vec::new(),
            to_disable: Vec::new(),
            to_reenable: vec!["Zehs-REPOLib".to_string()],
        };
        let mut progress_log = Vec::new();
        apply_execution_plan(
            &profile,
            &community_slug,
            after_disable,
            &reenable_plan,
            &client,
            |step, current, total| progress_log.push((step.to_string(), current, total)),
        )
        .await
        .unwrap();
        assert_eq!(progress_log, vec![("Re-enabling Zehs-REPOLib".to_string(), 1, 1)]);
        let after_reenable = mods_yml::read_mods_yml(&profile).unwrap();
        assert!(after_reenable.iter().find(|e| e.name == "Zehs-REPOLib").unwrap().enabled);

        fs::remove_dir_all(&profile).unwrap();
    }

    /// Confirms the lock actually blocks a second concurrent sync against
    /// the same profile, independent of the rest of the pipeline.
    #[test]
    fn profile_lock_rejects_concurrent_acquire() {
        let dir = std::env::temp_dir().join("modsync-lock-test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let first = lock::ProfileLock::acquire(&dir).unwrap();
        let second = lock::ProfileLock::acquire(&dir);
        assert!(second.is_err(), "a second concurrent acquire must fail while the first is held");

        drop(first);
        let third = lock::ProfileLock::acquire(&dir);
        assert!(third.is_ok(), "the lock must be released once the guard is dropped");

        fs::remove_dir_all(&dir).unwrap();
    }
}

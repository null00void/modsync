use std::fs;
use std::io;
use std::path::Path;

/// BepInEx category folder names r2modman knows about. Matched against a
/// folder's own name (case-insensitive) regardless of how deep it's
/// nested when installing (see `install_flat`), and checked directly
/// under `BepInEx/` when uninstalling/enabling a mod that might live
/// under any of them -- harmless to check ones that don't apply to a
/// given package.
const CATEGORIES: [&str; 5] = ["plugins", "patchers", "core", "config", "monomod"];

/// Thunderstore's direct download URL. Confirmed live during planning:
/// this 302-redirects straight to the CDN zip, so no API round-trip is
/// needed just to download a package.
pub fn download_url(author_name: &str, name: &str, version: &str) -> String {
    format!("https://thunderstore.io/package/download/{author_name}/{name}/{version}/")
}

pub async fn download_to(client: &reqwest::Client, url: &str, dest: &Path) -> Result<(), String> {
    let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("download failed ({}) for {url}", resp.status()));
    }
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(dest, &bytes).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn extract_zip(zip_path: &Path, dest_dir: &Path) -> Result<(), String> {
    let file = fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    fs::create_dir_all(dest_dir).map_err(|e| e.to_string())?;
    archive.extract(dest_dir).map_err(|e| e.to_string())?;
    Ok(())
}

fn copy_dir_all(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dst_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst_path)?;
        } else {
            fs::copy(entry.path(), &dst_path)?;
        }
    }
    Ok(())
}

/// Installs a package that has no top-level `BepInEx/` folder, replicating
/// r2modman's real recursive rule-matching instead of dumping the zip
/// contents as-is (the latter was M5's known bug: a real package with a
/// non-BepInEx-recognized wrapper folder, e.g. `Resources/`, was being
/// installed with that folder preserved, when r2modman actually discards
/// it entirely).
///
/// Any folder whose own name matches [`CATEGORIES`] is
/// installed as a unit (its internal structure preserved) under
/// `BepInEx/<category>/<full_name>/`. Any other folder is walked through
/// transparently -- its name is never used as a path component. Loose
/// files found this way are flattened straight to
/// `BepInEx/<route>/<full_name>/<basename>`, where `<route>` is `monomod`
/// for `.mm.dll` files and `plugins` for everything else (`plugins` is
/// every BepInEx game's default fallback location), discarding whatever
/// chain of unrecognized folders they were found under.
fn install_flat(dir: &Path, profile_path: &Path, full_name: &str) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let ty = entry.file_type().map_err(|e| e.to_string())?;
        let path = entry.path();
        if ty.is_dir() {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if let Some(category) = CATEGORIES.iter().find(|c| **c == name) {
                let dest = profile_path.join("BepInEx").join(category).join(full_name);
                copy_dir_all(&path, &dest).map_err(|e| e.to_string())?;
            } else {
                install_flat(&path, profile_path, full_name)?;
            }
        } else {
            let is_monomod = entry
                .file_name()
                .to_string_lossy()
                .to_lowercase()
                .ends_with(".mm.dll");
            let route = if is_monomod { "monomod" } else { "plugins" };
            let dest_dir = profile_path.join("BepInEx").join(route).join(full_name);
            fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;
            fs::copy(&path, dest_dir.join(entry.file_name())).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Installs an already-extracted package into the live profile, following
/// r2modman's own two install shapes (verified against a real profile:
/// see the plan doc):
///
/// - If the archive has a top-level `BepInEx/` dir, each category under
///   it (plugins, patchers, ...) gets merged into the profile nested one
///   level deeper under `<full_name>/`, e.g. an archive's
///   `BepInEx/patchers/RepoXR` becomes the profile's
///   `BepInEx/patchers/DaXcess-RepoXR/RepoXR`.
/// - Otherwise, see [`install_flat`] for the recursive rule matching that
///   replaces r2modman's flattening logic.
pub fn install_extracted(
    extracted_root: &Path,
    profile_path: &Path,
    full_name: &str,
) -> Result<(), String> {
    let bepinex_dir = extracted_root.join("BepInEx");
    if bepinex_dir.is_dir() {
        for category_entry in fs::read_dir(&bepinex_dir).map_err(|e| e.to_string())? {
            let category_entry = category_entry.map_err(|e| e.to_string())?;
            if !category_entry
                .file_type()
                .map_err(|e| e.to_string())?
                .is_dir()
            {
                continue;
            }
            let dest = profile_path
                .join("BepInEx")
                .join(category_entry.file_name())
                .join(full_name);
            copy_dir_all(&category_entry.path(), &dest).map_err(|e| e.to_string())?;
        }
    } else {
        install_flat(extracted_root, profile_path, full_name)?;
    }
    Ok(())
}

/// Removes a previously installed version's files before installing a
/// replacement, so a version update doesn't leave stale files from the
/// old version mixed in with the new one.
pub fn uninstall(profile_path: &Path, full_name: &str) -> Result<(), String> {
    for category in CATEGORIES {
        let dir = profile_path.join("BepInEx").join(category).join(full_name);
        if dir.is_dir() {
            fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Enables/disables a mod on disk the same way r2modman itself does:
/// rename its top-level loose files (not nested subfolder contents) with
/// a `.old` suffix, in every category folder it's installed under. Never
/// deletes anything, so it's trivially reversible.
pub fn set_enabled(profile_path: &Path, full_name: &str, enabled: bool) -> Result<(), String> {
    for category in CATEGORIES {
        let dir = profile_path.join("BepInEx").join(category).join(full_name);
        if !dir.is_dir() {
            continue;
        }
        for entry in fs::read_dir(&dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
                continue;
            }
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if enabled {
                if let Some(stripped) = name.strip_suffix(".old") {
                    fs::rename(&path, dir.join(stripped)).map_err(|e| e.to_string())?;
                }
            } else if !name.ends_with(".old") {
                fs::rename(&path, dir.join(format!("{name}.old"))).map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic fixtures for `install_flat` shapes that don't have a
    /// real local package to test against on this machine (unlike the
    /// `Resources/`-wrapper case, which was confirmed against a real
    /// Rogue-Backrooms install): a package whose non-BepInEx wrapper
    /// contains a *recognized* category folder (`patchers`), a loose
    /// `.mm.dll` file with no wrapper at all, and a two-level-deep
    /// unrecognized nesting. These are structural/logic checks, not
    /// real-data verification -- kept deliberately small since there's no
    /// real package on hand to cross-check them against.
    fn scratch_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("modsync-installer-test-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn install_flat_preserves_a_recognized_category_folder_found_under_a_wrapper() {
        let extracted = scratch_dir("patchers-under-wrapper");
        let wrapper = extracted.join("SomeWrapper").join("patchers");
        fs::create_dir_all(&wrapper).unwrap();
        fs::write(wrapper.join("MyPatcher.dll"), b"fake").unwrap();

        let profile = scratch_dir("patchers-under-wrapper-profile");
        install_extracted(&extracted, &profile, "Author-Mod").unwrap();

        assert!(profile
            .join("BepInEx/patchers/Author-Mod/MyPatcher.dll")
            .is_file());
    }

    #[test]
    fn install_flat_routes_loose_mm_dll_to_monomod() {
        let extracted = scratch_dir("loose-mm-dll");
        fs::write(extracted.join("Example.mm.dll"), b"fake").unwrap();
        fs::write(extracted.join("Example.dll"), b"fake").unwrap();

        let profile = scratch_dir("loose-mm-dll-profile");
        install_extracted(&extracted, &profile, "Author-Mod").unwrap();

        assert!(profile
            .join("BepInEx/monomod/Author-Mod/Example.mm.dll")
            .is_file());
        assert!(profile
            .join("BepInEx/plugins/Author-Mod/Example.dll")
            .is_file());
    }

    #[test]
    fn install_flat_flattens_through_two_levels_of_unrecognized_folders() {
        let extracted = scratch_dir("double-nested");
        let nested = extracted.join("Resources").join("SubStuff");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("data.json"), b"{}").unwrap();

        let profile = scratch_dir("double-nested-profile");
        install_extracted(&extracted, &profile, "Author-Mod").unwrap();

        assert!(profile.join("BepInEx/plugins/Author-Mod/data.json").is_file());
        assert!(!profile.join("BepInEx/plugins/Author-Mod/Resources").exists());
    }

    /// A folder found deeper than the top level that happens to be named
    /// like a recognized category (e.g. `patchers`) must stop the
    /// recursion there and preserve everything below it, even if it's
    /// several unrecognized wrapper folders deep.
    #[test]
    fn install_flat_recognizes_category_folders_at_any_depth() {
        let extracted = scratch_dir("category-at-depth");
        let deep = extracted.join("A").join("B").join("plugins").join("SubDir");
        fs::create_dir_all(&deep).unwrap();
        fs::write(deep.join("Nested.dll"), b"fake").unwrap();

        let profile = scratch_dir("category-at-depth-profile");
        install_extracted(&extracted, &profile, "Author-Mod").unwrap();

        assert!(profile
            .join("BepInEx/plugins/Author-Mod/SubDir/Nested.dll")
            .is_file());
    }
}

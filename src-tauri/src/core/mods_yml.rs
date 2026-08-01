use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VersionNumber {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl std::fmt::Display for VersionNumber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl VersionNumber {
    pub fn parse(s: &str) -> Option<Self> {
        let mut parts = s.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        let patch = parts.next()?.parse().ok()?;
        Some(Self { major, minor, patch })
    }
}

/// Matches r2modman's real `mods.yml` entry schema exactly, verified
/// against a live profile on disk (~/.config/r2modmanPlus-local/REPO/
/// profiles/Default/mods.yml). Field order/naming here mirrors that file
/// so round-tripping (read, tweak, write) doesn't silently drop or
/// reorder anything r2modman itself expects to find.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModEntry {
    #[serde(rename = "manifestVersion")]
    pub manifest_version: u32,
    /// "<AuthorName>-<PackageName>", referred to elsewhere as the FullName.
    pub name: String,
    #[serde(rename = "authorName")]
    pub author_name: String,
    #[serde(rename = "websiteUrl")]
    pub website_url: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    pub description: String,
    #[serde(rename = "gameVersion")]
    pub game_version: String,
    #[serde(rename = "networkMode")]
    pub network_mode: String,
    #[serde(rename = "packageType")]
    pub package_type: String,
    #[serde(rename = "installMode")]
    pub install_mode: String,
    #[serde(rename = "installedAtTime")]
    pub installed_at_time: i64,
    pub loaders: Vec<String>,
    /// "<FullName>-<version>" strings, e.g. "BepInEx-BepInExPack-5.4.2305".
    pub dependencies: Vec<String>,
    pub incompatibilities: Vec<String>,
    #[serde(rename = "optionalDependencies")]
    pub optional_dependencies: Vec<String>,
    #[serde(rename = "versionNumber")]
    pub version_number: VersionNumber,
    pub enabled: bool,
    #[serde(rename = "onlineSource")]
    pub online_source: bool,
}

impl ModEntry {
    /// Pulls the Thunderstore community slug out of this entry's
    /// websiteUrl, e.g. "https://thunderstore.io/c/repo/p/Zehs/REPOLib/"
    /// -> "repo".
    pub fn community_slug(&self) -> Option<String> {
        let after_c = self.website_url.split("/c/").nth(1)?;
        let slug = after_c.split('/').next()?;
        (!slug.is_empty()).then(|| slug.to_string())
    }
}

pub fn read_mods_yml(profile_path: &Path) -> io::Result<Vec<ModEntry>> {
    let content = fs::read_to_string(profile_path.join("mods.yml"))?;
    serde_yaml::from_str(&content).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Overwrites mods.yml atomically: write to a sibling `.tmp` file, then
/// rename over the original. `rename` is atomic on the same filesystem on
/// both Linux and Windows/NTFS, so a crash mid-write can never leave a
/// half-written, corrupt mods.yml behind.
pub fn write_mods_yml(profile_path: &Path, entries: &[ModEntry]) -> io::Result<()> {
    let target = profile_path.join("mods.yml");
    let tmp = profile_path.join("mods.yml.tmp");
    let content = serde_yaml::to_string(entries)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    fs::write(&tmp, content)?;
    fs::rename(&tmp, &target)?;
    Ok(())
}

/// The community slug anchor: prefer BepInEx-BepInExPack since it's
/// present in essentially every real profile, but fall back to any entry
/// with a parseable websiteUrl so an unusual profile doesn't hard-fail.
pub fn find_community_slug(entries: &[ModEntry]) -> Option<String> {
    entries
        .iter()
        .find(|e| e.name == "BepInEx-BepInExPack")
        .and_then(|e| e.community_slug())
        .or_else(|| entries.iter().find_map(|e| e.community_slug()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses the real REPO profile on this dev machine end to end, as a
    /// sanity check that the schema above actually matches what r2modman
    /// writes (not just what the docs/observed sample said).
    #[test]
    fn parses_real_repo_profile() {
        let home = std::env::var("HOME").expect("HOME not set");
        let path = Path::new(&home)
            .join(".config/r2modmanPlus-local/REPO/profiles/Default");
        if !path.join("mods.yml").is_file() {
            eprintln!("skipping: no real r2modman REPO profile on this machine");
            return;
        }

        let entries = read_mods_yml(&path).expect("mods.yml should parse");
        assert!(!entries.is_empty(), "expected at least one mod entry");

        let bepinex = entries
            .iter()
            .find(|e| e.name == "BepInEx-BepInExPack")
            .expect("BepInEx-BepInExPack anchor entry should be present");
        assert_eq!(bepinex.community_slug().as_deref(), Some("repo"));

        let slug = find_community_slug(&entries);
        assert_eq!(slug.as_deref(), Some("repo"));

        let repolib = entries
            .iter()
            .find(|e| e.name == "Zehs-REPOLib")
            .expect("Zehs-REPOLib should be present in this real profile");
        assert!(repolib.dependencies.contains(&"BepInEx-BepInExPack-5.4.2305".to_string()));
    }
}

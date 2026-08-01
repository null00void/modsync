use super::mods_yml::ModEntry;
use serde::{Deserialize, Serialize};
use serde_json::json;

// The publishable key is Supabase's client-safe API key (the modern
// replacement for the old JWT "anon" key) -- it's meant to be embedded in
// shipped client code, same as it would be in any web frontend. All
// writes still go through SECURITY DEFINER RPCs that check owner_secret
// server-side, so this key alone can never mutate another owner's data.
const SUPABASE_URL: &str = "https://iqidgnlqgzmcteydkrfu.supabase.co";
const SUPABASE_PUBLISHABLE_KEY: &str = "sb_publishable__yaYm3JdT4Ff3k6FDF3JrQ_X8hLMznU";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnerIdentity {
    pub owner_id: String,
    pub share_code: String,
    pub owner_secret: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncedModEntry {
    pub full_name: String,
    pub author_name: String,
    pub display_name: String,
    pub version: String,
    pub enabled: bool,
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FriendProfilePayload {
    pub owner_id: String,
    pub display_name: Option<String>,
    pub community_slug: String,
    pub profile_name: String,
    pub mods: Vec<SyncedModEntry>,
    pub updated_at: String,
}

/// Builds the push payload from a real mods.yml: only mods that came from
/// Thunderstore (onlineSource) are shareable, since a friend's client can
/// only re-install packages it can actually fetch from the Thunderstore
/// API. Drops fields that are meaningless across machines
/// (installedAtTime, installMode, loaders, incompatibilities,
/// optionalDependencies).
pub fn build_push_payload(entries: &[ModEntry]) -> Vec<SyncedModEntry> {
    entries
        .iter()
        .filter(|e| e.online_source)
        .map(|e| SyncedModEntry {
            full_name: e.name.clone(),
            author_name: e.author_name.clone(),
            display_name: e.display_name.clone(),
            version: e.version_number.to_string(),
            enabled: e.enabled,
            dependencies: e.dependencies.clone(),
        })
        .collect()
}

fn client() -> reqwest::Client {
    reqwest::Client::new()
}

fn rpc_url(fn_name: &str) -> String {
    format!("{SUPABASE_URL}/rest/v1/rpc/{fn_name}")
}

/// Generates a short, human-typeable share code (uppercase, excludes
/// visually ambiguous characters like 0/O and 1/I/L).
pub fn generate_share_code() -> String {
    use rand::Rng;
    const ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";
    let mut rng = rand::thread_rng();
    (0..7)
        .map(|_| ALPHABET[rng.gen_range(0..ALPHABET.len())] as char)
        .collect()
}

/// Calls the create_owner RPC, retrying with a fresh share_code on a
/// uniqueness collision (extremely unlikely at this code length, but
/// cheap to handle correctly).
pub async fn create_owner(display_name: Option<&str>) -> Result<OwnerIdentity, String> {
    let http = client();

    for _ in 0..5 {
        let share_code = generate_share_code();
        let resp = http
            .post(rpc_url("create_owner"))
            .header("apikey", SUPABASE_PUBLISHABLE_KEY)
            .header("Authorization", format!("Bearer {SUPABASE_PUBLISHABLE_KEY}"))
            .header("Content-Type", "application/json")
            .json(&json!({
                "p_share_code": share_code,
                "p_display_name": display_name,
            }))
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if resp.status() == reqwest::StatusCode::CONFLICT {
            continue; // share_code collision, try another
        }
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("create_owner failed ({status}): {body}"));
        }

        #[derive(Deserialize)]
        struct Row {
            owner_id: String,
            share_code: String,
            owner_secret: String,
        }
        let rows: Vec<Row> = resp.json().await.map_err(|e| e.to_string())?;
        let row = rows.into_iter().next().ok_or("create_owner returned no rows")?;

        return Ok(OwnerIdentity {
            owner_id: row.owner_id,
            share_code: row.share_code,
            owner_secret: row.owner_secret,
            display_name: display_name.map(|s| s.to_string()),
        });
    }

    Err("could not allocate a unique share code after several attempts".to_string())
}

pub async fn upsert_synced_profile(
    identity: &OwnerIdentity,
    game_short_name: &str,
    community_slug: &str,
    profile_name: &str,
    mods: &[SyncedModEntry],
) -> Result<(), String> {
    let http = client();
    let resp = http
        .post(rpc_url("upsert_synced_profile"))
        .header("apikey", SUPABASE_PUBLISHABLE_KEY)
        .header("Authorization", format!("Bearer {SUPABASE_PUBLISHABLE_KEY}"))
        .header("Content-Type", "application/json")
        .json(&json!({
            "p_owner_id": identity.owner_id,
            "p_owner_secret": identity.owner_secret,
            "p_game_short_name": game_short_name,
            "p_community_slug": community_slug,
            "p_profile_name": profile_name,
            "p_mods": mods,
        }))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("upsert_synced_profile failed ({status}): {body}"));
    }
    Ok(())
}

pub async fn get_synced_profile(
    share_code: &str,
    game_short_name: &str,
) -> Result<Option<FriendProfilePayload>, String> {
    let http = client();
    let resp = http
        .post(rpc_url("get_synced_profile"))
        .header("apikey", SUPABASE_PUBLISHABLE_KEY)
        .header("Authorization", format!("Bearer {SUPABASE_PUBLISHABLE_KEY}"))
        .header("Content-Type", "application/json")
        .json(&json!({
            "p_share_code": share_code,
            "p_game_short_name": game_short_name,
        }))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("get_synced_profile failed ({status}): {body}"));
    }

    let rows: Vec<FriendProfilePayload> = resp.json().await.map_err(|e| e.to_string())?;
    Ok(rows.into_iter().next())
}

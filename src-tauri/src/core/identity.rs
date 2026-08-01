use crate::core::supabase_client::{self, OwnerIdentity};
use serde_json::json;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

const IDENTITY_STORE: &str = "identity.json";
const FRIENDS_STORE: &str = "friends.json";

#[derive(Debug, Clone, serde::Serialize)]
pub struct OwnerIdentityPublic {
    pub share_code: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Friend {
    pub share_code: String,
    pub nickname: String,
}

/// Loads the local owner identity if one exists, or creates a new one via
/// the create_owner RPC and persists it. This is the only place
/// owner_secret is read from disk -- callers that only need to *display*
/// the share code should use `get_or_create_identity` and read
/// `.share_code`/`.display_name`, never serialize the whole struct back
/// out to the frontend.
pub async fn get_or_create_identity(
    app: &AppHandle,
    display_name: Option<&str>,
) -> Result<OwnerIdentity, String> {
    let store = app.store(IDENTITY_STORE).map_err(|e| e.to_string())?;

    if let Some(value) = store.get("identity") {
        let identity: OwnerIdentity =
            serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
        return Ok(identity);
    }

    let identity = supabase_client::create_owner(display_name).await?;
    store.set("identity", json!(identity));
    store.save().map_err(|e| e.to_string())?;
    Ok(identity)
}

pub fn list_friends(app: &AppHandle) -> Result<Vec<Friend>, String> {
    let store = app.store(FRIENDS_STORE).map_err(|e| e.to_string())?;
    match store.get("list") {
        Some(value) => serde_json::from_value(value.clone()).map_err(|e| e.to_string()),
        None => Ok(Vec::new()),
    }
}

pub fn add_friend(app: &AppHandle, share_code: &str, nickname: &str) -> Result<Vec<Friend>, String> {
    let store = app.store(FRIENDS_STORE).map_err(|e| e.to_string())?;
    let mut friends = list_friends(app)?;

    let share_code = share_code.trim().to_uppercase();
    if let Some(existing) = friends.iter_mut().find(|f| f.share_code == share_code) {
        existing.nickname = nickname.to_string();
    } else {
        friends.push(Friend {
            share_code,
            nickname: nickname.to_string(),
        });
    }

    store.set("list", json!(friends));
    store.save().map_err(|e| e.to_string())?;
    Ok(friends)
}

pub fn remove_friend(app: &AppHandle, share_code: &str) -> Result<Vec<Friend>, String> {
    let store = app.store(FRIENDS_STORE).map_err(|e| e.to_string())?;
    let share_code = share_code.trim().to_uppercase();
    let mut friends = list_friends(app)?;
    friends.retain(|f| f.share_code != share_code);

    store.set("list", json!(friends));
    store.save().map_err(|e| e.to_string())?;
    Ok(friends)
}

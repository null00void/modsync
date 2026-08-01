use crate::core::identity::{self, Friend, OwnerIdentityPublic};
use tauri::AppHandle;

#[tauri::command]
pub async fn get_or_create_owner_identity(
    app: AppHandle,
    display_name: Option<String>,
) -> Result<OwnerIdentityPublic, String> {
    super::log_err("get_or_create_owner_identity", async {
        let identity = identity::get_or_create_identity(&app, display_name.as_deref()).await?;
        Ok(OwnerIdentityPublic {
            share_code: identity.share_code,
            display_name: identity.display_name,
        })
    }.await)
}

#[tauri::command]
pub fn list_friends(app: AppHandle) -> Result<Vec<Friend>, String> {
    super::log_err("list_friends", identity::list_friends(&app))
}

#[tauri::command]
pub fn add_friend(app: AppHandle, share_code: String, nickname: String) -> Result<Vec<Friend>, String> {
    super::log_err("add_friend", identity::add_friend(&app, &share_code, &nickname))
}

#[tauri::command]
pub fn remove_friend(app: AppHandle, share_code: String) -> Result<Vec<Friend>, String> {
    super::log_err("remove_friend", identity::remove_friend(&app, &share_code))
}

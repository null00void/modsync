use crate::core::identity::{self, Friend, OwnerIdentityPublic};
use tauri::AppHandle;

#[tauri::command]
pub async fn get_or_create_owner_identity(
    app: AppHandle,
    display_name: Option<String>,
) -> Result<OwnerIdentityPublic, String> {
    let identity = identity::get_or_create_identity(&app, display_name.as_deref()).await?;
    Ok(OwnerIdentityPublic {
        share_code: identity.share_code,
        display_name: identity.display_name,
    })
}

#[tauri::command]
pub fn list_friends(app: AppHandle) -> Result<Vec<Friend>, String> {
    identity::list_friends(&app)
}

#[tauri::command]
pub fn add_friend(app: AppHandle, share_code: String, nickname: String) -> Result<Vec<Friend>, String> {
    identity::add_friend(&app, &share_code, &nickname)
}

#[tauri::command]
pub fn remove_friend(app: AppHandle, share_code: String) -> Result<Vec<Friend>, String> {
    identity::remove_friend(&app, &share_code)
}

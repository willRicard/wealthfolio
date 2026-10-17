use crate::profiles::ProfileAccess;
use serde::Deserialize;
use wealthfolio_procurement::{
    items::{Item, ItemWithStats, SaveItem},
    photo_drafts::PhotoDraft,
    purchases::{ItemPrices, Purchase, SavePurchase},
};

#[tauri::command]
pub async fn get_procurement_items(state: ProfileAccess) -> Result<Vec<ItemWithStats>, String> {
    let context = state.context()?;
    context
        .procurement_items
        .list_with_stats()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_procurement_item(state: ProfileAccess, id: String) -> Result<Item, String> {
    let context = state.context()?;
    context
        .procurement_items
        .get(&id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_procurement_item(
    state: ProfileAccess,
    input: SaveItem,
) -> Result<Item, String> {
    let context = state.context()?;
    context
        .procurement_items
        .create(input)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_procurement_item(
    state: ProfileAccess,
    id: String,
    input: SaveItem,
) -> Result<Item, String> {
    let context = state.context()?;
    context
        .procurement_items
        .update(&id, input)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn clone_procurement_item(state: ProfileAccess, id: String) -> Result<Item, String> {
    let context = state.context()?;
    context
        .procurement_items
        .clone_item(&id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_procurement_item(state: ProfileAccess, id: String) -> Result<(), String> {
    let context = state.context()?;
    context
        .procurement_items
        .delete(&id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_procurement_item_prices(
    state: ProfileAccess,
    id: String,
) -> Result<ItemPrices, String> {
    let context = state.context()?;
    context
        .procurement_purchases
        .list_for_item(&id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_procurement_activity_purchases(
    state: ProfileAccess,
    activity_id: String,
) -> Result<Vec<Purchase>, String> {
    let context = state.context()?;
    context
        .procurement_purchases
        .list_for_activity(&activity_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_procurement_activity_purchases(
    state: ProfileAccess,
    activity_id: String,
    purchases: Vec<SavePurchase>,
) -> Result<Vec<Purchase>, String> {
    let context = state.context()?;
    context
        .procurement_purchases
        .set_for_activity(&activity_id, purchases)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn discard_procurement_photo_draft(
    state: ProfileAccess,
    id: String,
) -> Result<(), String> {
    let context = state.context()?;
    context
        .procurement_photos
        .discard(&id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn upload_procurement_photo_draft(
    state: ProfileAccess,
    request: tauri::ipc::Request<'_>,
) -> Result<PhotoDraft, String> {
    let filename = request
        .headers()
        .get("x-wf-filename")
        .and_then(|h| h.to_str().ok())
        .ok_or("Missing filename")?;
    let filename = urlencoding::decode(filename)
        .map_err(|_| "Invalid filename")?
        .into_owned();
    let content_type = request
        .headers()
        .get("x-wf-content-type")
        .and_then(|h| h.to_str().ok())
        .ok_or("Missing content type")?
        .to_string();
    let bytes = match request.body() {
        tauri::ipc::InvokeBody::Raw(bytes) => bytes.clone(),
        tauri::ipc::InvokeBody::Json(value) => {
            Vec::<u8>::deserialize(value).map_err(|_| "Photo payload must be bytes")?
        }
    };
    let context = state.context()?;
    let slot = context
        .procurement_photos
        .upload_slot()
        .await
        .map_err(|e| e.to_string())?;
    context
        .procurement_photos
        .upload(&filename, &content_type, bytes, slot)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn read_procurement_photo(
    state: ProfileAccess,
    id: String,
) -> Result<tauri::ipc::Response, String> {
    let context = state.context()?;
    let item = context
        .procurement_items
        .get(&id)
        .map_err(|e| e.to_string())?;
    let photo = item.photo.ok_or("Item has no photo")?;
    context
        .procurement_photos
        .read(&photo, true)
        .await
        .map(tauri::ipc::Response::new)
        .map_err(|e| e.to_string())
}

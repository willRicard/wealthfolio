use crate::{
    error::{ApiError, ApiResult},
    main_lib::AppState,
};
use axum::{
    extract::{DefaultBodyLimit, Multipart, Path},
    http::{header, HeaderMap},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
    Extension, Json, Router,
};
use std::sync::Arc;
use wealthfolio_procurement::{
    items::{Item, ItemWithStats, SaveItem},
    photo_drafts::PhotoDraft,
    purchases::{ItemPrices, Purchase, SavePurchase},
};

async fn get_procurement_items(
    Extension(context): Extension<Arc<AppState>>,
) -> ApiResult<Json<Vec<ItemWithStats>>> {
    Ok(Json(context.procurement_items.list_with_stats()?))
}

async fn get_procurement_item(
    Extension(context): Extension<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<Json<Item>> {
    Ok(Json(context.procurement_items.get(&id)?))
}

async fn create_procurement_item(
    Extension(context): Extension<Arc<AppState>>,
    Json(input): Json<SaveItem>,
) -> ApiResult<Json<Item>> {
    Ok(Json(context.procurement_items.create(input).await?))
}

async fn update_procurement_item(
    Extension(context): Extension<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<SaveItem>,
) -> ApiResult<Json<Item>> {
    Ok(Json(context.procurement_items.update(&id, input).await?))
}

async fn clone_procurement_item(
    Extension(context): Extension<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<Json<Item>> {
    Ok(Json(context.procurement_items.clone_item(&id).await?))
}

async fn delete_procurement_item(
    Extension(context): Extension<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<Json<()>> {
    Ok(Json(context.procurement_items.delete(&id).await?))
}

async fn get_procurement_item_prices(
    Extension(context): Extension<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<Json<ItemPrices>> {
    Ok(Json(context.procurement_purchases.list_for_item(&id)?))
}

async fn create_procurement_purchase(
    Extension(context): Extension<Arc<AppState>>,
    Json(input): Json<SavePurchase>,
) -> ApiResult<Json<Purchase>> {
    Ok(Json(context.procurement_purchases.create(input).await?))
}

async fn update_procurement_purchase(
    Extension(context): Extension<Arc<AppState>>,
    Path((activity_id, item_id)): Path<(String, String)>,
    Json(input): Json<SavePurchase>,
) -> ApiResult<Json<Purchase>> {
    if input.activity_id != activity_id || input.item_id != item_id {
        return Err(ApiError::BadRequest(
            "Purchase identifiers must match the route".into(),
        ));
    }
    Ok(Json(context.procurement_purchases.update(input).await?))
}

async fn delete_procurement_purchase(
    Extension(context): Extension<Arc<AppState>>,
    Path((activity_id, item_id)): Path<(String, String)>,
) -> ApiResult<Json<()>> {
    Ok(Json(
        context
            .procurement_purchases
            .delete(&activity_id, &item_id)
            .await?,
    ))
}

async fn discard_procurement_photo_draft(
    Extension(context): Extension<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<Json<()>> {
    Ok(Json(context.procurement_photos.discard(&id).await?))
}

async fn upload(
    Extension(state): Extension<Arc<AppState>>,
    Extension(profiles): Extension<Arc<crate::profiles::WebProfiles>>,
    headers: HeaderMap,
    multipart: Multipart,
) -> ApiResult<Json<PhotoDraft>> {
    if !profiles.allows_browser_origin(&headers) {
        return Err(ApiError::Forbidden(
            "Cross-site photo access is not allowed".into(),
        ));
    }
    let slot = state.procurement_photos.upload_slot().await?;
    let (filename, content_type, bytes) =
        super::transaction_attachments::one_file(multipart).await?;
    Ok(Json(
        state
            .procurement_photos
            .upload(&filename, &content_type, bytes, slot)
            .await?,
    ))
}
async fn photo(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    let item = state.procurement_items.get(&id)?;
    let photo = item.photo.ok_or(ApiError::NotFound)?;
    let mut response = state
        .procurement_photos
        .read(&photo, true)
        .await?
        .into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        "image/webp".parse().expect("static header"),
    );
    headers.insert(
        header::CACHE_CONTROL,
        "private, no-store".parse().expect("static header"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        "nosniff".parse().expect("static header"),
    );
    Ok(response)
}
pub fn router<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route(
            "/procurement/items",
            get(get_procurement_items).post(create_procurement_item),
        )
        .route(
            "/procurement/items/{id}",
            get(get_procurement_item)
                .put(update_procurement_item)
                .delete(delete_procurement_item),
        )
        .route(
            "/procurement/items/{id}/clone",
            post(clone_procurement_item),
        )
        .route(
            "/procurement/items/{id}/prices",
            get(get_procurement_item_prices),
        )
        .route("/procurement/items/{id}/photo", get(photo))
        .route("/procurement/purchases", post(create_procurement_purchase))
        .route(
            "/procurement/purchases/{activity_id}/{item_id}",
            put(update_procurement_purchase).delete(delete_procurement_purchase),
        )
        .route(
            "/procurement/photo-drafts/{id}",
            delete(discard_procurement_photo_draft),
        )
        .route(
            "/procurement/photo-drafts",
            post(upload).layer(DefaultBodyLimit::max(
                wealthfolio_procurement::photos::MAX_PHOTO_BYTES + 64 * 1024,
            )),
        )
}

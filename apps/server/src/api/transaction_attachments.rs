use crate::{
    error::{ApiError, ApiResult},
    main_lib::AppState,
};
use axum::{
    extract::{DefaultBodyLimit, Multipart, Path},
    http::{header, HeaderMap, HeaderValue},
    response::{IntoResponse, Response},
    routing::get,
    Extension, Json, Router,
};
use std::sync::Arc;
use wealthfolio_spending::transaction_attachments::{
    AttachmentDraft, TransactionAttachment, MAX_ATTACHMENT_BYTES,
};

async fn attachments(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<TransactionAttachment>>> {
    Ok(Json(state.transaction_attachments_service.list(&id).await?))
}

pub(super) async fn one_file(mut multipart: Multipart) -> ApiResult<(String, String, Vec<u8>)> {
    let mut field = multipart
        .next_field()
        .await
        .map_err(|_| ApiError::BadRequest("Invalid upload".into()))?
        .ok_or_else(|| ApiError::BadRequest("Missing attachment".into()))?;
    if field.name() != Some("file") {
        return Err(ApiError::BadRequest("Expected one file field".into()));
    }
    let filename = field.file_name().unwrap_or_default().to_owned();
    let content_type = field.content_type().unwrap_or_default().to_owned();
    let mut bytes = Vec::new();
    while let Some(chunk) = field
        .chunk()
        .await
        .map_err(|_| ApiError::BadRequest("Invalid or oversized upload".into()))?
    {
        if bytes.len() + chunk.len() > MAX_ATTACHMENT_BYTES {
            return Err(ApiError::BadRequest("Maximum file size is 20 MiB".into()));
        }
        bytes.extend_from_slice(&chunk);
    }
    drop(field);
    if multipart
        .next_field()
        .await
        .map_err(|_| ApiError::BadRequest("Invalid upload".into()))?
        .is_some()
    {
        return Err(ApiError::BadRequest("Upload one file at a time".into()));
    }
    Ok((filename, content_type, bytes))
}

async fn upload(
    Extension(state): Extension<Arc<AppState>>,
    Extension(profiles): Extension<Arc<crate::profiles::WebProfiles>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    multipart: Multipart,
) -> ApiResult<Json<TransactionAttachment>> {
    same_origin(&profiles, &headers)?;
    state
        .transaction_attachments_service
        .check_access(&id)
        .await?;
    let (filename, content_type, bytes) = one_file(multipart).await?;
    let slot = state.transaction_attachment_files.upload_slot().await?;
    Ok(Json(
        state
            .transaction_attachment_files
            .upload(&id, &filename, &content_type, bytes, slot)
            .await?,
    ))
}

fn file_response(
    filename: &str,
    content_type: &str,
    bytes: Vec<u8>,
    preview: bool,
) -> ApiResult<Response> {
    let mut response = bytes.into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(if preview { "image/webp" } else { content_type })
            .map_err(|_| ApiError::Internal("Invalid attachment type".into()))?,
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("sandbox; default-src 'none'"),
    );
    if !preview {
        headers.insert(
            header::CONTENT_DISPOSITION,
            HeaderValue::from_str(&format!(
                "inline; filename*=UTF-8''{}",
                urlencoding::encode(filename)
            ))
            .map_err(|_| ApiError::Internal("Invalid attachment filename".into()))?,
        );
    }
    Ok(response)
}
async fn attachment_file(
    state: Arc<AppState>,
    activity_id: String,
    attachment_id: String,
    preview: bool,
) -> ApiResult<Response> {
    let (attachment, bytes) = state
        .transaction_attachment_files
        .read(&activity_id, &attachment_id, preview)
        .await?;
    file_response(
        &attachment.filename,
        &attachment.content_type,
        bytes,
        preview,
    )
}
async fn thumbnail(
    Extension(state): Extension<Arc<AppState>>,
    Path((activity_id, id)): Path<(String, String)>,
) -> ApiResult<Response> {
    attachment_file(state, activity_id, id, true).await
}
async fn original(
    Extension(state): Extension<Arc<AppState>>,
    Path((activity_id, id)): Path<(String, String)>,
) -> ApiResult<Response> {
    attachment_file(state, activity_id, id, false).await
}
async fn delete(
    Extension(state): Extension<Arc<AppState>>,
    Path((activity_id, id)): Path<(String, String)>,
) -> ApiResult<()> {
    state
        .transaction_attachment_files
        .delete(&activity_id, &id)
        .await?;
    Ok(())
}

fn same_origin(profiles: &crate::profiles::WebProfiles, headers: &HeaderMap) -> ApiResult<()> {
    if profiles.allows_browser_origin(headers) {
        return Ok(());
    }
    Err(ApiError::Forbidden(
        "Cross-site attachment access is not allowed".into(),
    ))
}

async fn drafts(
    Extension(state): Extension<Arc<AppState>>,
    Extension(profiles): Extension<Arc<crate::profiles::WebProfiles>>,
    Path(token): Path<String>,
    headers: HeaderMap,
) -> ApiResult<Json<Vec<AttachmentDraft>>> {
    same_origin(&profiles, &headers)?;
    Ok(Json(
        state
            .transaction_attachments_service
            .list_drafts(&token)
            .await?,
    ))
}

async fn upload_draft(
    Extension(state): Extension<Arc<AppState>>,
    Extension(profiles): Extension<Arc<crate::profiles::WebProfiles>>,
    Path(token): Path<String>,
    headers: HeaderMap,
    multipart: Multipart,
) -> ApiResult<Json<AttachmentDraft>> {
    same_origin(&profiles, &headers)?;
    if let Err(error) = state.transaction_attachment_files.sweep_drafts().await {
        tracing::warn!("Failed to sweep abandoned attachment drafts: {error}");
    }
    let (filename, content_type, bytes) = one_file(multipart).await?;
    let slot = state.transaction_attachment_files.upload_slot().await?;
    Ok(Json(
        state
            .transaction_attachment_files
            .upload_draft(&token, &filename, &content_type, bytes, slot)
            .await?,
    ))
}

async fn discard_drafts(
    Extension(state): Extension<Arc<AppState>>,
    Extension(profiles): Extension<Arc<crate::profiles::WebProfiles>>,
    Path(token): Path<String>,
    headers: HeaderMap,
) -> ApiResult<()> {
    same_origin(&profiles, &headers)?;
    state
        .transaction_attachment_files
        .discard_drafts(&token)
        .await?;
    Ok(())
}

async fn draft_file(
    state: Arc<AppState>,
    token: String,
    id: String,
    preview: bool,
) -> ApiResult<Response> {
    let (draft, bytes) = state
        .transaction_attachment_files
        .read_draft(&token, &id, preview)
        .await?;
    file_response(&draft.filename, &draft.content_type, bytes, preview)
}

async fn draft_thumbnail(
    Extension(state): Extension<Arc<AppState>>,
    Extension(profiles): Extension<Arc<crate::profiles::WebProfiles>>,
    Path((token, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    same_origin(&profiles, &headers)?;
    draft_file(state, token, id, true).await
}

async fn draft_original(
    Extension(state): Extension<Arc<AppState>>,
    Extension(profiles): Extension<Arc<crate::profiles::WebProfiles>>,
    Path((token, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    same_origin(&profiles, &headers)?;
    draft_file(state, token, id, false).await
}

async fn delete_draft(
    Extension(state): Extension<Arc<AppState>>,
    Extension(profiles): Extension<Arc<crate::profiles::WebProfiles>>,
    Path((token, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<()> {
    same_origin(&profiles, &headers)?;
    state
        .transaction_attachment_files
        .delete_draft(&token, &id)
        .await?;
    Ok(())
}

async fn claim_drafts(
    Extension(state): Extension<Arc<AppState>>,
    Extension(profiles): Extension<Arc<crate::profiles::WebProfiles>>,
    Path((activity_id, token)): Path<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<usize>> {
    same_origin(&profiles, &headers)?;
    Ok(Json(
        state
            .transaction_attachment_files
            .claim_drafts(&token, &activity_id)
            .await?,
    ))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReorderRequest {
    attachment_ids: Vec<String>,
}

async fn reorder(
    Extension(state): Extension<Arc<AppState>>,
    Extension(profiles): Extension<Arc<crate::profiles::WebProfiles>>,
    Path(activity_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<ReorderRequest>,
) -> ApiResult<()> {
    same_origin(&profiles, &headers)?;
    state
        .transaction_attachment_files
        .reorder(&activity_id, &body.attachment_ids)
        .await?;
    Ok(())
}

pub fn router() -> Router {
    let body_limit = || DefaultBodyLimit::max(MAX_ATTACHMENT_BYTES + 64 * 1024);
    Router::new()
        .route(
            "/spending/transactions/{id}/attachments",
            get(attachments).post(upload).layer(body_limit()),
        )
        .route(
            "/spending/transactions/{activity_id}/attachments/{id}",
            get(original).delete(delete),
        )
        .route(
            "/spending/transactions/{activity_id}/attachments/{id}/thumbnail",
            get(thumbnail),
        )
        .route(
            "/spending/transactions/{activity_id}/attachments/order",
            axum::routing::put(reorder),
        )
        .route(
            "/spending/attachment-drafts/{token}",
            get(drafts)
                .post(upload_draft)
                .delete(discard_drafts)
                .layer(body_limit()),
        )
        .route(
            "/spending/attachment-drafts/{token}/{id}",
            get(draft_original).delete(delete_draft),
        )
        .route(
            "/spending/attachment-drafts/{token}/{id}/thumbnail",
            get(draft_thumbnail),
        )
        .route(
            "/spending/transactions/{activity_id}/attachment-drafts/{token}",
            axum::routing::post(claim_drafts),
        )
}

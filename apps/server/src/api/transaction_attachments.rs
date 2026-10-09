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
use wealthfolio_spending::transaction_attachments::{TransactionAttachment, MAX_ATTACHMENT_BYTES};

async fn attachments(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<TransactionAttachment>>> {
    Ok(Json(state.transaction_attachments_service.list(&id).await?))
}

async fn upload(
    Extension(state): Extension<Arc<AppState>>,
    Extension(profiles): Extension<Arc<crate::profiles::WebProfiles>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> ApiResult<Json<TransactionAttachment>> {
    // Multipart is a browser "simple request"; reuse profile commands' origin
    // checks so a cross-site form cannot upload using the browser's session.
    if !profiles.allows_browser_origin(&headers) {
        return Err(ApiError::Forbidden(
            "Cross-site attachment uploads are not allowed".into(),
        ));
    }
    state
        .transaction_attachments_service
        .check_access(&id)
        .await?;
    let slot = state.transaction_attachment_files.upload_slot().await?;
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
    Ok(Json(
        state
            .transaction_attachment_files
            .upload(&id, &filename, &content_type, bytes, slot)
            .await?,
    ))
}

async fn file_response(
    state: Arc<AppState>,
    activity_id: String,
    attachment_id: String,
    preview: bool,
) -> ApiResult<Response> {
    let (attachment, bytes) = state
        .transaction_attachment_files
        .read(&activity_id, &attachment_id, preview)
        .await?;
    let mut response = bytes.into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(if preview {
            "image/webp"
        } else {
            &attachment.content_type
        })
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
                urlencoding::encode(&attachment.filename)
            ))
            .map_err(|_| ApiError::Internal("Invalid attachment filename".into()))?,
        );
    }
    Ok(response)
}
async fn thumbnail(
    Extension(state): Extension<Arc<AppState>>,
    Path((activity_id, id)): Path<(String, String)>,
) -> ApiResult<Response> {
    file_response(state, activity_id, id, true).await
}
async fn original(
    Extension(state): Extension<Arc<AppState>>,
    Path((activity_id, id)): Path<(String, String)>,
) -> ApiResult<Response> {
    file_response(state, activity_id, id, false).await
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

pub fn router() -> Router {
    Router::new()
        .route(
            "/spending/transactions/{id}/attachments",
            get(attachments)
                .post(upload)
                .layer(DefaultBodyLimit::max(MAX_ATTACHMENT_BYTES + 64 * 1024)),
        )
        .route(
            "/spending/transactions/{activity_id}/attachments/{id}",
            get(original).delete(delete),
        )
        .route(
            "/spending/transactions/{activity_id}/attachments/{id}/thumbnail",
            get(thumbnail),
        )
}

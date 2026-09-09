use super::{AppState, require_auth};
use axum::{
    Json,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};

fn error(status: StatusCode, code: &str, message: &str) -> Response {
    (
        status,
        Json(serde_json::json!({"code": code, "error": message})),
    )
        .into_response()
}

pub async fn handle_api_image_upload(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    bytes: Bytes,
) -> Response {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }
    let Some(backend) = state.session_backend.clone() else {
        return error(
            StatusCode::NOT_IMPLEMENTED,
            "ATTACHMENTS_UNSUPPORTED",
            "Image attachments require session persistence",
        );
    };
    // Decoding and durable filesystem writes must not block the async runtime.
    let result = tokio::task::spawn_blocking(move || {
        backend.cleanup_images()?;
        backend.upload_image(&format!("gw_{id}"), crate::LOCAL_OWNER_USER_ID, &bytes)
    })
    .await;
    match result {
        Ok(Ok(image)) => (StatusCode::CREATED, Json(image)).into_response(),
        Ok(Err(e)) => error(
            StatusCode::BAD_REQUEST,
            "IMAGE_UPLOAD_FAILED",
            &e.to_string(),
        ),
        Err(_) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "IMAGE_UPLOAD_FAILED",
            "Unable to process image",
        ),
    }
}

pub async fn handle_api_image_read(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((session_id, id)): Path<(String, String)>,
) -> Response {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }
    let Some(backend) = state.session_backend.clone() else {
        return error(
            StatusCode::NOT_IMPLEMENTED,
            "ATTACHMENTS_UNSUPPORTED",
            "Image attachments require session persistence",
        );
    };
    let result = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let images = backend.resolve_images(
            &format!("gw_{session_id}"),
            crate::LOCAL_OWNER_USER_ID,
            &[id],
        )?;
        let image = &images[0];
        let bytes = std::fs::read(
            image
                .resolved_path
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Image unavailable"))?,
        )?;
        Ok((image.mime_type.clone(), bytes))
    })
    .await;
    match result {
        Ok(Ok((mime, bytes))) => (
            [
                (header::CONTENT_TYPE, mime),
                (header::CACHE_CONTROL, "private, no-store".into()),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff".into()),
            ],
            bytes,
        )
            .into_response(),
        _ => error(
            StatusCode::NOT_FOUND,
            "ATTACHMENT_UNAVAILABLE",
            "Image is unavailable or belongs to another session",
        ),
    }
}

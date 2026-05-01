//! Static file serving for the web dashboard.

use axum::{
    extract::State,
    http::{StatusCode, header},
    response::{Html, IntoResponse},
};
use super::AppState;

/// GET /_app/{*path} — serve static assets from web/dist
pub async fn handle_static(
    State(state): State<AppState>,
    axum::extract::Path(path): axum::extract::Path<String>,
) -> impl IntoResponse {
    let Some(ref dir) = state.web_dist_dir else {
        return (StatusCode::NOT_FOUND, "Static files not available").into_response();
    };

    let file_path = dir.join("_app").join(&path);

    match tokio::fs::read(&file_path).await {
        Ok(bytes) => {
            let content_type = guess_content_type(&path);
            (StatusCode::OK, [(header::CONTENT_TYPE, content_type)], bytes).into_response()
        }
        Err(_) => (StatusCode::NOT_FOUND, "File not found").into_response(),
    }
}

/// SPA fallback: non-API GET requests serve index.html
pub async fn handle_spa_fallback(State(state): State<AppState>) -> impl IntoResponse {
    let Some(ref dir) = state.web_dist_dir else {
        return (StatusCode::NOT_FOUND, "Web dashboard not available").into_response();
    };

    let index_path = dir.join("index.html");
    match tokio::fs::read_to_string(&index_path).await {
        Ok(html) => Html(html).into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "Dashboard not found").into_response(),
    }
}

fn guess_content_type(path: &str) -> &'static str {
    if path.ends_with(".js") {
        "application/javascript"
    } else if path.ends_with(".css") {
        "text/css"
    } else if path.ends_with(".html") {
        "text/html"
    } else if path.ends_with(".png") {
        "image/png"
    } else if path.ends_with(".svg") {
        "image/svg+xml"
    } else if path.ends_with(".ico") {
        "image/x-icon"
    } else if path.ends_with(".woff2") {
        "font/woff2"
    } else if path.ends_with(".woff") {
        "font/woff"
    } else {
        "application/octet-stream"
    }
}

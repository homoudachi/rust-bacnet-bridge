use axum::{
    body::Bytes,
    http::{header, StatusCode},
    response::IntoResponse,
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "assets/"]
struct Assets;

fn serve_asset(path: &str) -> impl IntoResponse {
    match Assets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path)
                .first_or_octet_stream()
                .to_string();
            let body = Bytes::copy_from_slice(&content.data);
            (StatusCode::OK, [(header::CONTENT_TYPE, mime)], body)
        }
        None => (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "text/plain".to_string())],
            Bytes::from("not found"),
        ),
    }
}

pub async fn index() -> impl IntoResponse {
    serve_asset("index.html")
}

pub async fn style_css() -> impl IntoResponse {
    serve_asset("style.css")
}

pub async fn app_js() -> impl IntoResponse {
    serve_asset("app.js")
}

pub async fn favicon() -> impl IntoResponse {
    serve_asset("favicon.ico")
}

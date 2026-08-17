use axum::http::{Response, StatusCode};

use crate::service;

pub async fn hello() -> Response<String> {
    Response::builder()
        .status(StatusCode::OK)
        .body(service::hello().await)
        .unwrap_or_default()
}

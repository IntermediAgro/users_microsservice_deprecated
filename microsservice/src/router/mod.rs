use axum::{routing::get, Router};

use crate::controller;

pub fn hello() -> Router {
    Router::new().route("/", get(controller::hello))
}

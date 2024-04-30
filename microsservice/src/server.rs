use axum::{http::Error, Router, ServiceExt};
use tokio::net::TcpListener;

use crate::router;

pub async fn startup() -> Result<(), Error> {
    let routes = Router::new().merge(router::hello());

    let listener = TcpListener::bind("0.0.0.0:8080")
        .await
        .expect("Failed to bind port 8080");

    println!("Listening on port 8080");
    axum::serve(listener, routes.into_make_service())
        .await
        .unwrap();

    Ok(())
}

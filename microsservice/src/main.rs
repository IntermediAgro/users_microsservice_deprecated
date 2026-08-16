use std::env;

use axum::http::Result;
use database::connect_postgres;

mod controller;
mod router;
mod server;
mod service;

//const SECRET_KEY: &str = env!("SECRET_KEY");

#[tokio::main]
async fn main() -> Result<()> {
    //    println!("Hello, world! {:?}", SECRET_KEY);

    connect_postgres(env::var("DATABASE_URL").unwrap()).await;

    server::startup().await
}

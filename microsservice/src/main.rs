mod controller;
mod router;
mod server;
mod service;

//const SECRET_KEY: &str = env!("SECRET_KEY");

#[tokio::main]
async fn main() {
    //    println!("Hello, world! {:?}", SECRET_KEY);
    server::startup().await.unwrap()
}

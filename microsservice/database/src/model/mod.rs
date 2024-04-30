pub mod postgres;

use sqlx::PgConnection;

pub trait Database<C, P> {
    async fn new(url: &str) -> Self;

    fn get_connection() -> C;

    fn get_pool() -> P;
}

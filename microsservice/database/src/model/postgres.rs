use sqlx::PgPool;

use super::{Database, Db};

pub struct Postgres;

impl Database<PgPool> for Postgres {
    async fn new(url: &'static str) -> Db<PgPool> {
        Db {
            url,
            //connection: PgConnection::connect(url).await.unwrap(),
            pool: PgPool::connect(url).await.unwrap(),
        }
    }
}

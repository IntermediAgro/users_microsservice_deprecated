use sqlx::{Error, PgPool};

use super::{Database, Db};

pub struct Postgres;

impl Database<PgPool> for Postgres {
    async fn new(url: String) -> Result<Db<PgPool>, Error> {
        Ok(Db {
            pool: PgPool::connect(url.as_str()).await.unwrap(),
            url,
        })
    }
}

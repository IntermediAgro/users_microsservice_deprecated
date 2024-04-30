use sqlx::{Connection, PgConnection, PgPool};

use super::Database;

pub struct Postgres {
    pub url: String,
    pub connection: PgConnection,
    pub pool: PgPool,
}

impl Database<PgConnection, PgPool> for Postgres {
    async fn new(url: &str) -> Self {
        Self {
            url: url.to_string(),
            connection: PgConnection::connect(url).await.unwrap(),
            pool: PgPool::connect(url).await.unwrap(),
        }
    }

    fn get_connection() -> PgConnection {
        todo!()
    }

    fn get_pool() -> PgPool {
        todo!()
    }
}

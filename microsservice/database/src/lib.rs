use model::{postgres::Postgres, Database, Db};
use sqlx::PgPool;
mod model;

pub async fn connect_postgres(url: &'static str) -> Db<PgPool> {
    Postgres::new(url).await.unwrap()
}

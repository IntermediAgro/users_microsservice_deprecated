use sqlx::Error;

pub mod postgres;

pub struct Db<P> {
    pub url: String,
    //    pub connection: C,
    pub pool: P,
}

pub trait Database<P> {
    async fn new(url: String) -> Result<Db<P>, Error>;
}

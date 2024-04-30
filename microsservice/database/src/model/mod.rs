use sqlx::Error;

pub mod postgres;

pub struct Db<P> {
    pub url: &'static str,
    //    pub connection: C,
    pub pool: P,
}

pub trait Database<P> {
    async fn new(url: &'static str) -> Result<Db<P>, Error>;
}

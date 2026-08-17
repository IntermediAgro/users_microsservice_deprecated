pub mod model;

use crate::model::{claims::Claims, user::User};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};

use std::env;

//const SECRET_KEY: &str = env!("SECRET_KEY");

pub fn encode_jwt(user: User) -> Result<String, String> {
    let SECRET_KEY = env::var("SECRET_KEY").expect("SECRET_KEY must be set");

    let claims = Claims {
        email: user.email,
        exp: (Utc::now() + Duration::days(1)).timestamp(),
    };
    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(SECRET_KEY.as_bytes()),
    )
    .map_err(|e| e.to_string());

    return token;
}

pub fn decode_jwt(token: &str) -> Result<User, String> {
    let SECRET_KEY = env::var("SECRET_KEY").expect("SECRET_KEY must be set");

    let token_data = decode::<User>(
        token,
        &DecodingKey::from_secret(SECRET_KEY.as_bytes()),
        &Validation::default(),
    );

    match token_data {
        Ok(token_data) => Ok(token_data.claims),
        Err(e) => Err(e.to_string()),
    }
}

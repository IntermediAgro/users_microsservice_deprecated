pub mod model;

use crate::model::{claims::Claims, user::User};
use chrono::{Duration, Utc};
use jsonwebtoken::{encode, EncodingKey, Header};

pub fn encode_jwt(user: User) -> Result<String, String> {
    let claims = Claims {
        email: user.email,
        exp: (Utc::now() + Duration::days(1)).timestamp(),
    };
    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret("mykey".as_bytes()),
    )
    .map_err(|e| e.to_string());

    return token;
}

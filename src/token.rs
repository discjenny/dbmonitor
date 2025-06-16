use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use std::env;

static SECRET_KEY: LazyLock<String> = LazyLock::new(|| {
    env::var("DEVICE_TOKEN_SECRET").unwrap_or_else(|_| {
        eprintln!("DEVICE_TOKEN_SECRET not set, using insecure default key");
        "69420".to_string()
    })
});

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub device_id: i32,
}

pub fn generate_token(device_id: i32) -> Result<String, jsonwebtoken::errors::Error> {
    let claims = Claims { device_id };
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(SECRET_KEY.as_bytes()),
    )
}

pub fn verify_token(token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = false; // token never expires
    validation.required_spec_claims.clear(); // don't require "exp" claim
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(SECRET_KEY.as_bytes()),
        &validation,
    )?;
    Ok(token_data.claims)
} 
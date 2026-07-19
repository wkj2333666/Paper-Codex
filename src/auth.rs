use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone)]
pub struct Auth {
    password_hash: Arc<String>,
    jwt_secret: Arc<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    exp: usize,
    iat: usize,
}

impl Auth {
    pub fn new(password_hash: String, jwt_secret: String) -> Self {
        Self {
            password_hash: Arc::new(password_hash),
            jwt_secret: Arc::new(jwt_secret),
        }
    }

    pub async fn login(&self, password: String) -> anyhow::Result<String> {
        let hash = self.password_hash.clone();
        let valid = tokio::task::spawn_blocking(move || bcrypt::verify(password, &hash)).await??;
        if !valid {
            anyhow::bail!("invalid credentials");
        }
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as usize;
        Ok(encode(
            &Header::default(),
            &Claims {
                sub: "paper-user".into(),
                iat: now,
                exp: now + 7 * 24 * 3600,
            },
            &EncodingKey::from_secret(self.jwt_secret.as_bytes()),
        )?)
    }

    pub fn verify(&self, token: &str) -> bool {
        decode::<Claims>(
            token,
            &DecodingKey::from_secret(self.jwt_secret.as_bytes()),
            &Validation::default(),
        )
        .is_ok()
    }
}

pub async fn require_auth(
    State(auth): State<Auth>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let token = request
        .headers()
        .get("x-paper-codex-token")
        .and_then(|v| v.to_str().ok())
        .or_else(|| {
            request
                .headers()
                .get(header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "))
        })
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if !auth.verify(token) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(next.run(request).await)
}

use crate::{dto::response::SessionData, ServiceState};
use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
    RequestPartsExt,
};
use axum_extra::{
    headers::{authorization::Bearer, Authorization},
    TypedHeader,
};
use jsonwebtoken::{DecodingKey, TokenData, Validation};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::error;
use uuid::Uuid;

pub static DECODE_HEADER: Lazy<Validation> = Lazy::new(|| Validation::default());

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserClaims {
    pub iat: i64,
    pub exp: i64,
    pub uid: i64,
    pub sid: Uuid,
    pub session_data: Option<SessionData>,
    pub token: Option<String>,
}

impl UserClaims {
    pub fn decode(token: &str, key: &str) -> Result<TokenData<Self>, jsonwebtoken::errors::Error> {
        jsonwebtoken::decode::<UserClaims>(
            token,
            &DecodingKey::from_secret(key.as_ref()),
            &DECODE_HEADER,
        )
    }
    async fn check_session(&mut self, auth_uri: &str, token: &str) -> Result<bool, String> {
        let client = reqwest::Client::new();
        self.token = Some(token.to_string());
        match client
            .get(&format!("{}/session", auth_uri))
            .bearer_auth(token)
            .send()
            .await
        {
            Ok(response) => {
                if response.status().is_success() {
                    match response.json::<SessionData>().await {
                        Ok(session_data) => {
                            self.session_data = Some(session_data);
                            return Ok(true);
                        }
                        Err(e) => Err(format!("Failed to parse session data: {}", e)),
                    }
                } else {
                    return Ok(false);
                }
            }
            Err(e) => {
                return Err(format!("Check session failed: {}", e));
            }
        }
    }
}

#[async_trait::async_trait]
impl FromRequestParts<Arc<ServiceState>> for UserClaims {
    type Rejection = (StatusCode, String);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<ServiceState>,
    ) -> Result<Self, Self::Rejection> {
        let TypedHeader(Authorization(bearer)) = parts
            .extract::<TypedHeader<Authorization<Bearer>>>()
            .await
            .map_err(|_| {
                error!("Failed to extract authorization header for jwt token");
                (
                    StatusCode::UNAUTHORIZED,
                    "Failed to extract authorization header for jwt token".to_string(),
                )
            })?;

        let mut user_claims =
            UserClaims::decode(bearer.token(), &state.config.jwt.access_token_secret)
                .map_err(|_| {
                    error!("Failed to decode jwt token");
                    (
                        StatusCode::UNAUTHORIZED,
                        "Failed to decode jwt token".to_string(),
                    )
                })?
                .claims;

        if user_claims
            .check_session(state.config.server.auth_service.as_str(), bearer.token())
            .await
            .map_err(|e| {
                let error_message = format!("Failed to check session: {}", e);
                error!("{}", error_message);
                (StatusCode::INTERNAL_SERVER_ERROR, error_message)
            })?
            == false
        {
            error!("Invalid authorization header");
            return Err((
                StatusCode::UNAUTHORIZED,
                "Invalid authorization header".to_string(),
            ));
        };

        Ok(user_claims)
    }
}

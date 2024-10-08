use crate::ServiceState;
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
use uuid::Uuid;

pub static DECODE_HEADER: Lazy<Validation> = Lazy::new(|| Validation::default());

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct UserClaims {
    pub iat: i64,
    pub exp: i64,
    pub uid: i64,
    pub sid: Uuid,
}

impl UserClaims {
    pub fn decode(token: &str, key: &str) -> Result<TokenData<Self>, jsonwebtoken::errors::Error> {
        jsonwebtoken::decode::<UserClaims>(
            token,
            &DecodingKey::from_secret(key.as_ref()),
            &DECODE_HEADER,
        )
    }
    pub fn check_session(&self) -> bool {
        return true;
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
                (
                    StatusCode::UNAUTHORIZED,
                    "Invalid authorization header".to_string(),
                )
            })?;

        let user_claims = UserClaims::decode(bearer.token(), &state.config.jwt.access_token_secret)
            .map_err(|_| (StatusCode::UNAUTHORIZED, "Invalid token".to_string()))?
            .claims;

        if user_claims.check_session() == false {
            return Err((
                StatusCode::UNAUTHORIZED,
                "Invalid authorization header".to_string(),
            ));
        };

        Ok(user_claims)
    }
}

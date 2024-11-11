use crate::{
    utils::{error::format_error, jwt::UserClaims, openai},
    ServiceState,
};
use axum::{
    extract::{Multipart, State},
    http::StatusCode,
    response::IntoResponse,
};
use std::sync::Arc;
use tracing::{error, info};

type AppResult<T> = Result<T, (StatusCode, String)>;

pub async fn speech_to_text(
    State(state): State<Arc<ServiceState>>,
    user: UserClaims,
    mut multipart: Multipart,
) -> AppResult<impl IntoResponse> {
    info!("Speech to text API from the user: {}", user.uid);
    while let Some(field) = multipart.next_field().await.map_err(|e| {
        format_error(
            "Failed to read multipart fields",
            e,
            StatusCode::INTERNAL_SERVER_ERROR,
        )
    })? {
        let name = field.name();
        if name.is_none() {
            continue;
        }
        let name = name.unwrap();
        if name != "voice" {
            return Err(format_error(
                "Unknown Multipart field name",
                name,
                StatusCode::INTERNAL_SERVER_ERROR,
            ));
        }
        let filename = field.file_name().map(|s| s.to_string());
        let filename = match filename {
            Some(name) => name,
            _ => "speech_to_text".into(),
        };
        info!("{}", filename);
        let data = field.bytes().await;
        if data.is_err() {
            continue;
        }
        let data = data.unwrap();
        let res = openai::speech_to_text(
            &state.config.openai.openai_key,
            data.to_vec(),
            filename.clone(),
        )
        .await
        .map_err(|e| {
            error!("{}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e)
        })?;
        return Ok(res);
    }
    Err((StatusCode::BAD_REQUEST, "No voice field specified.".into()))
}

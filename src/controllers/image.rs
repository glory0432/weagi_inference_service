use crate::{
    dto::request::ImageGenerationRequest,
    utils::{error, jwt::UserClaims, openai::text_to_image},
    ServiceState,
};
use axum::{
    body::Body,
    extract::{Json, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use reqwest::Client;
use std::sync::Arc;
use tracing::{error, info};
type AppResult<T> = Result<T, (StatusCode, String)>;

pub async fn image_generate(
    State(state): State<Arc<ServiceState>>,
    user: UserClaims,
    Json(req): Json<ImageGenerationRequest>,
) -> AppResult<impl IntoResponse> {
    info!(
        "User '{}' is generating the image of the text '{}'.",
        user.uid, req.text
    );

    let url = text_to_image(&state.config.openai.openai_key, &req.text)
        .await
        .map_err(|e| {
            error!("{}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e)
        })?;

    let client = Client::new();
    let res = client.get(url).send().await.map_err(|e| {
        error::format_error(
            "Failed to get image data from the url",
            e,
            StatusCode::INTERNAL_SERVER_ERROR,
        )
    })?;
    if res.status().is_success() {
        let bytes = res.bytes().await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get bytes of the image: {}", e),
            )
        })?;
        Ok(Response::builder()
            .header(header::CONTENT_TYPE, "image/png")
            .body(Body::from(bytes))
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to build response: {}", e),
                )
            })?)
    } else {
        return Err((
            StatusCode::BAD_GATEWAY,
            format!("Failed to access to the generated image"),
        ));
    }
}

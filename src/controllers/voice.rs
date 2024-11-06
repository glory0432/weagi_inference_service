use crate::utils::jwt::UserClaims;
use crate::ServiceState;
use axum::{
    extract::{Multipart, State},
    http::StatusCode,
    response::IntoResponse,
};
use rs_openai::{
    audio::{AudioModel, CreateTranscriptionRequestBuilder, ResponseFormat},
    shared::types::FileMeta,
    OpenAI,
};
use std::sync::Arc;
use tracing::{error, info};

type AppResult<T> = Result<T, (StatusCode, String)>;

fn format_error(message: &str, error: impl std::fmt::Display) -> (StatusCode, String) {
    let error_message = format!("{}: {}", message, error);
    error!("Error occurred: {}", error_message);
    (StatusCode::INTERNAL_SERVER_ERROR, error_message)
}

pub async fn speech_to_text(
    State(state): State<Arc<ServiceState>>,
    user: UserClaims,
    mut multipart: Multipart,
) -> AppResult<impl IntoResponse> {
    info!("Speech to text API from the user: {}", user.uid);
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| format_error("Failed to read multipart fields", e))?
    {
        let name = field.name();
        if name.is_none() {
            continue;
        }
        let name = name.unwrap();
        if name != "voice" {
            return Err(format_error("Unknown Multipart field name", name));
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
        let client = OpenAI::new(&OpenAI {
            api_key: state.config.openai.openai_key.clone(),
            org_id: None,
        });
        info!("{}", data.to_vec().len());
        let req = CreateTranscriptionRequestBuilder::default()
            .file(FileMeta {
                buffer: data.to_vec(),
                filename: filename,
            })
            .model(AudioModel::Whisper1)
            .response_format(ResponseFormat::Text)
            .build()
            .map_err(|e| format_error("OpenAI transcription request build failed", e))?;

        let res = client
            .audio()
            .create_transcription_with_text_response(&req)
            .await
            .map_err(|e| format_error("OpenAI transcription sending request failed", e))?;
        return Ok(res);
    }
    Err((StatusCode::BAD_REQUEST, "No voice field specified.".into()))
}

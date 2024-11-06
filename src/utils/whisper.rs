use axum::http::StatusCode;
use rs_openai::{
    audio::{AudioModel, CreateTranscriptionRequestBuilder, ResponseFormat},
    shared::types::FileMeta,
    OpenAI,
};
use tracing::error;
fn format_error(message: &str, error: impl std::fmt::Display) -> (StatusCode, String) {
    let error_message = format!("{}: {}", message, error);
    error!("Error occurred: {}", error_message);
    (StatusCode::INTERNAL_SERVER_ERROR, error_message)
}
pub async fn speech_to_text(
    api_key: &str,
    audio_data: Vec<u8>,
    filename: String,
) -> Result<String, (StatusCode, String)> {
    let client = OpenAI::new(&OpenAI {
        api_key: api_key.into(),
        org_id: None,
    });
    let req = CreateTranscriptionRequestBuilder::default()
        .file(FileMeta {
            buffer: audio_data.to_vec(),
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
    Ok(res)
}

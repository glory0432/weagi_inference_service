use crate::config::constant;
use crate::dto::response::SessionData;
use crate::entity::conversation::Message;
use crate::entity::conversation::MessageType;
use crate::repositories::conversation;
use crate::utils::file::save_file;
use crate::utils::session::send_session_data;
use crate::utils::whisper::speech_to_text;
use crate::ServiceState;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::Response;
use base64::prelude::BASE64_STANDARD;
use base64::Engine;
use deepgram::speak::options::Container;
use deepgram::speak::options::Encoding;
use deepgram::speak::options::Model;
use deepgram::speak::options::Options;
use deepgram::Deepgram;
use http_body_util::StreamBody;
use hyper::body::{Bytes, Frame};
use image::ImageFormat;
use image::ImageReader;
use regex::Regex;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use rs_openai::{chat::Role, OpenAI};
use sea_orm::TransactionTrait;

use tokio_stream::StreamExt;
use tracing::{error, info};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct ChatChunkDelta {
    content: Option<String>,
}
#[derive(Debug, Deserialize)]
pub struct ChatChunkChoice {
    delta: ChatChunkDelta,
    index: usize,
    finish_reason: Option<String>,
}
#[derive(Debug, Deserialize)]
pub struct ChatCompletionChunk {
    id: String,
    object: String,
    created: usize,
    model: String,
    choices: Vec<ChatChunkChoice>,
}

pub async fn handle_user_message(
    state: Arc<ServiceState>,
    user_id: i64,
    session_data: Option<SessionData>,
    conversation_id: Uuid,
    message_type: String,
    message_data: Vec<u8>,
    message_model: String,
    images: Vec<Bytes>,
    message_id: i64,
    voice_filename: Option<String>,
    image_filnames: Vec<Option<String>>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    if session_data.is_none() {
        error!(
            "Session data is missing for user '{}'. User might not be authenticated properly.",
            user_id
        );
        return Err((
            StatusCode::UNAUTHORIZED,
            "Session data is required but missing. Please log in to continue.".to_string(),
        ));
    }

    info!(
        "User '{}' is attempting to send a message in the conversation '{}'. Model used: '{}'",
        user_id, conversation_id, message_model
    );

    let credits_remaining: i64;
    let message_type = format!("\"{}\"", message_type);

    let message_type: Result<MessageType, serde_json::Error> =
        serde_json::from_str::<MessageType>(&message_type);
    if message_type.is_err() {
        error!(
            "Failed to parse message type: {}",
            message_type.unwrap_err()
        );
        return Err((
            StatusCode::BAD_REQUEST,
            "Failed to parse message type".to_string(),
        ));
    }
    let message_type = message_type.unwrap();

    if let Some(&cost) = constant::MODEL_TO_PRICE.get(message_model.as_str()) {
        credits_remaining = session_data.clone().unwrap().credits_remaining;
        if cost > credits_remaining {
            error!(
                "Credit check failed for user '{}'. Required: {:.2}, Available: {:.2}.",
                user_id, cost, credits_remaining
            );
            return Err((
                StatusCode::FORBIDDEN,
                "Insufficient credits to proceed with the action.".to_string(),
            ));
        }
        info!(
            "User '{}' has sufficient credits remaining. Deducting {:.2} credits. Remaining credits: {:.2}",
            user_id, cost, credits_remaining
        );
    } else {
        error!(
            "Invalid model name '{}' provided by user '{}'. Model not recognized.",
            message_model, user_id
        );
        return Err((
            StatusCode::BAD_REQUEST,
            "The provided model name is invalid or not supported.".to_string(),
        ));
    }
    let user_message = match message_type {
        MessageType::Text => String::from_utf8(message_data.clone()).map_err(|e| {
            let error_message = format!("Failed to convert message data into string: {}", e);
            error!("{}", error_message);
            (StatusCode::BAD_REQUEST, error_message)
        })?,
        _ => {
            speech_to_text(
                &state.config.openai.openai_key,
                message_data.clone(),
                voice_filename.clone().unwrap(),
            )
            .await?
        }
    };

    let transaction = state.db.begin().await.map_err(|e| {
        let error_message = format!(
            "Could not start a database transaction due to an error: '{}'",
            e
        );
        error!("{}", error_message);
        (StatusCode::INTERNAL_SERVER_ERROR, error_message)
    })?;

    let conversation_model =
        conversation::find_by_user_id_and_conversation_id(&transaction, user_id, conversation_id)
            .await
            .map_err(|e| {
                let error_message = format!(
                    "Failed to query the database for conversation '{}': {}",
                    conversation_id, e
                );
                error!("{}", error_message);
                (StatusCode::INTERNAL_SERVER_ERROR, error_message)
            })?;

    if conversation_model.is_none() {
        error!(
            "No conversation found with ID '{}' for user '{}'. Cannot send message.",
            conversation_id, user_id
        );
        return Err((
            StatusCode::NOT_FOUND,
            "The specified conversation does not exist.".to_string(),
        ));
    }

    if message_id >= (conversation_model.clone().unwrap().conversation.len() / 2) as i64 {
        error!(
            "Invalid message ID '{}' provided for conversation '{}' by user '{}'.",
            message_id, conversation_id, user_id
        );
        return Err((
            StatusCode::BAD_REQUEST,
            "The message ID is invalid or out of range.".to_string(),
        ));
    }

    info!(
        "Setting up OpenAI client for user '{}' with conversation '{}'.",
        user_id, conversation_id
    );

    let client = OpenAI::new(&OpenAI {
        api_key: state.config.openai.openai_key.clone(),
        org_id: None,
    });

    let mut conversation_list: Vec<(String, Role, Vec<String>)> = conversation_model
        .clone()
        .unwrap()
        .conversation
        .clone()
        .into_iter()
        .map(|e| {
            let message: Message = serde_json::from_value(e).unwrap();
            match message.msgtype {
                MessageType::Text => (message.content, message.role, message.images),
                _ => (
                    message.transcription.unwrap_or_default(),
                    message.role,
                    message.images,
                ),
            }
        })
        .collect();
    let mut last_user_message_images = vec![];
    for (index, image) in images.iter().enumerate() {
        let saved_filename;
        let mut file_extension: Option<&str> = None;
        if let Some(ref filename) = image_filnames[index] {
            file_extension = Path::new(filename.as_str())
                .extension()
                .and_then(std::ffi::OsStr::to_str);
        }
        if let Some(extension) = file_extension {
            saved_filename = format!(
                "images/{}-{}-{}.{}",
                conversation_id,
                conversation_list.len(),
                index,
                extension,
            );
        } else {
            saved_filename = format!(
                "images/{}-{}-{}",
                conversation_id,
                conversation_list.len(),
                index
            );
        }
        save_file(saved_filename.as_str(), image.to_vec().clone()).map_err(|e| {
            let error_message = format!("Error in saving user's image file: {}", e);
            error!("{}", error_message);
            (StatusCode::INTERNAL_SERVER_ERROR, error_message)
        })?;
        last_user_message_images.push(saved_filename);
    }
    conversation_list.push((
        user_message.clone(),
        Role::User,
        last_user_message_images.clone(),
    ));
    let request_url = "https://api.openai.com/v1/chat/completions";

    let request_body = json!({
        "model": message_model,
        "stream": true,
        "messages": conversation_list
        .iter()
        .map(|&(ref message, ref role, ref images)| {

            let content = if images.is_empty() {
                json!([{
                    "type": "text",
                    "text": message.clone()
                }])
            } else {
                let mut content_items = vec![json!({
                    "type": "text",
                    "text": message.clone()
                })];

                for image in images {
                    let img = ImageReader::open(format!("./public/{}", image));
                    if img.is_err() {
                        continue;
                    }
                    let img = img.unwrap().decode();
                    if img.is_err() {
                        continue;
                    }
                    let img = img.unwrap().to_rgb8();
                    let mut jpeg_buffer = Vec::new();
                    {
                        let mut cursor = Cursor::new(&mut jpeg_buffer);
                        if img.write_to(&mut cursor, ImageFormat::Jpeg).is_err() {
                            continue;
                        }
                    }
                    let base64_string = BASE64_STANDARD.encode(&jpeg_buffer);
                    content_items.push(json!({
                        "type": "image_url",
                        "image_url": {
                            "url" : format!("data:image/jpeg;base64,{}", base64_string)
                        }
                    }));
                }
                json!(content_items)
            };

            json!({
                "role": role,
                "content": content
            })
        }).collect::<Vec<_>>(),
    });
    let client = Client::new();
    let response = client
        .post(request_url)
        .bearer_auth(state.config.openai.openai_key.clone())
        .json(&request_body)
        .send()
        .await
        .map_err(|e| {
            let error_message = format!("OpenAI response failed: {}", e);
            error!("{}", error_message);
            (StatusCode::INTERNAL_SERVER_ERROR, error_message)
        })?;
    let mut openai_stream = response.bytes_stream();

    let mut total_content = "".to_string();
    let mut total_voice: Vec<u8> = vec![];
    let sentence_regex = Regex::new(r"(?m)(?:[.!?]\s+|\n|\r\n)").map_err(|e| {
        let error_message = format!("Sentence split regex creation failed: {}", e);
        error!("{}", error_message);
        (StatusCode::INTERNAL_SERVER_ERROR, error_message)
    })?;
    let (tx, rx) = mpsc::channel::<Result<Frame<Bytes>, String>>(1000000);
    tokio::spawn(async move {
        let mut buffer = String::new();
        let mut is_started = false;
        while let Some(response) = openai_stream.next().await {
            match response {
                Ok(result) => {
                    let s = match std::str::from_utf8(&result) {
                        Ok(v) => v,
                        Err(e) => {
                            error!("Invalid UTF-8 sequence: {}", e);
                            break;
                        }
                    };
                    let mut cached_str = "".to_string();
                    for p in s.split("\n") {
                        match p.strip_prefix("data: ") {
                            Some(p) => {
                                if p == "[DONE]" {
                                    break;
                                }

                                let d = serde_json::from_str::<ChatCompletionChunk>(&format!(
                                    "{}{}",
                                    cached_str, p
                                ));
                                if d.is_err() {
                                    cached_str.push_str(p);
                                    continue;
                                }
                                let d = d.unwrap();

                                let c = d.choices.get(0);
                                if c.is_none() {
                                    error!("No choice returned");
                                    continue;
                                }
                                let c = c.unwrap();
                                cached_str = String::from("");

                                if let Some(content) = &c.delta.content {
                                    let content_clone = content.clone();
                                    total_content.push_str(content.clone().as_str());
                                    match message_type {
                                        MessageType::Voice => {
                                            buffer.push_str(&content_clone);
                                            while let Some(pos) = sentence_regex.find(&buffer) {
                                                let sentence =
                                                    buffer.drain(..pos.end()).collect::<String>();
                                                let dg_client = Deepgram::new(
                                                    &state.config.deepgram.deepgram_key.clone(),
                                                );
                                                if dg_client.is_err() {
                                                    continue;
                                                }
                                                let dg_client = dg_client.unwrap();
                                                let sample_rate = 16000;
                                                let channels = 1;

                                                let options = Options::builder()
                                                    .model(Model::AuraAsteriaEn)
                                                    .encoding(Encoding::Linear16)
                                                    .sample_rate(sample_rate)
                                                    .container(if is_started == false {
                                                        Container::Wav
                                                    } else {
                                                        Container::CustomContainer(
                                                            "none".to_owned(),
                                                        )
                                                    })
                                                    .build();
                                                is_started = true;
                                                let audio_stream = dg_client
                                                    .text_to_speech()
                                                    .speak_to_stream(&sentence, &options)
                                                    .await;
                                                if audio_stream.is_err() {
                                                    continue;
                                                }
                                                let mut audio_stream = audio_stream.unwrap();
                                                while let Some(data) = audio_stream.next().await {
                                                    total_voice.append(&mut data.to_vec());
                                                    if tx.send(Ok(Frame::data(data))).await.is_err()
                                                    {
                                                        return Err(());
                                                    }
                                                }
                                            }
                                        }
                                        MessageType::Text => {
                                            if tx
                                                .send(Ok(Frame::data(Bytes::from(content_clone))))
                                                .await
                                                .is_err()
                                            {
                                                return Err(());
                                            }
                                        }
                                    }
                                }
                            }
                            None => {}
                        }
                    }
                }
                Err(e) => {
                    let error_message = format!("Stream error occurred while processing OpenAI response for conversation '{}': {}", conversation_id, e);
                    error!(error_message);
                    let _ = tx.send(Err(error_message)).await;
                    return Err(());
                }
            }
        }
        let mut saved_filename = String::from("");
        let mut file_extension: Option<&str> = None;
        if message_type != MessageType::Text {
            if let Some(ref filename) = voice_filename {
                file_extension = Path::new(filename.as_str())
                    .extension()
                    .and_then(std::ffi::OsStr::to_str);
            }
            if let Some(extension) = file_extension {
                saved_filename = format!(
                    "voice/{}-{}.{}",
                    conversation_id,
                    conversation_list.len() - 1,
                    extension
                );
            } else {
                saved_filename =
                    format!("voice/{}-{}", conversation_id, conversation_list.len() - 1);
            }
            save_file(saved_filename.as_str(), message_data.clone()).unwrap();
        }

        if conversation::add_message(
            &transaction,
            user_id,
            conversation_id,
            message_type.clone(),
            if message_type == MessageType::Text {
                user_message.clone()
            } else {
                saved_filename
            },
            if message_type == MessageType::Text {
                None
            } else {
                Some(user_message)
            },
            last_user_message_images,
            total_content,
            if message_id == -1 {
                (conversation_list.len() - 1) as i64
            } else {
                message_id * 2
            },
        )
        .await
        .is_err()
        {
            let error_message = format!("Failed to save message in database");
            error!("{}", error_message);
            let _ = tx.send(Err(error_message)).await;
            return Err(());
        };

        if send_session_data(
            json!({
                "credits_remaining" : credits_remaining,
                "user_id" : user_id
            }),
            state.config.server.auth_service.as_str(),
            state.config.server.auth_secret_key.clone(),
        )
        .await
        .is_err()
        {
            let error_message =
                format!("Error sending updated session data for user '{}'", user_id);
            error!("{}", error_message);
            let _ = tx.send(Err(error_message)).await;
            return Err(());
        };

        if transaction.commit().await.is_err() {
            let error_message = format!("Committing the database transaction failed");
            error!("{error_message}");
            let _ = tx.send(Err(error_message)).await;
            return Err(());
        };
        Ok(())
    });
    let stream = ReceiverStream::new(rx);
    let body_openai = StreamBody::new(stream);

    return Ok(Response::builder()
        .header("Cache-Control", "no-cache")
        .header("Connection", "keep-alive")
        .body(body_openai)
        .unwrap());
}

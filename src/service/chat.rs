use crate::{
    config::constant,
    dto::response::SessionData,
    entity::conversation::{Message, MessageType},
    repositories::conversation,
    utils::{
        deepgram::text_to_speech,
        error::format_error,
        file::save_file,
        openai::{chunk_to_content_list, send_chat_completion, speech_to_text},
        session::send_session_data,
    },
    ServiceState,
};
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

use http_body_util::StreamBody;
use hyper::body::{Bytes, Frame};
use regex::Regex;
use rs_openai::{chat::Role, OpenAI};
use sea_orm::TransactionTrait;
use serde::Deserialize;
use serde_json::json;
use std::{path::Path, sync::Arc};
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream, StreamExt};
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
        return Err(format_error(
            "Session data is required but missing for the user",
            user_id,
            StatusCode::BAD_REQUEST,
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
        return Err(format_error(
            "Failed to parse message type",
            message_type.unwrap_err(),
            StatusCode::BAD_REQUEST,
        ));
    }
    let message_type = message_type.unwrap();

    if let Some(&cost) = constant::MODEL_TO_PRICE.get(message_model.as_str()) {
        credits_remaining = session_data.clone().unwrap().credits_remaining;
        if cost > credits_remaining {
            return Err(format_error(
                "Insufficient credits to proceed with the action. Required",
                cost,
                StatusCode::BAD_REQUEST,
            ));
        }
    } else {
        return Err(format_error(
            "Invalid model name",
            message_model,
            StatusCode::BAD_REQUEST,
        ));
    }
    let user_message = match message_type {
        MessageType::Text => String::from_utf8(message_data.clone()).map_err(|e| {
            format_error(
                "Failed to convert message data into string",
                e,
                StatusCode::BAD_REQUEST,
            )
        })?,
        _ => speech_to_text(
            &state.config.openai.openai_key,
            message_data.clone(),
            voice_filename.clone().unwrap(),
        )
        .await
        .map_err(|e| {
            error!("{}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e)
        })?,
    };

    let transaction = state.db.begin().await.map_err(|e| {
        format_error(
            "Could not start a database transaction due to an error",
            e,
            StatusCode::INTERNAL_SERVER_ERROR,
        )
    })?;

    let conversation_model =
        conversation::find_by_user_id_and_conversation_id(&transaction, user_id, conversation_id)
            .await
            .map_err(|e| {
                format_error(
                    "Failed to find the specific conversation of the user",
                    e,
                    StatusCode::INTERNAL_SERVER_ERROR,
                )
            })?;

    if conversation_model.is_none() {
        return Err(format_error(
            "No conversation found for the user",
            user_id,
            StatusCode::NOT_FOUND,
        ));
    }

    if message_id >= (conversation_model.clone().unwrap().conversation.len() / 2) as i64 {
        return Err(format_error(
            "Invalid Message Id",
            message_id,
            StatusCode::BAD_REQUEST,
        ));
    }

    let mut message_list: Vec<(String, Role, Vec<String>)> = conversation_model
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
    let mut last_message = vec![];

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
                message_list.len(),
                index,
                extension,
            );
        } else {
            saved_filename = format!(
                "images/{}-{}-{}",
                conversation_id,
                message_list.len(),
                index
            );
        }
        save_file(saved_filename.as_str(), image.to_vec().clone()).map_err(|e| {
            format_error(
                "Error in saving user's image file",
                e,
                StatusCode::INTERNAL_SERVER_ERROR,
            )
        })?;
        last_message.push(saved_filename);
    }
    message_list.push((user_message.clone(), Role::User, last_message.clone()));

    let openai_response = send_chat_completion(
        state.config.openai.openai_key.clone(),
        message_model,
        message_list.clone(),
    )
    .await
    .map_err(|e| {
        error!("{}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, format!("{}", e))
    })?;

    let mut openai_stream = openai_response.bytes_stream();

    let mut total_content = "".to_string();
    let mut total_voice: Vec<u8> = vec![];
    let sentence_regex = Regex::new(r"(?m)(?:[.!?]\s+|\n|\r\n)").map_err(|e| {
        format_error(
            "Sentence split regex creation failed",
            e,
            StatusCode::INTERNAL_SERVER_ERROR,
        )
    })?;

    let (tx, rx) = mpsc::channel::<Result<Frame<Bytes>, String>>(1000000);
    let message_type_clone = message_type.clone();

    tokio::spawn(async move {
        let mut buffer = String::new();
        let mut is_started = false;
        while let Some(response) = openai_stream.next().await {
            match response {
                Ok(result) => {
                    let content = match chunk_to_content_list(result) {
                        Ok(content_list) => content_list,
                        _ => {
                            continue;
                        }
                    };
                    for content_str in content {
                        total_content.push_str(content_str.clone().as_str());
                        match message_type {
                            MessageType::Voice => {
                                let stream_result = text_to_speech(
                                    &state.config.deepgram.deepgram_key,
                                    &content_str,
                                    is_started,
                                )
                                .await;
                                is_started = true;
                                if stream_result.is_err() {
                                    continue;
                                }
                                let mut audio_stream = stream_result.unwrap();
                                while let Some(data) = audio_stream.next().await {
                                    total_voice.append(&mut data.to_vec());
                                    if tx.send(Ok(Frame::data(data))).await.is_err() {
                                        error!("Failed to send voice stream data to buffer");
                                        return Err(());
                                    }
                                }
                            }
                            MessageType::Text => {
                                if tx
                                    .send(Ok(Frame::data(Bytes::from(content_str.clone()))))
                                    .await
                                    .is_err()
                                {
                                    error!("Failed send openaai text response to buffer");
                                    return Err(());
                                }
                            }
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
                    message_list.len() - 1,
                    extension
                );
            } else {
                saved_filename = format!("voice/{}-{}", conversation_id, message_list.len() - 1);
            }

            save_file(saved_filename.as_str(), message_data.clone()).unwrap();
            // let mut reader = hound::WavReader::new(Cursor::new(total_voice)).map_err(|e| {
            //     let error_message = format!("Failed to create wav reader: {}", e);
            //     error!("{}", error_message);
            //     ()
            // })?;
            // let samples: Vec<i16> = reader.samples::<i16>().filter_map(Result::ok)  .collect();
            // save_audio_file(&format!("voice/{}-{}.mp3", conversation_id, conversation_list.len()), samples);
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
            last_message,
            total_content,
            if message_id == -1 {
                (message_list.len() - 1) as i64
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
        .header(
            "Content-Type",
            if message_type_clone == MessageType::Text {
                "text/plain"
            } else {
                "audio/wav"
            },
        )
        .body(body_openai)
        .unwrap());
}

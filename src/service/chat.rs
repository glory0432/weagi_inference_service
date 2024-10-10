use std::sync::Arc;

use crate::config::constant;
use crate::dto::response::SessionData;
use crate::repositories::conversation;
use crate::utils::session::send_session_data;
use crate::ServiceState;
use axum::http::StatusCode;
use axum::response::sse::{Event, Sse};
use axum::response::IntoResponse;
use rs_openai::{
    chat::{ChatCompletionMessageRequestBuilder, CreateChatRequestBuilder, Role},
    OpenAI,
};
use sea_orm::TransactionTrait;
use serde_json::json;
use tokio::sync::oneshot;
use tokio_stream::{wrappers::UnboundedReceiverStream, StreamExt as _};
use tracing::{error, info};
use uuid::Uuid;

pub async fn save_message(
    state: Arc<ServiceState>,
    user_id: i64,
    session_data: Option<SessionData>,
    token: Option<String>,
    conversation_id: Uuid,
    user_message: String,
    model_name: String,
    message_id: i64,
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
    if token.is_none() {
        error!(
            "JWT token is missing for user '{}'. Token is required for authentication.",
            user_id
        );
        return Err((
            StatusCode::UNAUTHORIZED,
            "Valid JWT token is required but missing.".to_string(),
        ));
    }

    info!(
        "User '{}' is attempting to send a message in the conversation '{}'. Model used: '{}'",
        user_id, conversation_id, model_name
    );

    let credits_remaining: f64;
    if let Some(&cost) = constant::MODEL_TO_PRICE.get(model_name.as_str()) {
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
            model_name, user_id
        );
        return Err((
            StatusCode::BAD_REQUEST,
            "The provided model name is invalid or not supported.".to_string(),
        ));
    }

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

    let mut conversation_list = conversation_model.clone().unwrap().conversation.clone();
    conversation_list.push(user_message.clone());

    let chat_request = CreateChatRequestBuilder::default()
        .model(model_name)
        .messages(
            conversation_list
                .iter()
                .enumerate()
                .map(|(index, message)| {
                    ChatCompletionMessageRequestBuilder::default()
                        .role(if index % 2 == 0 {
                            Role::User
                        } else {
                            Role::Assistant
                        })
                        .content(message.clone())
                        .build()
                        .unwrap()
                })
                .collect::<Vec<_>>(),
        )
        .stream(true)
        .build()
        .unwrap();

    let mut stream = client
        .chat()
        .create_with_stream(&chat_request)
        .await
        .map_err(|e| {
            let error_message = format!("OpenAI service error: {}", e);
            error!("{}", error_message);
            (StatusCode::INTERNAL_SERVER_ERROR, error_message)
        })?;

    let (sender, receiver) =
        tokio::sync::mpsc::unbounded_channel::<Result<Event, std::convert::Infallible>>();

    let (result_tx, result_rx) = oneshot::channel();

    tokio::spawn(async move {
        let mut content_buffer = String::new();
        let mut commit_transaction = true;

        while let Some(response) = stream.next().await {
            match response {
                Ok(result) => {
                    for choice in result.choices {
                        if let Some(content) = choice.delta.content {
                            content_buffer.push_str(&content); // Collect content
                            if sender
                                .send(Ok(Event::default().data(content.clone())))
                                .is_err()
                            {
                                error!(
                                    "Error: Failed to send event stream for conversation '{}'.",
                                    conversation_id
                                );
                                commit_transaction = false;
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    error!(
                        "Stream error occurred while processing OpenAI response for conversation '{}': {}",  
                        conversation_id, e
                    );
                    commit_transaction = false;
                    break;
                }
            }
        }

        if commit_transaction {
            info!(
                "Committing transaction for conversation '{}' and user '{}'.",
                conversation_id, user_id
            );
            if let Err(e) = conversation::add_message(
                &transaction,
                user_id,
                conversation_id,
                user_message,
                content_buffer,
                if message_id == -1 {
                    (conversation_list.len() - 1) as i64
                } else {
                    message_id * 2
                },
            )
            .await
            {
                error!("Failed to save message in database: {}", e);
                commit_transaction = false;
            }
        }

        if commit_transaction {
            if let Err(e) = transaction.commit().await {
                error!(
                    "Failed to commit transaction for conversation '{}': {}",
                    conversation_id, e
                );
                commit_transaction = false;
            }
        } else {
            if let Err(e) = transaction.rollback().await {
                error!(
                    "Failed to rollback transaction for conversation '{}': {}",
                    conversation_id, e
                );
            }
        }

        let _ = result_tx.send(commit_transaction);
    });

    if result_rx.await.map_err(|_| {
        error!(
            "Transaction handling failed for conversation '{}' and user '{}'.",
            conversation_id, user_id
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Transaction failed unexpectedly.".into(),
        )
    })? {
        send_session_data(
            json!({
                "credits_remaining" : credits_remaining
            }),
            state.config.server.auth_service.as_str(),
            state.config.server.auth_secret_key.clone(),
            token.unwrap(),
        )
        .await
        .map_err(|e| {
            error!(
                "Error sending updated session data for user '{}': {}",
                user_id, e
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to update session data.".to_string(),
            )
        })?;
        let body = UnboundedReceiverStream::new(receiver);
        let sse = Sse::new(body).into_response();
        Ok(sse)
    } else {
        error!(
            "Transaction handling failed at the end of the process for conversation '{}' and user '{}'.",  
            conversation_id, user_id
        );

        Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "Transaction processing completed with errors.".into(),
        ))
    }
}

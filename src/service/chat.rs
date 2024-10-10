use std::sync::Arc;

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
use tracing::error;
use uuid::Uuid;

pub async fn save_message(
    state: Arc<ServiceState>,
    user_id: i64,
    conversation_id: Uuid,
    user_message: String,
    model_name: String,
    message_id: i64,
    credits_remaining: f64,
    token: String,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let transaction = state.db.begin().await.map_err(|e| {
        let error_message = format!("Failed to start a database transaction: {}", e);
        error!("{}", error_message);
        (StatusCode::INTERNAL_SERVER_ERROR, error_message)
    })?;
    let conversation_model =
        conversation::find_by_user_id_and_conversation_id(&transaction, user_id, conversation_id)
            .await
            .map_err(|_e| {
                let error_message =
                    "Failed to fetch the conversation data of specific conversation id and userid"
                        .to_string();
                error!("{}", error_message);
                (StatusCode::INTERNAL_SERVER_ERROR, error_message)
            })?;
    if conversation_model.is_none() {
        let error_message =
            "Failed to fetch the conversation data of specific conversation id and userid"
                .to_string();
        error!("{}", error_message);
        return Err((StatusCode::INTERNAL_SERVER_ERROR, error_message));
    }
    if message_id >= (conversation_model.clone().unwrap().conversation.len() / 2) as i64 {
        let error_message = "Message id is invalid.".to_string();
        error!("{}", error_message);
        return Err((StatusCode::INTERNAL_SERVER_ERROR, error_message));
    }
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
                                commit_transaction = false;
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("Error while processing OpenAI response: {}", e);
                    commit_transaction = false;
                    break;
                }
            }
        }

        if commit_transaction {
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
                error!("Failed to save message: {}", e);
                commit_transaction = false;
            }
        }

        if commit_transaction {
            if let Err(e) = transaction.commit().await {
                error!("Failed to commit transaction: {}", e);
                commit_transaction = false;
            }
        } else {
            if let Err(e) = transaction.rollback().await {
                error!("Failed to rollback transaction: {}", e);
            }
        }

        let _ = result_tx.send(commit_transaction);
    });

    if result_rx.await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Transaction handling failed".into(),
        )
    })? {
        send_session_data(
            json!({
                "credits_remaining" : credits_remaining
            }),
            state.config.server.auth_service.as_str(),
            state.config.server.auth_secret_key.clone(),
            token,
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let body = UnboundedReceiverStream::new(receiver);
        let sse = Sse::new(body).into_response();
        Ok(sse)
    } else {
        Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "Transaction handling failed".into(),
        ))
    }
}

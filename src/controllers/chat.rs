use crate::dto::request::EditTitleRequest;
use crate::dto::response::{
    CreateNewConversationResponse, DeleteConversationResponse, EditTitleResponse,
    GetConversationResponse, RetrieveAllConversationResponse,
};
use crate::entity::conversation::Message;
use crate::repositories::conversation;
use crate::service::chat::save_message;
use crate::utils::jwt::UserClaims;
use crate::ServiceState;
use axum::{
    extract::{Json, Multipart, Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use sea_orm::{DatabaseConnection, ModelTrait, TransactionTrait};
use std::sync::Arc;
use tracing::{error, info};
use uuid::Uuid;

type AppResult<T> = Result<T, (StatusCode, String)>;

async fn handle_transaction<T, F>(db: &DatabaseConnection, operation: F) -> AppResult<T>
where
    F: for<'a> FnOnce(&'a mut sea_orm::DatabaseTransaction) -> BoxFuture<'a, AppResult<T>> + Send,
    T: Send + 'static,
{
    let mut transaction = db
        .begin()
        .await
        .map_err(|e| format_error("Starting a database transaction failed", e))?;

    let result = operation(&mut transaction).await;

    match result {
        Ok(response) => {
            transaction
                .commit()
                .await
                .map_err(|e| format_error("Committing the database transaction failed", e))?;
            Ok(response)
        }
        Err(e) => {
            if let Err(rollback_err) = transaction.rollback().await {
                error!(
                    "Rolling back the database transaction failed. Possible data inconsistency: {}",
                    rollback_err
                );
            }
            Err(e)
        }
    }
}

fn format_error(message: &str, error: impl std::fmt::Display) -> (StatusCode, String) {
    let error_message = format!("{}: {}", message, error);
    error!("Error occurred: {}", error_message);
    (StatusCode::INTERNAL_SERVER_ERROR, error_message)
}

pub async fn create_new_conversation(
    State(state): State<Arc<ServiceState>>,
    user: UserClaims,
) -> AppResult<impl IntoResponse> {
    info!(
        "Initiating process to create a new conversation for user with ID '{}'.",
        user.uid
    );

    handle_transaction(&state.db, |transaction| {
        Box::pin(async move {
            let conversation_id = conversation::new_conversation(transaction, user.uid)
                .await
                .map_err(|e| {
                    format_error(
                        "Failed to create a new conversation due to a database error",
                        e,
                    )
                })?;

            info!(
                "Successfully created new conversation with ID '{}' for user '{}'.",
                conversation_id, user.uid
            );
            Ok(Json(CreateNewConversationResponse { conversation_id }).into_response())
        })
    })
    .await
}

pub async fn retrieve_all_conversations(
    State(state): State<Arc<ServiceState>>,
    user: UserClaims,
) -> AppResult<impl IntoResponse> {
    info!(
        "Retrieving all conversations for user with ID '{}'.",
        user.uid
    );
    handle_transaction(&state.db, |transaction| {
        Box::pin(async move {
            let conversation_list: Vec<(Uuid, String, DateTime<Utc>)> =
                conversation::find_by_user_id(transaction, user.uid)
                    .await
                    .map_err(|e| {
                        format_error(
                            "Failed to fetch user's conversations due to a database error",
                            e,
                        )
                    })?
                    .into_iter()
                    .map(|x| (x.id, x.title, x.updated_at))
                    .collect();

            info!(
                "Successfully retrieved {} conversations for user '{}'.",
                conversation_list.len(),
                user.uid
            );
            Ok(Json(RetrieveAllConversationResponse { conversation_list }).into_response())
        })
    })
    .await
}

pub async fn delete_conversation(
    Path(conversation_id): Path<Uuid>,
    State(state): State<Arc<ServiceState>>,
    user: UserClaims,
) -> AppResult<impl IntoResponse> {
    info!(
        "User with ID '{}' is attempting to delete conversation with ID '{}'.",
        user.uid, conversation_id
    );
    handle_transaction(&state.db, |transaction| {
        Box::pin(async move {
            let conversation_model = conversation::find_by_user_id_and_conversation_id(
                transaction,
                user.uid,
                conversation_id,
            )
            .await
            .map_err(|e| {
                format_error(
                    "Database query failed while fetching the specified conversation",
                    e,
                )
            })?;

            if conversation_model.is_none() {
                let error_message = "Conversation could not be found for deletion".to_string();
                error!("Failed to delete: {}", error_message);
                return Err((StatusCode::NOT_FOUND, error_message));
            }

            conversation_model
                .unwrap()
                .delete(transaction)
                .await
                .map_err(|e| {
                    format_error(
                        "Failed to delete the conversation due to a database error",
                        e,
                    )
                })?;

            info!(
                "Conversation with ID '{}' successfully deleted by user '{}'.",
                conversation_id, user.uid
            );
            Ok(Json(DeleteConversationResponse {
                message: "Conversation successfully deleted".to_string(),
            })
            .into_response())
        })
    })
    .await
}

pub async fn get_conversation(
    Path(conversation_id): Path<Uuid>,
    State(state): State<Arc<ServiceState>>,
    user: UserClaims,
) -> AppResult<impl IntoResponse> {
    info!(
        "User with ID '{}' is requesting details for conversation with ID '{}'.",
        user.uid, conversation_id
    );
    handle_transaction(&state.db, |transaction| {
        Box::pin(async move {
            let conversation_model = conversation::find_by_user_id_and_conversation_id(
                transaction,
                user.uid,
                conversation_id,
            )
            .await
            .map_err(|e| {
                format_error("Error fetching conversation details from the database", e)
            })?;

            if let Some(model) = conversation_model {
                info!(
                    "Successfully retrieved details for conversation with ID '{}' for user '{}'.",
                    conversation_id, user.uid
                );
                let message_result: Result<Vec<Message>, serde_json::Error> = model
                    .conversation
                    .into_iter()
                    .map(|v| serde_json::from_value::<Message>(v))
                    .collect();
                let message_result = message_result
                    .map_err(|e| format_error("Error converting to Message array", e))?;
                Ok(Json(GetConversationResponse {
                    messages: message_result,
                })
                .into_response())
            } else {
                let error_message = "Requested conversation could not be found".to_string();
                error!("Failed to retrieve: {}", error_message);
                Err((StatusCode::NOT_FOUND, error_message))
            }
        })
    })
    .await
}

pub async fn send_message(
    Path(conversation_id): Path<Uuid>,
    State(state): State<Arc<ServiceState>>,
    user: UserClaims,
    mut multipart: Multipart,
) -> AppResult<impl IntoResponse> {
    let mut message_type = String::from("");
    let mut message_data: Vec<u8> = vec![];
    let mut message_model: String = String::from("");
    let mut images = vec![];
    let mut image_filenames = vec![];
    let mut voice_filename: Option<String> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| format_error("Failed to read multipart fields", e))?
    {
        let name = field.name();
        if name.is_none() {
            continue;
        }
        let filename = field.file_name().map(|s| s.to_string());
        let name = name.unwrap().to_string();
        let data = field.bytes().await;
        if data.is_err() {
            continue;
        }
        let data = data.unwrap();

        if name == String::from("message_type") {
            message_type = String::from_utf8(data.iter().as_slice().to_vec())
                .map_err(|e| format_error("Error parsing message type as string", e))?;
        } else if name == String::from("user_message") {
            message_data = data.iter().as_slice().to_vec();
            voice_filename = filename;
        } else if name == String::from("model_name") {
            message_model = String::from_utf8(data.iter().as_slice().to_vec())
                .map_err(|e| format_error("Error parsing message model as string", e))?;
        } else if name == String::from("images") {
            image_filenames.push(filename);
            images.push(data.clone());
        }
    }
    if message_type.is_empty() || message_data.is_empty() || message_model.is_empty() {
        let error_message = format!("Something is missing in the payload: (type existing){}, (data existing){}, (model existing){}", message_type.is_empty(), message_data.is_empty(), message_model.is_empty());
        error!("{}", error_message);
        return Err((StatusCode::BAD_REQUEST, error_message));
    }
    info!(
        "User '{}' is attempting to send a message to conversation '{}'. Message type: {}, Message Model: {}",
        user.uid, conversation_id, message_type, message_model
    );

    save_message(
        state.clone(),
        user.uid,
        user.session_data,
        conversation_id,
        message_type,
        message_data,
        message_model,
        images,
        -1,
        voice_filename,
        image_filenames,
    )
    .await
}

pub async fn edit_message(
    Path(conversation_id): Path<Uuid>,
    State(state): State<Arc<ServiceState>>,
    user: UserClaims,
    mut multipart: Multipart,
) -> AppResult<impl IntoResponse> {
    let mut message_type = String::from("");
    let mut message_data: Vec<u8> = vec![];
    let mut message_model: String = String::from("");
    let mut message_id = 0 as i64;
    let mut images = vec![];
    let mut image_filenames = vec![];
    let mut voice_filename: Option<String> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| format_error("Failed to read multipart fields", e))?
    {
        let name = field.name();
        if name.is_none() {
            continue;
        }
        let filename = field.file_name().map(|s| s.to_string());
        let name = name.unwrap().to_string();
        let data = field.bytes().await;
        if data.is_err() {
            continue;
        }
        let data = data.unwrap();

        if name == String::from("message_type") {
            message_type = String::from_utf8(data.iter().as_slice().to_vec())
                .map_err(|e| format_error("Error parsing message type as string", e))?;
        } else if name == String::from("user_message") {
            message_data = data.iter().as_slice().to_vec();
            voice_filename = filename;
        } else if name == String::from("message_id") {
            message_id = String::from_utf8(data.iter().as_slice().to_vec())
                .map_err(|e| format_error("Error parsing message id as string", e))?
                .parse::<i64>()
                .map_err(|e| format_error("Error parsing string as u32", e))?;
        } else if name == String::from("model_name") {
            message_model = String::from_utf8(data.iter().as_slice().to_vec())
                .map_err(|e| format_error("Error parsing message model as string", e))?;
        } else if name == String::from("images") {
            image_filenames.push(filename);
            images.push(data.clone());
        }
    }
    if message_type.is_empty() || message_data.is_empty() || message_model.is_empty() {
        let error_message = format!("Something is missing in the payload: (type existing){}, (data existing){}, (model existing){}", message_type.is_empty(), message_data.is_empty(), message_model.is_empty());
        error!("{}", error_message);
        return Err((StatusCode::BAD_REQUEST, error_message));
    }
    info!(
        "User '{}' is attempting to send a message to conversation '{}'. Message type: {}, Message Model: {}",
        user.uid, conversation_id, message_type, message_model
    );

    save_message(
        state.clone(),
        user.uid,
        user.session_data,
        conversation_id,
        message_type,
        message_data,
        message_model,
        images,
        message_id,
        voice_filename,
        image_filenames,
    )
    .await
}

pub async fn edit_title(
    Path(conversation_id): Path<Uuid>,
    State(state): State<Arc<ServiceState>>,
    user: UserClaims,
    Json(req): Json<EditTitleRequest>,
) -> AppResult<impl IntoResponse> {
    info!(
        "User '{}' is editing the title of conversation '{}' to '{}'.",
        user.uid, conversation_id, req.title
    );
    handle_transaction(&state.db, |transaction| {
        Box::pin(async move {
            conversation::edit_title(transaction, user.uid, conversation_id, req.title.clone())
                .await
                .map_err(|e| {
                    format_error("Error updating the conversation title in the database", e)
                })?;

            info!(
                "Successfully updated title for conversation with ID '{}' to '{}'.",
                conversation_id, req.title
            );
            Ok(Json(EditTitleResponse {
                message: "Title successfully updated".to_string(),
            })
            .into_response())
        })
    })
    .await
}

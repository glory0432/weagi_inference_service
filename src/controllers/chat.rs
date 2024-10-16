use crate::dto::request::{EditMessageRequest, EditTitleRequest, SendMessageRequest};
use crate::dto::response::{
    CreateNewConversationResponse, DeleteConversationResponse, EditTitleResponse,
    GetConversationResponse, RetrieveAllConversationResponse,
};
use crate::repositories::conversation;
use crate::service::chat::save_message;
use crate::utils::jwt::UserClaims;
use crate::ServiceState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
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
            let conversation_list: Vec<(Uuid, String)> =
                conversation::find_by_user_id(transaction, user.uid)
                    .await
                    .map_err(|e| {
                        format_error(
                            "Failed to fetch user's conversations due to a database error",
                            e,
                        )
                    })?
                    .into_iter()
                    .map(|x| (x.id, x.title))
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
                Ok(Json(GetConversationResponse {
                    messages: model.conversation,
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
    Json(req): Json<SendMessageRequest>,
) -> AppResult<impl IntoResponse> {
    info!(
        "User '{}' is attempting to send a message to conversation '{}'. Message: '{}', Model: '{}'.",
        user.uid, conversation_id, req.user_message, req.model_name
    );

    save_message(
        state.clone(),
        user.uid,
        user.session_data,
        conversation_id,
        req.user_message,
        req.model_name,
        -1,
    )
    .await
}

pub async fn edit_message(
    Path(conversation_id): Path<Uuid>,
    State(state): State<Arc<ServiceState>>,
    user: UserClaims,
    Json(req): Json<EditMessageRequest>,
) -> AppResult<impl IntoResponse> {
    info!(
        "User '{}' requested to edit message with ID '{}' in conversation '{}'. New content: '{}', Model: '{}'.",  
        user.uid, req.message_id, conversation_id, req.user_message, req.model_name
    );
    save_message(
        state.clone(),
        user.uid,
        user.session_data,
        conversation_id,
        req.user_message,
        req.model_name,
        req.message_id as i64,
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

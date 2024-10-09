use crate::dto::request::{EditMessageRequest, EditTitleRequest, SendMessageRequest};
use crate::dto::response::{
    CreateNewConversationResponse, EditTitleResponse, GetConversationResponse,
    RetrieveAllConversationResponse,
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
use sea_orm::{DatabaseConnection, TransactionTrait};
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
        .map_err(|e| format_error("Failed to start a database transaction", e))?;

    let result = operation(&mut transaction).await;

    match result {
        Ok(response) => {
            transaction
                .commit()
                .await
                .map_err(|e| format_error("Failed to commit transaction", e))?;
            Ok(response)
        }
        Err(e) => {
            if let Err(rollback_err) = transaction.rollback().await {
                error!("Failed to rollback transaction: {}", rollback_err);
            }
            Err(e)
        }
    }
}

fn format_error(message: &str, error: impl std::fmt::Display) -> (StatusCode, String) {
    let error_message = format!("{}: {}", message, error);
    error!("{}", error_message);
    (StatusCode::INTERNAL_SERVER_ERROR, error_message)
}

pub async fn create_new_conversation(
    State(state): State<Arc<ServiceState>>,
    user: UserClaims,
) -> AppResult<impl IntoResponse> {
    info!("📥 Create new conversation request from user {}", user.uid);

    handle_transaction(&state.db, |transaction| {
        Box::pin(async move {
            let conversation_id = conversation::new_conversation(transaction, user.uid)
                .await
                .map_err(|e| format_error("Failed to create a new conversation", e))?;

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
        "📥 Retrieve all conversation request from user {}",
        user.uid
    );
    handle_transaction(&state.db, |transaction| {
        Box::pin(async move {
            let conversation_list: Vec<(Uuid, String)> =
                conversation::find_by_user_id(transaction, user.uid)
                    .await
                    .map_err(|e| format_error("Failed to fetch all conversation content", e))?
                    .into_iter()
                    .map(|x| (x.id, x.title))
                    .collect();

            Ok(Json(RetrieveAllConversationResponse { conversation_list }).into_response())
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
        "📥 Get the conversation request from user {} within conversation {}",
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
            .map_err(|e| format_error("Failed to fetch the conversation content", e))?;

            let response = if let Some(model) = conversation_model {
                Json(GetConversationResponse {
                    messages: model.conversation,
                })
                .into_response()
            } else {
                Json(GetConversationResponse::default()).into_response()
            };

            Ok(response)
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
        "📥 Send the message request from user {} within conversation {}",
        user.uid, conversation_id
    );
    info!(
        "🧾 Message: {}, Model name: {}",
        req.user_message, req.model_name
    );
    save_message(
        state.clone(),
        user.uid,
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
        "📥 Edit the message request from user {} within conversation {}",
        user.uid, conversation_id
    );
    info!(
        "🧾 Message: {}, Model name: {}, Message id: {}",
        req.user_message, req.model_name, req.message_id
    );
    save_message(
        state.clone(),
        user.uid,
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
    info!("📥 Edit the title of conversation request from user {} within conversation {} by the title {}", user.uid, conversation_id, req.title);
    handle_transaction(&state.db, |transaction| {
        Box::pin(async move {
            conversation::edit_title(transaction, user.uid, conversation_id, req.title)
                .await
                .map_err(|e| format_error("Failed to edit title", e))?;

            Ok(Json(EditTitleResponse {
                message: "success".to_string(),
            })
            .into_response())
        })
    })
    .await
}
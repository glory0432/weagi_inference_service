use std::sync::Arc;

use crate::controllers::chat;
use crate::ServiceState;
use axum::routing::{get, patch, post};

pub fn add_routers(router: axum::Router<Arc<ServiceState>>) -> axum::Router<Arc<ServiceState>> {
    router
        .route(
            "/api/chat/conversation/:conversation_id",
            get(chat::get_conversation),
        )
        .route(
            "/api/chat/conversation/:conversation_id",
            post(chat::send_message),
        )
        .route(
            "/api/chat/conversation/:conversation_id",
            patch(chat::edit_message),
        )
        .route(
            "/api/chat/conversation/:conversation_id/:title",
            patch(chat::edit_title),
        )
        .route(
            "/api/chat/conversation",
            get(chat::retrieve_all_conversations),
        )
        .route(
            "/api/chat/conversation",
            post(chat::create_new_conversation),
        )
}

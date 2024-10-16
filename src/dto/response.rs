use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionData {
    pub credits_remaining: i64,
    pub preferences: serde_json::Value,
    pub session_metadata: serde_json::Value,
    pub subscription_status: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct GetConversationResponse {
    pub messages: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RetrieveAllConversationResponse {
    pub conversation_list: Vec<(Uuid, String)>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CreateNewConversationResponse {
    pub conversation_id: Uuid,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct EditTitleResponse {
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct DeleteConversationResponse {
    pub message: String,
}

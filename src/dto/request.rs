use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SendMessageRequest {
    pub user_message: String,
    pub model_name: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct EditMessageRequest {
    pub message_id: usize,
    pub user_message: String,
    pub model_name: String,
}

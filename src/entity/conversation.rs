use chrono::{DateTime, Utc};
use rs_openai::chat::Role;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid; // Importing Uuid

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum MessageType {
    Text,
    Voice,
}
impl Default for MessageType {
    fn default() -> Self {
        MessageType::Text
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Message {
    #[serde(rename = "type")]
    pub msgtype: MessageType,
    pub id: usize,
    pub role: Role,
    pub content: String,
    pub transcription: Option<String>,
    pub images: Vec<String>,
}

#[derive(Debug, PartialEq, Eq, Clone, DeriveEntityModel)]
#[sea_orm(table_name = "conversations")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Uuid,
    pub user_id: i64,
    pub conversation: Vec<serde_json::Value>,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Copy, Clone, Debug, EnumIter)]
pub enum Relation {}

impl RelationTrait for Relation {
    fn def(&self) -> RelationDef {
        panic!("No relations are defined for this model!")
    }
}

impl ActiveModelBehavior for ActiveModel {}

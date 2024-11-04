use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct EditTitleRequest {
    pub title: String,
}

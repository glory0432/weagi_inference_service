use sea_orm::entity::prelude::*;
use uuid::Uuid; // Importing Uuid

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, DeriveEntityModel)]
#[sea_orm(table_name = "conversations")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Uuid,
    pub user_id: i64,
    pub conversation: Vec<String>,
    pub title: String,
}

#[derive(Copy, Clone, Debug, EnumIter)]
pub enum Relation {}

impl RelationTrait for Relation {
    fn def(&self) -> RelationDef {
        panic!("No relations are defined for this model!")
    }
}

impl ActiveModelBehavior for ActiveModel {}

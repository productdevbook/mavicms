use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// A mail account, or whatever else a step needs to be allowed to do its work.
///
/// Its own table because one account is used by several flows, and rotating it
/// should be one edit rather than five. `secret` is encrypted with the site's
/// key and never leaves the server.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "flow_credentials")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub name: String,
    /// "smtp" for now.
    pub kind: String,
    #[sea_orm(column_type = "Text")]
    pub secret: String,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

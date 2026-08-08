use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Something happened, so do these things.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "flows")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub name: String,
    /// What sets it off. See `crate::flows::trigger`.
    pub trigger_kind: String,
    /// That trigger's own settings, as JSON.
    #[sea_orm(column_type = "Text")]
    pub trigger_config: String,
    pub enabled: bool,
    /// The unguessable part of a webhook's address, when the trigger is one.
    pub webhook_key: Option<String>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// One thing a flow does, and where in the order it does it.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "flow_steps")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub flow_id: Uuid,
    pub position: i32,
    /// See `crate::flows::action`.
    pub action: String,
    #[sea_orm(column_type = "Text")]
    pub config: String,
    /// "stop" or "continue".
    pub on_error: String,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

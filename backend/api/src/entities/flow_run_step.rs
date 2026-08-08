use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// What one step of one run did. The answer to "did it work", which is the
/// question automation is always asked.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "flow_run_steps")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub run_id: Uuid,
    pub position: i32,
    pub action: String,
    /// "succeeded", "failed" or "skipped".
    pub status: String,
    #[sea_orm(column_type = "Text")]
    pub output: String,
    #[sea_orm(column_type = "Text", nullable)]
    pub error: Option<String>,
    pub finished_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// One time a flow ran, and what came of it.
///
/// The queue and the record are the same row, as they are for a build: a
/// crash loses neither the fact that something was asked for nor the reason
/// it failed.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "flow_runs")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub flow_id: Uuid,
    pub status: String,
    /// What set it off, as JSON. Steps read their values out of this.
    #[sea_orm(column_type = "Text")]
    pub trigger: String,
    #[sea_orm(column_type = "Text", nullable)]
    pub error: Option<String>,
    /// How many flows deep. What stops a flow that sets itself off.
    pub depth: i32,
    pub created_at: DateTimeWithTimeZone,
    pub started_at: Option<DateTimeWithTimeZone>,
    pub finished_at: Option<DateTimeWithTimeZone>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

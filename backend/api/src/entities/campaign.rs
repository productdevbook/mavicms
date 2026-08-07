use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, serde::Serialize, serde::Deserialize)]
#[sea_orm(table_name = "campaigns")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(column_type = "Text")]
    pub name: String,
    #[sea_orm(column_type = "Text")]
    pub subject: String,
    #[sea_orm(column_type = "Text")]
    pub body: String,
    pub template_id: Option<Uuid>,
    /// Which of the site's senders this goes out as. Empty means the default.
    pub from_address: String,
    /// draft, scheduled, running, paused, finished or cancelled.
    pub status: String,
    pub send_at: Option<DateTimeWithTimeZone>,
    pub started_at: Option<DateTimeWithTimeZone>,
    pub finished_at: Option<DateTimeWithTimeZone>,
    /// Counted when it starts, so progress means something while it runs.
    pub to_send: i32,
    pub sent: i32,
    pub failed: i32,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

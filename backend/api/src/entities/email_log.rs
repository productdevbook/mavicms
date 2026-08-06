use sea_orm::entity::prelude::*;

/// Every message this site tried to send, and what came of it.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "email_log")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub to_address: String,
    #[sea_orm(column_type = "Text")]
    pub subject: String,
    /// "sent" or "failed".
    pub status: String,
    /// What SES said when it refused. Empty when it did not.
    #[sea_orm(column_type = "Text")]
    pub detail: String,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

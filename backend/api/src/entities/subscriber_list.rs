use sea_orm::entity::prelude::*;

/// One person's place on one list.
///
/// The status is per list rather than per person: leaving the newsletter is
/// not leaving the order notifications.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, serde::Serialize, serde::Deserialize)]
#[sea_orm(table_name = "subscriber_lists")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub subscriber_id: Uuid,
    #[sea_orm(primary_key, auto_increment = false)]
    pub list_id: Uuid,
    /// "unconfirmed", "confirmed" or "unsubscribed".
    pub status: String,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

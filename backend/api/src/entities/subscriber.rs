use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, serde::Serialize, serde::Deserialize)]
#[sea_orm(table_name = "subscribers")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub email: String,
    #[sea_orm(column_type = "Text")]
    pub name: String,
    /// "enabled" or "blocked". Blocked outranks every list they are on.
    pub status: String,
    /// A JSON object of whatever else the site knows about them.
    #[sea_orm(column_type = "Text")]
    pub attributes: String,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

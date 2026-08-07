use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, serde::Serialize, serde::Deserialize)]
#[sea_orm(table_name = "media")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(column_type = "Text")]
    pub filename: String,
    #[sea_orm(column_type = "Text")]
    pub url_path: String,
    pub mime_type: String,
    pub size_bytes: i64,
    #[sea_orm(column_type = "Text")]
    pub alt_text: String,
    pub uploaded_at: DateTimeWithTimeZone,
    /// "local" or "s3" — which backend actually holds the bytes, so deletion
    /// still works after the active backend changes.
    pub storage_backend: String,
    #[sea_orm(column_type = "Text")]
    pub storage_key: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

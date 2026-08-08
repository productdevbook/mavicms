use sea_orm::entity::prelude::*;

/// A video on whichever host this site uses.
///
/// The bytes are not here and never were: this row is a name, a length and a
/// pointer, so that the panel can list what a site has without asking the host
/// about every one of them on every page load.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, serde::Serialize, serde::Deserialize)]
#[sea_orm(table_name = "video_assets")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    /// "bunny" or "cloudflare". Kept on the row rather than read from the
    /// plugin, so that a site which changes host can still find, play and
    /// delete what it uploaded under the old one.
    pub host: String,
    #[sea_orm(column_type = "Text")]
    pub host_id: String,
    #[sea_orm(column_type = "Text")]
    pub title: String,
    /// "uploading", "processing", "ready" or "failed".
    pub status: String,
    pub duration_seconds: i32,
    #[sea_orm(column_type = "Text")]
    pub thumbnail_url: String,
    pub size_bytes: i64,
    /// Why it failed, in the host's words. Empty otherwise.
    #[sea_orm(column_type = "Text")]
    pub error: String,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "posts")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub title: String,
    pub slug: String,
    pub excerpt: String,
    pub status: String,
    pub publish_at: Option<DateTimeWithTimeZone>,
    pub author: String,
    pub category: String,
    pub tags: Json,
    pub cover_url: String,
    pub seo_title: String,
    pub seo_description: String,
    pub canonical: String,
    pub featured: bool,
    pub allow_comments: bool,
    #[sea_orm(column_type = "Text")]
    pub content_html: String,
    pub locale: String,
    /// Rows sharing this id are translations of one another. Nullable in the
    /// schema only because a uuid column cannot take a portable NOT NULL
    /// default; every write path sets it, so a NULL here is a bug and should
    /// fail loudly rather than be silently tolerated.
    pub translation_group_id: Uuid,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

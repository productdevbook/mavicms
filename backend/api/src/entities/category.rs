use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "categories")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(column_type = "Text")]
    pub name: String,
    pub slug: String,
    pub parent_id: Option<Uuid>,
    #[sea_orm(column_type = "Text")]
    pub description: String,
    pub locale: String,
    /// See `post::Model::translation_group_id`.
    pub translation_group_id: Uuid,
    /// "complete" or "needs_translation" — flags stubs auto-created when a
    /// post in a new language needed a category that had no version yet.
    pub translation_status: String,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "Entity",
        from = "Column::ParentId",
        to = "Column::Id"
    )]
    Parent,
}

impl ActiveModelBehavior for ActiveModel {}

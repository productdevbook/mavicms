use sea_orm::entity::prelude::*;

/// A deleted thing, kept whole.
///
/// See the migration for why this is a table rather than a flag on the others.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "trash")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    /// "post", "page", "media", "form" or "form_submission".
    pub kind: String,
    /// The id the row had before it was deleted.
    pub subject_id: Uuid,
    #[sea_orm(column_type = "Text")]
    pub title: String,
    /// The row and its dependants, as JSON.
    #[sea_orm(column_type = "Text")]
    pub payload: String,
    pub deleted_at: DateTimeWithTimeZone,
    pub deleted_by: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

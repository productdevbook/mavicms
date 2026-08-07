use sea_orm::entity::prelude::*;

/// A kind of thing this site publishes.
///
/// `post` and `page` are rows here like any other, so that "which kinds are
/// there" is a query rather than a match arm, and so that a blog post can gain
/// a field the same way a course can.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "content_types")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    /// What `kind` on a post holds, and what a front end asks for. Fixed once
    /// made: it is in the addresses somebody's build already fetches.
    pub slug: String,
    pub name: String,
    /// What a list of them is called. "Courses" beside "Course".
    pub plural: String,
    /// A JSON array of field definitions — the same shape a form's fields
    /// have, because it is the same question.
    #[sea_orm(column_type = "Text")]
    pub fields: String,
    /// `post` and `page`. They may gain fields and may not be removed.
    pub built_in: bool,
    pub sort_order: i32,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

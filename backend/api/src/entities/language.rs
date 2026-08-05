use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "languages")]
pub struct Model {
    /// BCP-47 tag ("tr", "en", "pt-BR"). Immutable: it is denormalized into
    /// posts/categories/tags without a foreign key, so changing it would
    /// orphan every row that references it.
    #[sea_orm(primary_key, auto_increment = false)]
    pub code: String,
    #[sea_orm(column_type = "Text")]
    pub name: String,
    #[sea_orm(column_type = "Text")]
    pub native_name: String,
    /// "ltr" or "rtl" — drives `dir` on the editor for Arabic/Hebrew.
    pub direction: String,
    pub is_default: bool,
    pub is_active: bool,
    pub sort_order: i32,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

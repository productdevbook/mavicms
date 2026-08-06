use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "forms")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(column_type = "Text")]
    pub name: String,
    pub slug: String,
    #[sea_orm(column_type = "Text")]
    pub description: String,
    /// A JSON array of field definitions — see `dto::forms::FormField`.
    #[sea_orm(column_type = "Text")]
    pub fields: String,
    pub active: bool,
    /// Where to tell somebody a submission arrived. Empty means nowhere.
    pub notify: String,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::form_submission::Entity")]
    Submissions,
}

impl Related<super::form_submission::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Submissions.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

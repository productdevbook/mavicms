use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, serde::Serialize, serde::Deserialize)]
#[sea_orm(table_name = "post_categories")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub post_id: Uuid,
    #[sea_orm(primary_key, auto_increment = false)]
    pub category_id: Uuid,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

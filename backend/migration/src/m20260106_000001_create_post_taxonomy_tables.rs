use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(PostCategories::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(PostCategories::PostId).uuid().not_null())
                    .col(ColumnDef::new(PostCategories::CategoryId).uuid().not_null())
                    .primary_key(
                        Index::create()
                            .col(PostCategories::PostId)
                            .col(PostCategories::CategoryId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-post_categories-post_id")
                            .from(PostCategories::Table, PostCategories::PostId)
                            .to(Posts::Table, Posts::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-post_categories-category_id")
                            .from(PostCategories::Table, PostCategories::CategoryId)
                            .to(Categories::Table, Categories::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(PostTags::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(PostTags::PostId).uuid().not_null())
                    .col(ColumnDef::new(PostTags::TagId).uuid().not_null())
                    .primary_key(
                        Index::create()
                            .col(PostTags::PostId)
                            .col(PostTags::TagId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-post_tags-post_id")
                            .from(PostTags::Table, PostTags::PostId)
                            .to(Posts::Table, Posts::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-post_tags-tag_id")
                            .from(PostTags::Table, PostTags::TagId)
                            .to(Tags::Table, Tags::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(PostTags::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(PostCategories::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum PostCategories {
    Table,
    PostId,
    CategoryId,
}

#[derive(DeriveIden)]
enum PostTags {
    Table,
    PostId,
    TagId,
}

#[derive(DeriveIden)]
enum Posts {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Categories {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Tags {
    Table,
    Id,
}

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Posts::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Posts::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(Posts::Title).text().not_null())
                    .col(ColumnDef::new(Posts::Slug).string_len(255).not_null())
                    .col(ColumnDef::new(Posts::Excerpt).text().not_null())
                    .col(ColumnDef::new(Posts::Status).string_len(20).not_null())
                    .col(ColumnDef::new(Posts::PublishAt).timestamp_with_time_zone())
                    .col(ColumnDef::new(Posts::Author).string_len(255).not_null())
                    .col(ColumnDef::new(Posts::Category).string_len(255).not_null())
                    .col(ColumnDef::new(Posts::Tags).json().not_null())
                    .col(ColumnDef::new(Posts::CoverUrl).text().not_null())
                    .col(ColumnDef::new(Posts::SeoTitle).text().not_null())
                    .col(ColumnDef::new(Posts::SeoDescription).text().not_null())
                    .col(ColumnDef::new(Posts::Canonical).text().not_null())
                    .col(ColumnDef::new(Posts::Featured).boolean().not_null())
                    .col(ColumnDef::new(Posts::AllowComments).boolean().not_null())
                    .col(ColumnDef::new(Posts::ContentHtml).text().not_null())
                    .col(
                        ColumnDef::new(Posts::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Posts::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx-posts-slug")
                    .table(Posts::Table)
                    .col(Posts::Slug)
                    .unique()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Posts::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Posts {
    Table,
    Id,
    Title,
    Slug,
    Excerpt,
    Status,
    PublishAt,
    Author,
    Category,
    Tags,
    CoverUrl,
    SeoTitle,
    SeoDescription,
    Canonical,
    Featured,
    AllowComments,
    ContentHtml,
    CreatedAt,
    UpdatedAt,
}

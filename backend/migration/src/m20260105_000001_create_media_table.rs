use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Media::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Media::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(Media::Filename).text().not_null())
                    .col(ColumnDef::new(Media::UrlPath).text().not_null())
                    .col(ColumnDef::new(Media::MimeType).string_len(100).not_null())
                    .col(ColumnDef::new(Media::SizeBytes).big_integer().not_null())
                    .col(ColumnDef::new(Media::AltText).text().not_null())
                    .col(
                        ColumnDef::new(Media::UploadedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Media::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Media {
    Table,
    Id,
    Filename,
    UrlPath,
    MimeType,
    SizeBytes,
    AltText,
    UploadedAt,
}

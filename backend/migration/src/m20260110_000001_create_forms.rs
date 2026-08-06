use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Forms::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Forms::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(Forms::Name).text().not_null())
                    .col(ColumnDef::new(Forms::Slug).string_len(255).not_null())
                    .col(ColumnDef::new(Forms::Description).text().not_null())
                    // JSON array of field definitions. Stored as text for the
                    // same reason everything else here is: one shape across
                    // SQLite, Postgres and MySQL.
                    .col(ColumnDef::new(Forms::Fields).text().not_null())
                    .col(
                        ColumnDef::new(Forms::Active)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(
                        ColumnDef::new(Forms::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Forms::UpdatedAt)
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
                    .name("idx-forms-slug")
                    .table(Forms::Table)
                    .col(Forms::Slug)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(FormSubmissions::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(FormSubmissions::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(FormSubmissions::FormId).uuid().not_null())
                    .col(ColumnDef::new(FormSubmissions::Data).text().not_null())
                    // Not "read": that is a reserved word in MySQL.
                    .col(
                        ColumnDef::new(FormSubmissions::Seen)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(FormSubmissions::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-form_submissions-form_id")
                            .from(FormSubmissions::Table, FormSubmissions::FormId)
                            .to(Forms::Table, Forms::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx-form_submissions-form_id-created_at")
                    .table(FormSubmissions::Table)
                    .col(FormSubmissions::FormId)
                    .col(FormSubmissions::CreatedAt)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(FormSubmissions::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Forms::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Forms {
    Table,
    Id,
    Name,
    Slug,
    Description,
    Fields,
    Active,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum FormSubmissions {
    Table,
    Id,
    FormId,
    Data,
    Seen,
    CreatedAt,
}

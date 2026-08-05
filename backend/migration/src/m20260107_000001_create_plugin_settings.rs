use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(PluginSettings::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(PluginSettings::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(PluginSettings::Plugin)
                            .string_len(60)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(PluginSettings::Enabled)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    // Encrypted blob (see api/src/crypto.rs) — never plaintext.
                    .col(ColumnDef::new(PluginSettings::Config).text().not_null())
                    .col(
                        ColumnDef::new(PluginSettings::UpdatedAt)
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
                    .name("idx-plugin_settings-plugin")
                    .table(PluginSettings::Table)
                    .col(PluginSettings::Plugin)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Media::Table)
                    .add_column(
                        ColumnDef::new(Media::StorageBackend)
                            .string_len(20)
                            .not_null()
                            .default("local"),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Media::Table)
                    .add_column(
                        // varchar, not text: MySQL rejects a DEFAULT on
                        // TEXT/BLOB columns ("can't have a default value"),
                        // and the default is what backfills existing rows.
                        ColumnDef::new(Media::StorageKey)
                            .string_len(512)
                            .not_null()
                            .default(""),
                    )
                    .to_owned(),
            )
            .await?;

        // Backfill the key for rows written before this column existed; their
        // url_path is always "/uploads/{key}" (9 characters of prefix).
        // Fixed literal SQL, portable across sqlite/postgres/mysql.
        manager
            .get_connection()
            .execute_unprepared(
                "UPDATE media SET storage_key = SUBSTR(url_path, 10) WHERE url_path LIKE '/uploads/%'",
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Media::Table)
                    .drop_column(Media::StorageKey)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Media::Table)
                    .drop_column(Media::StorageBackend)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(Table::drop().table(PluginSettings::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum PluginSettings {
    Table,
    Id,
    Plugin,
    Enabled,
    Config,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Media {
    Table,
    StorageBackend,
    StorageKey,
}

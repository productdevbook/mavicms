use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    fn use_transaction(&self) -> Option<bool> {
        Some(true)
    }

    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.has_column("posts", "content_markdown").await? {
            return Ok(());
        }

        // Nullable and without a default on purpose: MySQL refuses a default on
        // a TEXT column, and a post that predates this column genuinely has no
        // markdown yet — NULL says so, where an empty string would look like a
        // post someone had emptied.
        manager
            .alter_table(
                Table::alter()
                    .table(Posts::Table)
                    .add_column(ColumnDef::new(Posts::ContentMarkdown).text().null())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if !manager.has_column("posts", "content_markdown").await? {
            return Ok(());
        }
        manager
            .alter_table(
                Table::alter()
                    .table(Posts::Table)
                    .drop_column(Posts::ContentMarkdown)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum Posts {
    Table,
    ContentMarkdown,
}

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Forms::Table)
                    .add_column(
                        // varchar, not text: MySQL rejects a DEFAULT on
                        // TEXT/BLOB, and the default is what fills the rows
                        // that already exist.
                        ColumnDef::new(Forms::Notify)
                            .string_len(320)
                            .not_null()
                            .default(""),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Forms::Table)
                    .drop_column(Forms::Notify)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum Forms {
    Table,
    Notify,
}

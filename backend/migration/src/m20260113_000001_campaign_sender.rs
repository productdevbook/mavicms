use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // One column per statement: SQLite panics on two in one alter_table.
        manager
            .alter_table(
                Table::alter()
                    .table(Campaigns::Table)
                    .add_column(
                        // varchar with a default, not text: MySQL refuses a
                        // DEFAULT on TEXT, and the default is what fills the
                        // campaigns that already exist.
                        ColumnDef::new(Campaigns::FromAddress)
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
                    .table(Campaigns::Table)
                    .drop_column(Campaigns::FromAddress)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum Campaigns {
    Table,
    FromAddress,
}

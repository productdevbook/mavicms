use sea_orm_migration::prelude::*;

/// Somewhere for deleted things to go instead of nowhere.
///
/// A table of its own rather than a `deleted_at` column on each of the others,
/// which is the usual way and the wrong one here. A flag means every read in
/// the program has to remember to exclude it, and the day one of them forgets,
/// deleted posts are back in somebody's archive. It also puts a deleted row's
/// address in the way of the next one to use it: the unique index on
/// (kind, locale, slug) cannot tell a live post from a discarded one, and
/// partial indexes are not something all three databases here have.
///
/// So the row leaves its table and its whole self is kept here, dependants and
/// all. Nothing that reads posts has to change, nothing collides, and putting
/// it back is writing the same rows again.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Trash::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Trash::Id).uuid().not_null().primary_key())
                    // "post", "page", "media", "form", "form_submission".
                    .col(ColumnDef::new(Trash::Kind).string_len(40).not_null())
                    // The id the row had, so a second delete of the same thing
                    // can be recognised rather than kept twice.
                    .col(ColumnDef::new(Trash::SubjectId).uuid().not_null())
                    // What to show in the list. A person looking for what they
                    // deleted knows the title, not the id.
                    .col(ColumnDef::new(Trash::Title).text().not_null())
                    // The row and everything that hung off it, as JSON.
                    .col(ColumnDef::new(Trash::Payload).text().not_null())
                    .col(
                        ColumnDef::new(Trash::DeletedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    // Whoever pressed it, or whichever assistant did.
                    .col(ColumnDef::new(Trash::DeletedBy).string_len(120).not_null())
                    .to_owned(),
            )
            .await?;

        if !manager.has_index("trash", "idx-trash-deleted-at").await? {
            manager
                .create_index(
                    Index::create()
                        .name("idx-trash-deleted-at")
                        .table(Trash::Table)
                        .col(Trash::DeletedAt)
                        .to_owned(),
                )
                .await?;
        }

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.has_index("trash", "idx-trash-deleted-at").await? {
            manager
                .drop_index(
                    Index::drop()
                        .name("idx-trash-deleted-at")
                        .table(Trash::Table)
                        .to_owned(),
                )
                .await?;
        }
        manager
            .drop_table(Table::drop().table(Trash::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Trash {
    Table,
    Id,
    Kind,
    SubjectId,
    Title,
    Payload,
    DeletedAt,
    DeletedBy,
}

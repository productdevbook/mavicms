use sea_orm_migration::prelude::*;

/// Pages: the About, the Contact, the one with the opening hours on it.
///
/// A column on `posts` rather than a table of its own. Everything a page needs
/// — a title, an address, a body, the same body in three languages, SEO,
/// scheduling, a digest a build can key on — is machinery that exists here
/// already, and a second content type with half of it would be worse than no
/// second content type at all. What actually differs between a post and a page
/// is whether it belongs in the feed, and that is one word.
#[derive(DeriveMigrationName)]
pub struct Migration;

/// What everything written before this is.
const DEFAULT_KIND: &str = "post";

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if !manager.has_column("posts", "kind").await? {
            manager
                .alter_table(
                    Table::alter()
                        .table(Posts::Table)
                        .add_column(
                            ColumnDef::new(Posts::Kind)
                                .string_len(20)
                                .not_null()
                                .default(DEFAULT_KIND),
                        )
                        .to_owned(),
                )
                .await?;
        }

        // The address has to be unique within a kind rather than across both.
        // A site with a post about its opening hours and a page of them is not
        // a site with a conflict: those are two addresses, /blog/hours and
        // /hours, and only the front end knows they are laid out that way.
        if manager.has_index("posts", "idx-posts-locale-slug").await? {
            manager
                .drop_index(
                    Index::drop()
                        .name("idx-posts-locale-slug")
                        .table(Posts::Table)
                        .to_owned(),
                )
                .await?;
        }

        if !manager
            .has_index("posts", "idx-posts-kind-locale-slug")
            .await?
        {
            manager
                .create_index(
                    Index::create()
                        .name("idx-posts-kind-locale-slug")
                        .table(Posts::Table)
                        .col(Posts::Kind)
                        .col(Posts::Locale)
                        .col(Posts::Slug)
                        .unique()
                        .to_owned(),
                )
                .await?;
        }

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager
            .has_index("posts", "idx-posts-kind-locale-slug")
            .await?
        {
            manager
                .drop_index(
                    Index::drop()
                        .name("idx-posts-kind-locale-slug")
                        .table(Posts::Table)
                        .to_owned(),
                )
                .await?;
        }

        manager
            .alter_table(
                Table::alter()
                    .table(Posts::Table)
                    .drop_column(Posts::Kind)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum Posts {
    Table,
    Kind,
    Locale,
    Slug,
}

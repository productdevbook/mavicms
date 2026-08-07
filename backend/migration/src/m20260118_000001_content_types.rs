use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::ConnectionTrait;

/// Content types: courses, packages, properties, whatever the site is about.
///
/// A site that sells training wants a course to have a price, a length and a
/// level. A site that lets flats wants rooms and a floor. Neither is a blog
/// post with the numbers typed into the paragraph, and neither is worth a
/// bespoke feature — so a site says what its own kinds of thing are, and what
/// each one is made of.
///
/// The two halves of this already existed and had not been introduced. A post
/// learned last week that it has a `kind`. A form has known for months how to
/// describe a set of fields and check something against them. A content type
/// is those two facts in one row: a name, and the fields that go with it.
///
/// `post` and `page` are rows here like any other, seeded rather than special
/// -cased, so that a blog post can gain a field the same way a course can.
/// They cannot be removed; nothing else about them differs.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(ContentTypes::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ContentTypes::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    // What `kind` on a post holds. Fixed once made: it is in
                    // the addresses a front end fetches.
                    .col(
                        ColumnDef::new(ContentTypes::Slug)
                            .string_len(60)
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(ContentTypes::Name).text().not_null())
                    // What a list of them is called: "Courses" beside "Course".
                    .col(ColumnDef::new(ContentTypes::Plural).text().not_null())
                    // The same shape a form's fields have, because it is the
                    // same question: what is this thing made of.
                    .col(ColumnDef::new(ContentTypes::Fields).text().not_null())
                    // post and page. Removing them would leave rows with a
                    // kind nothing describes.
                    .col(
                        ColumnDef::new(ContentTypes::BuiltIn)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(ContentTypes::SortOrder)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(ContentTypes::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(ContentTypes::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        // What each post carries for its type's fields. Empty for a post that
        // has none, which is every post that exists today.
        if !manager.has_column("posts", "fields").await? {
            manager
                .alter_table(
                    Table::alter()
                        .table(Posts::Table)
                        .add_column(
                            ColumnDef::new(Posts::Fields)
                                .text()
                                .not_null()
                                .default("{}"),
                        )
                        .to_owned(),
                )
                .await?;
        }

        seed(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Posts::Table)
                    .drop_column(Posts::Fields)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(Table::drop().table(ContentTypes::Table).to_owned())
            .await
    }
}

/// The two every site has. Written as rows rather than assumed in code, so
/// that "which kinds are there" has one answer and it is a query.
///
/// Built through the query builder rather than as raw SQL: three databases
/// spell their placeholders three ways, and this runs on all of them.
async fn seed(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let db = manager.get_connection();
    let now = chrono::Utc::now().fixed_offset();

    for (slug, name, plural, order) in [("post", "Post", "Posts", 0), ("page", "Page", "Pages", 1)]
    {
        let looking = Query::select()
            .column(ContentTypes::Id)
            .from(ContentTypes::Table)
            .and_where(Expr::col(ContentTypes::Slug).eq(slug))
            .to_owned();

        if db.query_one(&looking).await?.is_some() {
            continue;
        }

        let adding = Query::insert()
            .into_table(ContentTypes::Table)
            .columns([
                ContentTypes::Id,
                ContentTypes::Slug,
                ContentTypes::Name,
                ContentTypes::Plural,
                ContentTypes::Fields,
                ContentTypes::BuiltIn,
                ContentTypes::SortOrder,
                ContentTypes::CreatedAt,
                ContentTypes::UpdatedAt,
            ])
            .values_panic([
                uuid::Uuid::now_v7().into(),
                slug.into(),
                name.into(),
                plural.into(),
                "[]".into(),
                true.into(),
                order.into(),
                now.into(),
                now.into(),
            ])
            .to_owned();

        db.execute(&adding).await?;
    }

    Ok(())
}

#[derive(DeriveIden)]
enum ContentTypes {
    Table,
    Id,
    Slug,
    Name,
    Plural,
    Fields,
    BuiltIn,
    SortOrder,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Posts {
    Table,
    Fields,
}

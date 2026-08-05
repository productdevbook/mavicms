use sea_orm_migration::{
    prelude::*,
    sea_orm::{ConnectionTrait, Statement},
};

#[derive(DeriveMigrationName)]
pub struct Migration;

/// Long enough for the widest real BCP-47 tags (`ca-ES-valencia` is 14, and
/// Polylang exports `en_US`). SQLite cannot widen a column afterwards, so this
/// is a one-shot decision.
const LOCALE_LEN: u32 = 35;

const CONTENT_TABLES: [(&str, &str); 3] = [
    ("posts", "idx-posts-slug"),
    ("categories", "idx-categories-slug"),
    ("tags", "idx-tags-slug"),
];

fn language_name(code: &str) -> (&'static str, &'static str) {
    match code.split(['-', '_']).next().unwrap_or("") {
        "tr" => ("Turkish", "Türkçe"),
        "en" => ("English", "English"),
        "de" => ("German", "Deutsch"),
        "fr" => ("French", "Français"),
        "es" => ("Spanish", "Español"),
        "ar" => ("Arabic", "العربية"),
        _ => ("Language", "Language"),
    }
}

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    /// SQLite and MySQL run migrations without a transaction by default, and
    /// `Migrator::up` runs on every boot — a half-applied migration would
    /// crash-loop the process. Ask for a transaction and guard every step so
    /// re-running is harmless.
    fn use_transaction(&self) -> Option<bool> {
        Some(true)
    }

    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        let backend = manager.get_database_backend();

        manager
            .create_table(
                Table::create()
                    .table(Languages::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Languages::Code)
                            .string_len(LOCALE_LEN)
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Languages::Name).text().not_null())
                    .col(ColumnDef::new(Languages::NativeName).text().not_null())
                    .col(
                        ColumnDef::new(Languages::Direction)
                            .string_len(3)
                            .not_null()
                            .default("ltr"),
                    )
                    .col(
                        ColumnDef::new(Languages::IsDefault)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(Languages::IsActive)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(
                        ColumnDef::new(Languages::SortOrder)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(Languages::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        // An existing install already has a locale in site_settings. A fresh one
        // does not — `Migrator::up` runs before `POST /setup` writes that row —
        // so `None` here means "not installed yet" and seeding is left to
        // run_setup, which knows the language the admin actually picked.
        let site_locale = db
            .query_one_raw(Statement::from_string(
                backend,
                "SELECT locale FROM site_settings LIMIT 1",
            ))
            .await
            .ok()
            .flatten()
            .and_then(|row| row.try_get::<String>("", "locale").ok())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());

        // Only used as the column DEFAULT that backfills existing rows; on a
        // fresh database there are no rows for it to apply to.
        let default_locale = site_locale.clone().unwrap_or_else(|| "en".to_string());

        let existing_languages = db
            .query_one_raw(Statement::from_string(
                backend,
                "SELECT COUNT(*) AS count FROM languages",
            ))
            .await?
            .and_then(|row| row.try_get::<i64>("", "count").ok())
            .unwrap_or(0);

        if let Some(site_locale) = site_locale.as_deref().filter(|_| existing_languages == 0) {
            let (name, native_name) = language_name(site_locale);
            // Built through sea-query rather than raw SQL so the placeholder
            // style matches the dialect ($1 on Postgres, ? on MySQL/SQLite).
            db.execute(
                Query::insert()
                    .into_table(Languages::Table)
                    .columns([
                        Languages::Code,
                        Languages::Name,
                        Languages::NativeName,
                        Languages::Direction,
                        Languages::IsDefault,
                        Languages::IsActive,
                        Languages::SortOrder,
                        Languages::CreatedAt,
                    ])
                    .values_panic([
                        site_locale.into(),
                        name.into(),
                        native_name.into(),
                        "ltr".into(),
                        true.into(),
                        true.into(),
                        0.into(),
                        chrono::Utc::now().fixed_offset().into(),
                    ]),
            )
            .await?;
        }

        for (table, legacy_index) in CONTENT_TABLES {
            let table_iden = Alias::new(table);

            // One alter option per statement: sea-query panics on SQLite with
            // "doesn't support multiple alter options".
            if !manager.has_column(table, "locale").await? {
                manager
                    .alter_table(
                        Table::alter()
                            .table(table_iden.clone())
                            .add_column(
                                ColumnDef::new(Alias::new("locale"))
                                    .string_len(LOCALE_LEN)
                                    .not_null()
                                    // A string default is legal on all three
                                    // dialects and backfills existing rows for
                                    // free, so no UPDATE is needed. It also
                                    // avoids NULL->NOT NULL, which SQLite
                                    // cannot do at all.
                                    .default(default_locale.clone()),
                            )
                            .to_owned(),
                    )
                    .await?;
            }

            if !manager.has_column(table, "translation_group_id").await? {
                manager
                    .alter_table(
                        Table::alter()
                            .table(table_iden.clone())
                            // Nullable on purpose: a uuid column cannot take a
                            // portable NOT NULL default (MySQL maps uuid to
                            // binary(16) and would reject the 36-char literal).
                            // Every write path sets it, and the entity declares
                            // it non-Option so a NULL fails loudly rather than
                            // silently differing per dialect.
                            .add_column(ColumnDef::new(Alias::new("translation_group_id")).uuid())
                            .to_owned(),
                    )
                    .await?;
            }

            db.execute_raw(Statement::from_string(
                backend,
                format!(
                    "UPDATE {table} SET translation_group_id = id WHERE translation_group_id IS NULL"
                ),
            ))
            .await?;

            // `.if_exists()` panics on MySQL, and `.if_not_exists()` is
            // silently dropped there — guard with has_index instead.
            if manager.has_index(table, legacy_index).await? {
                manager
                    .drop_index(
                        Index::drop()
                            .name(legacy_index)
                            .table(table_iden.clone())
                            .to_owned(),
                    )
                    .await?;
            }

            let locale_slug_index = format!("idx-{table}-locale-slug");
            if !manager.has_index(table, &locale_slug_index).await? {
                manager
                    .create_index(
                        Index::create()
                            .name(&locale_slug_index)
                            .table(table_iden.clone())
                            .col(Alias::new("locale"))
                            .col(Alias::new("slug"))
                            .unique()
                            .to_owned(),
                    )
                    .await?;
            }

            let group_index = format!("idx-{table}-group-locale");
            if !manager.has_index(table, &group_index).await? {
                manager
                    .create_index(
                        Index::create()
                            .name(&group_index)
                            .table(table_iden.clone())
                            .col(Alias::new("translation_group_id"))
                            .col(Alias::new("locale"))
                            .unique()
                            .to_owned(),
                    )
                    .await?;
            }
        }

        // Lets the panel flag auto-created translation stubs.
        for table in ["categories", "tags"] {
            if !manager.has_column(table, "translation_status").await? {
                manager
                    .alter_table(
                        Table::alter()
                            .table(Alias::new(table))
                            .add_column(
                                ColumnDef::new(Alias::new("translation_status"))
                                    .string_len(20)
                                    .not_null()
                                    .default("complete"),
                            )
                            .to_owned(),
                    )
                    .await?;
            }
        }

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for table in ["categories", "tags"] {
            if manager.has_column(table, "translation_status").await? {
                manager
                    .alter_table(
                        Table::alter()
                            .table(Alias::new(table))
                            .drop_column(Alias::new("translation_status"))
                            .to_owned(),
                    )
                    .await?;
            }
        }

        for (table, legacy_index) in CONTENT_TABLES {
            let table_iden = Alias::new(table);

            // Indexes first: SQLite refuses to drop an indexed column.
            for index in [
                format!("idx-{table}-locale-slug"),
                format!("idx-{table}-group-locale"),
            ] {
                if manager.has_index(table, &index).await? {
                    manager
                        .drop_index(
                            Index::drop()
                                .name(&index)
                                .table(table_iden.clone())
                                .to_owned(),
                        )
                        .await?;
                }
            }

            for column in ["translation_group_id", "locale"] {
                if manager.has_column(table, column).await? {
                    manager
                        .alter_table(
                            Table::alter()
                                .table(table_iden.clone())
                                .drop_column(Alias::new(column))
                                .to_owned(),
                        )
                        .await?;
                }
            }

            if !manager.has_index(table, legacy_index).await? {
                manager
                    .create_index(
                        Index::create()
                            .name(legacy_index)
                            .table(table_iden.clone())
                            .col(Alias::new("slug"))
                            .unique()
                            .to_owned(),
                    )
                    .await?;
            }
        }

        manager
            .drop_table(Table::drop().table(Languages::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Languages {
    Table,
    Code,
    Name,
    NativeName,
    Direction,
    IsDefault,
    IsActive,
    SortOrder,
    CreatedAt,
}

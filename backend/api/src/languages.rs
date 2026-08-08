use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    QueryOrder,
};

use crate::{
    entities::language,
    error::{AppError, AppResult},
};

/// Canonical form for a BCP-47 tag: lowercase language, uppercase region
/// ("pt-BR"). MySQL's default collation is case-insensitive while Postgres and
/// SQLite are not, so codes are normalized on the way in rather than relying on
/// the database to consider "EN" and "en" the same.
pub fn normalize_code(input: &str) -> String {
    let trimmed = input.trim().replace('_', "-");
    let mut parts = trimmed.split('-');

    let Some(primary) = parts.next().filter(|p| !p.is_empty()) else {
        return String::new();
    };

    let mut out = primary.to_lowercase();
    for part in parts {
        out.push('-');
        if part.len() == 2 {
            out.push_str(&part.to_uppercase());
        } else {
            out.push_str(&part.to_lowercase());
        }
    }
    out
}

pub fn validate_code(code: &str) -> AppResult<()> {
    if code.is_empty() || code.len() > 35 {
        return Err(AppError::Validation(
            "language code must be 1-35 characters".to_string(),
        ));
    }
    if !code.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(AppError::Validation(
            "language code may only contain letters, digits and -".to_string(),
        ));
    }
    Ok(())
}

pub async fn all(db: &impl ConnectionTrait) -> AppResult<Vec<language::Model>> {
    Ok(language::Entity::find()
        .order_by_asc(language::Column::SortOrder)
        .order_by_asc(language::Column::Code)
        .all(db)
        .await?)
}

/// The locale new content gets when the client doesn't say.
///
/// The site's own language, never a language it does not have. This used to
/// answer "en" whenever no row was marked default, and a site can reach that
/// state — one opened from the console never runs the wizard that marks the
/// first one. Content was then written in a language the site had never heard
/// of, which is a post that exists and appears nowhere that lists a language's
/// content. A Turkish site ended up with Turkish posts filed under English.
///
/// So: the default, or failing that whichever language the site does have, and
/// "en" only when it has none at all — which is a site that cannot have
/// content yet anyway.
pub async fn default_code(db: &impl ConnectionTrait) -> AppResult<String> {
    if let Some(row) = language::Entity::find()
        .filter(language::Column::IsDefault.eq(true))
        .filter(language::Column::IsActive.eq(true))
        .one(db)
        .await?
    {
        return Ok(row.code);
    }

    Ok(language::Entity::find()
        .filter(language::Column::IsActive.eq(true))
        .order_by_asc(language::Column::CreatedAt)
        .order_by_asc(language::Column::SortOrder)
        .order_by_asc(language::Column::Code)
        .one(db)
        .await?
        .map(|row| row.code)
        .unwrap_or_else(|| "en".to_string()))
}

/// Makes sure exactly one language is the site's own.
///
/// Called where a site's languages are read for writing content. A site with
/// languages and no default is a site whose next post goes somewhere nobody
/// asked for, and the repair is cheap enough to do rather than report.
pub async fn ensure_a_default(db: &impl ConnectionTrait) -> AppResult<()> {
    if language::Entity::find()
        .filter(language::Column::IsDefault.eq(true))
        .filter(language::Column::IsActive.eq(true))
        .one(db)
        .await?
        .is_some()
    {
        return Ok(());
    }

    // Oldest first: the language a site started with is its own, and one added
    // later — by an operator naming a starting set, say — must not quietly
    // become the language its existing content is not written in.
    let Some(first) = language::Entity::find()
        .filter(language::Column::IsActive.eq(true))
        .order_by_asc(language::Column::CreatedAt)
        .order_by_asc(language::Column::SortOrder)
        .order_by_asc(language::Column::Code)
        .one(db)
        .await?
    else {
        return Ok(());
    };

    tracing::warn!(code = %first.code, "no default language; making the first one it");
    let mut row: language::ActiveModel = first.into();
    row.is_default = Set(true);
    row.update(db).await?;
    Ok(())
}

/// Resolves a caller-supplied locale, rejecting unknown or inactive ones so a
/// typo can't silently create content in a language that doesn't exist.
pub async fn resolve(db: &impl ConnectionTrait, requested: Option<&str>) -> AppResult<String> {
    let Some(requested) = requested.map(normalize_code).filter(|c| !c.is_empty()) else {
        return default_code(db).await;
    };

    let found = language::Entity::find_by_id(requested.clone())
        .one(db)
        .await?
        .ok_or_else(|| AppError::Validation(format!("unknown language: {requested}")))?;

    if !found.is_active {
        return Err(AppError::Validation(format!(
            "language {requested} is not active"
        )));
    }
    Ok(found.code)
}

/// Which languages a site starts with, when whoever runs the server has said.
///
/// A site opened from the console never runs the setup wizard, so without this
/// its languages depend on whichever migration happened to seed them — and a
/// site with no language cannot have content filed under one. Named in the
/// environment rather than in the panel because it is a decision about the
/// server, the same as who may open an agency on it.
///
/// The first is the site's own. Unset, a site gets whatever setup gave it.
pub fn starting_languages() -> Vec<String> {
    named_languages(&std::env::var("MAVICMS_SITE_LANGUAGES").unwrap_or_default())
}

fn named_languages(raw: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for code in raw.split(',').map(normalize_code) {
        if code.is_empty() || validate_code(&code).is_err() || out.contains(&code) {
            continue;
        }
        out.push(code);
    }
    out
}

/// Gives a site the languages it should have, and exactly one default.
///
/// Runs when a site's schema is opened, which is the only moment that happens
/// for every site however it was created. Adds nothing to a site that already
/// has languages beyond repairing a missing default.
pub async fn ensure_site_languages(db: &impl ConnectionTrait) -> AppResult<()> {
    give_languages(db, &starting_languages()).await
}

/// The above, with the list handed in rather than read from the environment.
async fn give_languages(db: &impl ConnectionTrait, wanted: &[String]) -> AppResult<()> {
    let existing = all(db).await?;
    let held: Vec<&str> = existing.iter().map(|row| row.code.as_str()).collect();
    // Behind whatever is already there. Sharing a number with an existing row
    // leaves the order to break a tie, and it broke it alphabetically: a site
    // whose language was Turkish was handed English as its own.
    let after = existing
        .iter()
        .map(|row| row.sort_order)
        .max()
        .map(|highest| highest + 1)
        .unwrap_or(0);

    for (position, code) in wanted.iter().enumerate() {
        if held.contains(&code.as_str()) {
            continue;
        }
        let (name, native_name) = well_known_name(code);
        language::ActiveModel {
            code: Set(code.clone()),
            name: Set(name.to_string()),
            native_name: Set(native_name.to_string()),
            direction: Set(direction_of(code).to_string()),
            // The first named is the site's own, but only when the site has
            // nothing yet: a site already using one is not re-pointed by an
            // environment variable changing.
            is_default: Set(position == 0 && existing.is_empty()),
            is_active: Set(true),
            sort_order: Set(after + position as i32),
            created_at: Set(Utc::now().fixed_offset()),
        }
        .insert(db)
        .await?;
    }

    ensure_a_default(db).await
}

/// Adds the first language during first-run setup. The migration seeds this
/// for existing installs, but on a fresh database it runs before
/// `site_settings` exists, so setup has to do it.
pub async fn ensure_seeded(db: &impl ConnectionTrait, locale: &str) -> AppResult<()> {
    if language::Entity::find().one(db).await?.is_some() {
        return Ok(());
    }

    let code = normalize_code(locale);
    let code = if code.is_empty() {
        "en".to_string()
    } else {
        code
    };
    let (name, native_name) = well_known_name(&code);

    language::ActiveModel {
        code: Set(code),
        name: Set(name.to_string()),
        native_name: Set(native_name.to_string()),
        direction: Set("ltr".to_string()),
        is_default: Set(true),
        is_active: Set(true),
        sort_order: Set(0),
        created_at: Set(Utc::now().fixed_offset()),
    }
    .insert(db)
    .await?;

    Ok(())
}

/// Names for the languages the panel itself ships in, so first-run setup
/// doesn't leave a row labelled "Language". Anything else is named by the admin.
/// Which way a language is written. Arabic, Hebrew, Persian and Urdu are the
/// ones a Turkish agency is realistically going to add; the rest of the
/// right-to-left scripts can be set in the panel.
pub fn direction_of(code: &str) -> &'static str {
    match code.split('-').next().unwrap_or("") {
        "ar" | "he" | "fa" | "ur" => "rtl",
        _ => "ltr",
    }
}

pub fn well_known_name(code: &str) -> (&'static str, &'static str) {
    match code.split('-').next().unwrap_or("") {
        "tr" => ("Turkish", "Türkçe"),
        "en" => ("English", "English"),
        "de" => ("German", "Deutsch"),
        "fr" => ("French", "Français"),
        "es" => ("Spanish", "Español"),
        "ar" => ("Arabic", "العربية"),
        _ => ("Language", "Language"),
    }
}

#[cfg(test)]
mod tests {
    use super::{default_code, ensure_a_default, named_languages};
    use crate::entities::language;
    use chrono::Utc;
    use migration::{Migrator, MigratorTrait};
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, Database, DatabaseConnection, EntityTrait};

    async fn site() -> DatabaseConnection {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        Migrator::up(&db, None).await.unwrap();
        language::Entity::delete_many().exec(&db).await.unwrap();
        db
    }

    async fn add(db: &DatabaseConnection, code: &str, default: bool, order: i32) {
        language::ActiveModel {
            code: Set(code.to_string()),
            name: Set(code.to_string()),
            native_name: Set(code.to_string()),
            direction: Set("ltr".to_string()),
            is_default: Set(default),
            is_active: Set(true),
            sort_order: Set(order),
            created_at: Set(Utc::now().fixed_offset()),
        }
        .insert(db)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn a_site_with_no_default_writes_in_a_language_it_has() {
        // It used to answer "en" here, and a Turkish site filed Turkish posts
        // under a language it had never added — where nothing looking at a
        // language's content would find them.
        let db = site().await;
        add(&db, "tr", false, 0).await;
        assert_eq!(default_code(&db).await.unwrap(), "tr");
    }

    #[tokio::test]
    async fn the_marked_default_still_wins() {
        let db = site().await;
        add(&db, "de", false, 0).await;
        add(&db, "tr", true, 1).await;
        assert_eq!(default_code(&db).await.unwrap(), "tr");
    }

    #[tokio::test]
    async fn only_a_site_with_no_languages_at_all_falls_back_to_english() {
        let db = site().await;
        assert_eq!(default_code(&db).await.unwrap(), "en");
    }

    #[tokio::test]
    async fn a_missing_default_is_repaired_to_the_one_the_site_started_with() {
        // Not the first in the list — the first in time. A site's own language
        // is the one it has had all along, and reordering the list in the panel
        // is not a decision about which language its content is in.
        let db = site().await;
        add(&db, "tr", false, 5).await;
        add(&db, "en", false, 0).await;
        ensure_a_default(&db).await.unwrap();

        let marked: Vec<String> = super::all(&db)
            .await
            .unwrap()
            .into_iter()
            .filter(|row| row.is_default)
            .map(|row| row.code)
            .collect();
        assert_eq!(marked, vec!["tr".to_string()]);
    }

    #[tokio::test]
    async fn a_new_site_gets_the_languages_the_server_names() {
        let db = site().await;
        super::give_languages(&db, &named_languages("tr, en ,ar"))
            .await
            .unwrap();

        let held = super::all(&db).await.unwrap();
        assert_eq!(
            held.iter().map(|r| r.code.as_str()).collect::<Vec<_>>(),
            vec!["tr", "en", "ar"]
        );
        // The first named is the site's own, and Arabic is written the other
        // way whether or not anybody remembered to say so.
        assert!(held[0].is_default);
        assert_eq!(held[2].direction, "rtl");
    }

    #[tokio::test]
    async fn a_site_already_using_a_language_is_not_re_pointed() {
        // Changing the environment variable must not move a working site's
        // own language out from under its content.
        let db = site().await;
        add(&db, "de", true, 0).await;
        super::give_languages(&db, &named_languages("tr,en"))
            .await
            .unwrap();

        assert_eq!(default_code(&db).await.unwrap(), "de");
    }

    #[tokio::test]
    async fn a_language_added_later_does_not_become_the_site_s_own() {
        // The site is Turkish and has been all along. Naming a starting set
        // that happens to include English must add English, not hand it the
        // site — which is what a shared sort order and an alphabetical
        // tie-break did.
        let db = site().await;
        add(&db, "tr", false, 1).await;
        super::give_languages(&db, &named_languages("tr,en"))
            .await
            .unwrap();

        assert_eq!(default_code(&db).await.unwrap(), "tr");
        let held = super::all(&db).await.unwrap();
        assert_eq!(held.len(), 2);
        // And the one added is behind the one that was there.
        let english = held.iter().find(|r| r.code == "en").unwrap();
        let turkish = held.iter().find(|r| r.code == "tr").unwrap();
        assert!(english.sort_order > turkish.sort_order);
    }

    #[test]
    fn the_named_languages_are_cleaned_and_kept_in_order() {
        assert_eq!(named_languages(" TR , en ,, tr "), vec!["tr", "en"]);
        assert_eq!(named_languages("pt_br"), vec!["pt-BR"]);
        assert!(named_languages("").is_empty());
        assert!(named_languages("!!!").is_empty());
    }
}

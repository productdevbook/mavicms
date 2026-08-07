//! What one site can be asked to do.
//!
//! Every tool here is the panel's own code path, reached differently. Nothing
//! reimplements a query the panel already answers — where the answer took more
//! than a line to work out, the function it came from is the one the endpoint
//! calls too, so the two cannot drift apart and quietly disagree about which
//! posts are published.
//!
//! There is nothing here that deletes. An assistant that misreads an
//! instruction and writes a bad paragraph has done something a person can
//! read and undo; one that misreads it and removes an archive has not.

use sea_orm::{
    ColumnTrait, EntityTrait, Order, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect,
};
use serde_json::{Value, json};
use uuid::Uuid;

use super::Tool;
use crate::{
    dto::{
        post::{CreatePostRequest, UpdatePostRequest},
        taxonomy::LocaleQuery,
    },
    entities::{category, form, form_submission, media, post, tag},
    error::{AppError, AppResult},
    state::AppState,
    tenants::{Hosting, Resolved},
};

/// The most rows any one call will hand back.
const MAX_ROWS: u64 = 100;

pub const TOOLS: &[Tool] = &[
    Tool {
        name: "site_overview",
        title: "How this site stands",
        description: "What is on this site and what state it is in: how many posts at each \
            status, which languages it writes in, which forms are taking answers, and how the \
            last builds went. Start here when you do not yet know the site.",
        writes: false,
        schema: || json!({ "type": "object", "additionalProperties": false }),
    },
    Tool {
        name: "posts_search",
        title: "Find posts",
        description: "Posts, newest first. Every language and every status unless you narrow \
            it. Bodies are left out — this is for finding the post you want; posts_get returns \
            one in full.",
        writes: false,
        schema: || {
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "q": { "type": "string", "description": "Free text across titles, summaries and bodies" },
                    "locale": { "type": "string", "description": "Language code, or several separated by commas" },
                    "status": {
                        "type": "string",
                        "description": "draft, review, scheduled or published; several separated by commas"
                    },
                    "limit": { "type": "integer", "description": "Up to 100. Default 20." },
                    "offset": { "type": "integer" }
                }
            })
        },
    },
    Tool {
        name: "posts_get",
        title: "Read one post",
        description: "One post in full, including its Markdown body and the other languages it \
            exists in. Give the id, or the address it answers on.",
        writes: false,
        schema: || {
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "id": { "type": "string", "description": "The post's id" },
                    "slug": { "type": "string", "description": "Its address, if you do not have the id" },
                    "locale": { "type": "string", "description": "Which language's copy, when using slug" }
                }
            })
        },
    },
    Tool {
        name: "posts_create",
        title: "Write a post",
        description: "Add a post. It is a draft unless you say otherwise, which is usually what \
            you want — somebody should read it before it is online. Give content_markdown; the \
            HTML is rendered from it. A scheduled post needs publish_at.",
        writes: true,
        schema: || {
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["title", "content_markdown"],
                "properties": {
                    "title": { "type": "string" },
                    "slug": { "type": "string", "description": "The address. Made from the title if left out." },
                    "content_markdown": { "type": "string" },
                    "excerpt": { "type": "string", "description": "One paragraph, for cards and search results" },
                    "status": {
                        "type": "string",
                        "enum": ["draft", "review", "scheduled", "published"],
                        "description": "Defaults to draft"
                    },
                    "publish_at": { "type": "string", "description": "RFC 3339. Required when scheduled." },
                    "locale": { "type": "string", "description": "Defaults to the site's own language" },
                    "author": { "type": "string" },
                    "tags": { "type": "array", "items": { "type": "string" } },
                    "seo_description": { "type": "string" },
                    "translation_of": { "type": "string", "description": "Id of the post this translates" }
                }
            })
        },
    },
    Tool {
        name: "posts_update",
        title: "Change a post",
        description: "Change one post. Only what you send is changed; everything else is left \
            as it was. Read the post first — sending content_markdown replaces the whole body, \
            so a correction to one paragraph means sending the rest of them back unaltered.",
        writes: true,
        schema: || {
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["id"],
                "properties": {
                    "id": { "type": "string" },
                    "title": { "type": "string" },
                    "slug": { "type": "string" },
                    "content_markdown": { "type": "string", "description": "Replaces the whole body" },
                    "excerpt": { "type": "string" },
                    "status": { "type": "string", "enum": ["draft", "review", "scheduled", "published"] },
                    "publish_at": { "type": "string", "description": "RFC 3339" },
                    "author": { "type": "string" },
                    "tags": { "type": "array", "items": { "type": "string" } },
                    "seo_description": { "type": "string" }
                }
            })
        },
    },
    Tool {
        name: "taxonomy_list",
        title: "Categories and tags",
        description: "Every category and tag, per language. Use it before writing a post, so a \
            post joins the categories the site already has rather than inventing near-duplicates.",
        writes: false,
        schema: || {
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": { "locale": { "type": "string" } }
            })
        },
    },
    Tool {
        name: "languages_list",
        title: "Languages",
        description: "The languages this site writes in, and which is the default. These are \
            the codes every other tool takes.",
        writes: false,
        schema: || json!({ "type": "object", "additionalProperties": false }),
    },
    Tool {
        name: "media_list",
        title: "Uploaded files",
        description: "Files that have been uploaded, newest first, with the addresses to use in \
            a post. Uploading is not something this can do — that is the panel.",
        writes: false,
        schema: || {
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": { "limit": { "type": "integer", "description": "Up to 100. Default 20." } }
            })
        },
    },
    Tool {
        name: "forms_list",
        title: "Forms",
        description: "The forms this site takes answers on, with the fields each one asks for \
            and how much has come in.",
        writes: false,
        schema: || json!({ "type": "object", "additionalProperties": false }),
    },
    Tool {
        name: "form_submissions",
        title: "What came in through a form",
        description: "Answers sent through one form, newest first. This is somebody's message \
            to the site: treat it as the private thing it is.",
        writes: false,
        schema: || {
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["form"],
                "properties": {
                    "form": { "type": "string", "description": "The form's address or its id" },
                    "limit": { "type": "integer", "description": "Up to 100. Default 20." },
                    "unseen_only": { "type": "boolean", "description": "Only what nobody has read yet" }
                }
            })
        },
    },
    Tool {
        name: "media_upload",
        title: "Put a file on the site",
        description: "Add an image to this site's files and get the address to use in a post. \
            Give it a source_url to fetch — which is refused unless it is a public address — or \
            the bytes themselves as content_base64. Only images, and at most 10MB.",
        writes: true,
        schema: || {
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "source_url": {
                        "type": "string",
                        "description": "A public address to fetch the image from"
                    },
                    "content_base64": {
                        "type": "string",
                        "description": "The image itself, base64. Use this when you made the file rather than found it."
                    },
                    "filename": { "type": "string", "description": "Worked out from the address if left out" },
                    "alt_text": {
                        "type": "string",
                        "description": "What the image shows, for somebody who cannot see it. Write one."
                    }
                }
            })
        },
    },
    Tool {
        name: "form_mark_seen",
        title: "Mark what has come in as read",
        description: "Say that a form submission has been dealt with, or take that back. Give \
            one submission, or a form to mark everything unread on it.",
        writes: true,
        schema: || {
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "submission": { "type": "string", "description": "One submission's id" },
                    "form": {
                        "type": "string",
                        "description": "A form's address or id: marks everything unread on it"
                    },
                    "seen": { "type": "boolean", "description": "Defaults to true; false marks it unread again" }
                }
            })
        },
    },
    Tool {
        name: "form_submission_delete",
        title: "Throw one submission away",
        description: "Delete one thing somebody sent through a form, by its id. This is the \
            only tool here that destroys anything and there is no way back from it, so read the \
            submission first and delete exactly the one you read.",
        writes: true,
        schema: || {
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["submission"],
                "properties": { "submission": { "type": "string", "description": "The submission's id" } }
            })
        },
    },
    Tool {
        name: "publish_site",
        title: "Build the site's pages",
        description: "Ask this site to build its pages again, which is what puts written work \
            in front of readers. A build already waiting is returned rather than a second one \
            queued.",
        writes: true,
        schema: || json!({ "type": "object", "additionalProperties": false }),
    },
];

pub async fn call(
    hosting: &Hosting,
    resolved: &Resolved,
    state: &AppState,
    name: &str,
    arguments: &Value,
) -> AppResult<Value> {
    let db = state.db_or_unavailable()?;

    match name {
        "site_overview" => overview(hosting, resolved, state).await,
        "posts_search" => search(db, arguments).await,
        "posts_get" => one_post(db, arguments).await,
        "posts_create" => write_post(db, arguments).await,
        "posts_update" => change_post(db, arguments).await,
        "taxonomy_list" => taxonomy(db, arguments).await,
        "languages_list" => languages(db).await,
        "media_list" => files(db, arguments).await,
        "forms_list" => forms(db).await,
        "form_submissions" => submissions(db, arguments).await,
        "media_upload" => upload(state, arguments).await,
        "form_mark_seen" => mark_seen(db, arguments).await,
        "form_submission_delete" => throw_away(db, arguments).await,
        "publish_site" => publish(hosting, resolved).await,
        other => Err(AppError::NotFound(format!("tool {other}"))),
    }
}

/// A post, as JSON. Serialising one cannot fail; saying so out loud beats a
/// conversion nobody would read.
fn described(post: crate::dto::post::PostResponse) -> AppResult<Value> {
    serde_json::to_value(post)
        .map_err(|err| AppError::Internal(format!("could not describe the post: {err}")))
}

/// A number the caller asked for, held to what one answer may carry.
fn rows(arguments: &Value, default: u64) -> u64 {
    arguments
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(default)
        .clamp(1, MAX_ROWS)
}

fn text(arguments: &Value, key: &str) -> Option<String> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

async fn overview(hosting: &Hosting, resolved: &Resolved, state: &AppState) -> AppResult<Value> {
    let db = state.db_or_unavailable()?;

    let counted = crate::routes::posts::page(
        db,
        &LocaleQuery {
            limit: Some(1),
            ..Default::default()
        },
    )
    .await?;

    let languages = crate::languages::all(db).await?;
    let forms = form::Entity::find().all(db).await?;

    // A site that has no agency and no repository is read through the API and
    // built elsewhere; saying "no builds" would read as a failure rather than
    // as an arrangement.
    let builds = match publishing(hosting, resolved) {
        Some((control, tenant)) => {
            let recent = crate::publish::latest(control, tenant, 5).await?;
            json!(
                recent
                    .iter()
                    .map(|build| json!({
                        "status": build.status,
                        "requested_at": build.requested_at,
                        "finished_at": build.finished_at,
                    }))
                    .collect::<Vec<_>>()
            )
        }
        None => Value::Null,
    };

    Ok(json!({
        "posts": counted.counts,
        "total_posts": counted.total,
        "languages": languages
            .iter()
            .map(|language| json!({ "code": language.code, "default": language.is_default }))
            .collect::<Vec<_>>(),
        "forms": forms
            .iter()
            .filter(|form| form.active)
            .map(|form| json!({ "slug": form.slug, "name": form.name }))
            .collect::<Vec<_>>(),
        "recent_builds": builds,
    }))
}

/// The control database and tenant id, when this site is one the server hosts
/// and so has pages that can be built.
///
/// The server's own installation is the panel rather than a site, and has
/// nowhere to publish to.
fn publishing<'a>(
    hosting: &'a Hosting,
    resolved: &Resolved,
) -> Option<(&'a sea_orm::DatabaseConnection, Uuid)> {
    let Resolved::Tenant(tenant) = resolved else {
        return None;
    };
    Some((hosting.registry.as_deref()?.control(), tenant.id))
}

async fn search(db: &sea_orm::DatabaseConnection, arguments: &Value) -> AppResult<Value> {
    let query = LocaleQuery {
        locale: text(arguments, "locale"),
        slug: None,
        limit: Some(rows(arguments, 20)),
        offset: arguments.get("offset").and_then(Value::as_u64),
        include: None,
        q: text(arguments, "q"),
        status: text(arguments, "status"),
    };

    let found = crate::routes::posts::page(db, &query).await?;
    Ok(json!({
        "total": found.total,
        "posts": found
            .items
            .iter()
            .map(|post| json!({
                "id": post.id,
                "title": post.title,
                "slug": post.slug,
                "status": post.status,
                "locale": post.locale,
                "excerpt": post.excerpt,
                "tags": post.tags,
                "updated_at": post.updated_at,
            }))
            .collect::<Vec<_>>(),
    }))
}

async fn one_post(db: &sea_orm::DatabaseConnection, arguments: &Value) -> AppResult<Value> {
    let found = match text(arguments, "id") {
        Some(id) => {
            let id = Uuid::parse_str(&id)
                .map_err(|_| AppError::Validation(format!("{id} is not a post id")))?;
            post::Entity::find_by_id(id).one(db).await?
        }
        None => {
            let slug = text(arguments, "slug").ok_or_else(|| {
                AppError::Validation(
                    "say which post: an id, or the address it answers on".to_string(),
                )
            })?;
            let mut find = post::Entity::find().filter(post::Column::Slug.eq(&slug));
            if let Some(locale) = text(arguments, "locale") {
                find = find.filter(post::Column::Locale.eq(locale));
            }
            find.one(db).await?
        }
    };

    let found = found.ok_or_else(|| AppError::NotFound("that post".to_string()))?;
    described(crate::routes::posts::detail(db, found).await?)
}

async fn write_post(db: &sea_orm::DatabaseConnection, arguments: &Value) -> AppResult<Value> {
    let title = text(arguments, "title")
        .ok_or_else(|| AppError::Validation("a post needs a title".to_string()))?;
    let markdown = arguments
        .get("content_markdown")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    let slug = match text(arguments, "slug") {
        Some(slug) => slug,
        None => crate::slug::slugify(&title),
    };

    let payload = CreatePostRequest {
        title,
        slug,
        excerpt: arguments
            .get("excerpt")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        status: status_of(arguments)?.unwrap_or_default(),
        publish_at: publish_at(arguments)?,
        author: arguments
            .get("author")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        category: String::new(),
        category_ids: Vec::new(),
        tags: tags(arguments)?.unwrap_or_default(),
        cover_url: String::new(),
        seo_title: String::new(),
        seo_description: arguments
            .get("seo_description")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        canonical: String::new(),
        featured: false,
        allow_comments: true,
        content_html: crate::markdown::to_html(&markdown),
        content_markdown: Some(markdown),
        locale: text(arguments, "locale"),
        translation_of: match text(arguments, "translation_of") {
            Some(id) => Some(
                Uuid::parse_str(&id)
                    .map_err(|_| AppError::Validation(format!("{id} is not a post id")))?,
            ),
            None => None,
        },
        created_at: None,
    };

    described(crate::routes::posts::create(db, payload).await?)
}

async fn change_post(db: &sea_orm::DatabaseConnection, arguments: &Value) -> AppResult<Value> {
    let id = text(arguments, "id")
        .ok_or_else(|| AppError::Validation("say which post to change".to_string()))?;
    let id =
        Uuid::parse_str(&id).map_err(|_| AppError::Validation(format!("{id} is not a post id")))?;

    let markdown = arguments
        .get("content_markdown")
        .and_then(Value::as_str)
        .map(str::to_string);

    let payload = UpdatePostRequest {
        title: text(arguments, "title"),
        slug: text(arguments, "slug"),
        excerpt: arguments
            .get("excerpt")
            .and_then(Value::as_str)
            .map(str::to_string),
        status: status_of(arguments)?,
        // The outer Option is "was it mentioned", the inner is "is it being
        // cleared". Only the first is offered here: clearing a date is not
        // something an assistant should be able to do by omission.
        publish_at: publish_at(arguments)?.map(Some),
        author: text(arguments, "author"),
        category: None,
        category_ids: None,
        tags: tags(arguments)?,
        cover_url: None,
        seo_title: None,
        seo_description: arguments
            .get("seo_description")
            .and_then(Value::as_str)
            .map(str::to_string),
        canonical: None,
        featured: None,
        allow_comments: None,
        // Left to the endpoint, which renders the Markdown when no HTML
        // comes with it — one rule for every way in.
        content_html: None,
        content_markdown: markdown,
    };

    described(crate::routes::posts::update(db, id, payload).await?)
}

fn status_of(arguments: &Value) -> AppResult<Option<crate::dto::post::PostStatus>> {
    use crate::dto::post::PostStatus;

    let Some(named) = text(arguments, "status") else {
        return Ok(None);
    };
    match named.as_str() {
        "draft" => Ok(Some(PostStatus::Draft)),
        "review" => Ok(Some(PostStatus::Review)),
        "scheduled" => Ok(Some(PostStatus::Scheduled)),
        "published" => Ok(Some(PostStatus::Published)),
        other => Err(AppError::Validation(format!(
            "{other} is not a status: draft, review, scheduled or published"
        ))),
    }
}

fn publish_at(arguments: &Value) -> AppResult<Option<chrono::DateTime<chrono::Utc>>> {
    let Some(written) = text(arguments, "publish_at") else {
        return Ok(None);
    };
    chrono::DateTime::parse_from_rfc3339(&written)
        .map(|at| Some(at.with_timezone(&chrono::Utc)))
        .map_err(|_| {
            AppError::Validation(format!(
                "{written} is not a time: write it as 2026-08-09T09:00:00Z"
            ))
        })
}

/// The tags asked for: absent, or a list of them.
///
/// Something that is not a list is refused rather than read as an empty one.
/// Tags are replaced wholesale by a write, so a misunderstanding about the
/// shape of this field would silently take every tag off the post.
fn tags(arguments: &Value) -> AppResult<Option<Vec<String>>> {
    let Some(given) = arguments.get("tags") else {
        return Ok(None);
    };

    let Some(values) = given.as_array() else {
        return Err(AppError::Validation(
            "tags is a list of words, such as [\"one\", \"two\"]".to_string(),
        ));
    };

    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| AppError::Validation("every tag is a word".to_string()))
        })
        .collect::<AppResult<Vec<_>>>()
        .map(Some)
}

async fn taxonomy(db: &sea_orm::DatabaseConnection, arguments: &Value) -> AppResult<Value> {
    let locale = text(arguments, "locale");

    let mut categories = category::Entity::find();
    let mut tags = tag::Entity::find();
    if let Some(locale) = &locale {
        categories = categories.filter(category::Column::Locale.eq(locale));
        tags = tags.filter(tag::Column::Locale.eq(locale));
    }

    Ok(json!({
        "categories": categories
            .all(db)
            .await?
            .iter()
            .map(|row| json!({ "id": row.id, "name": row.name, "slug": row.slug, "locale": row.locale }))
            .collect::<Vec<_>>(),
        "tags": tags
            .all(db)
            .await?
            .iter()
            .map(|row| json!({ "id": row.id, "name": row.name, "slug": row.slug, "locale": row.locale }))
            .collect::<Vec<_>>(),
    }))
}

async fn languages(db: &sea_orm::DatabaseConnection) -> AppResult<Value> {
    Ok(json!(
        crate::languages::all(db)
            .await?
            .iter()
            .map(|language| json!({
                "code": language.code,
                "name": language.name,
                "default": language.is_default,
                "active": language.is_active,
            }))
            .collect::<Vec<_>>()
    ))
}

async fn files(db: &sea_orm::DatabaseConnection, arguments: &Value) -> AppResult<Value> {
    Ok(json!(
        media::Entity::find()
            .order_by(media::Column::UploadedAt, Order::Desc)
            .limit(rows(arguments, 20))
            .all(db)
            .await?
            .iter()
            .map(|file| json!({
                "id": file.id,
                "filename": file.filename,
                "url": file.url_path,
                "mime_type": file.mime_type,
                "size_bytes": file.size_bytes,
                "alt_text": file.alt_text,
            }))
            .collect::<Vec<_>>()
    ))
}

async fn forms(db: &sea_orm::DatabaseConnection) -> AppResult<Value> {
    let mut described = Vec::new();
    for row in form::Entity::find().all(db).await? {
        let waiting = form_submission::Entity::find()
            .filter(form_submission::Column::FormId.eq(row.id))
            .filter(form_submission::Column::Seen.eq(false))
            .count(db)
            .await?;

        described.push(json!({
            "id": row.id,
            "slug": row.slug,
            "name": row.name,
            "taking_answers": row.active,
            "fields": serde_json::from_str::<Value>(&row.fields).unwrap_or(Value::Null),
            "unread": waiting,
        }));
    }
    Ok(json!(described))
}

/// The form meant by an address or an id.
async fn form_named(db: &sea_orm::DatabaseConnection, named: &str) -> AppResult<form::Model> {
    let found = match Uuid::parse_str(named) {
        Ok(id) => form::Entity::find_by_id(id).one(db).await?,
        Err(_) => {
            form::Entity::find()
                .filter(form::Column::Slug.eq(named))
                .one(db)
                .await?
        }
    };
    found.ok_or_else(|| AppError::NotFound(format!("form {named}")))
}

async fn submissions(db: &sea_orm::DatabaseConnection, arguments: &Value) -> AppResult<Value> {
    let named = text(arguments, "form")
        .ok_or_else(|| AppError::Validation("say which form".to_string()))?;

    let found = form_named(db, &named).await?;

    let mut find = form_submission::Entity::find()
        .filter(form_submission::Column::FormId.eq(found.id))
        .order_by(form_submission::Column::CreatedAt, Order::Desc);
    if arguments
        .get("unseen_only")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        find = find.filter(form_submission::Column::Seen.eq(false));
    }

    Ok(json!(
        find.limit(rows(arguments, 20))
            .all(db)
            .await?
            .iter()
            .map(|row| json!({
                "id": row.id,
                "seen": row.seen,
                "created_at": row.created_at.to_rfc3339(),
                "answers": serde_json::from_str::<Value>(&row.data)
                    .unwrap_or(Value::String(row.data.clone())),
            }))
            .collect::<Vec<_>>()
    ))
}

async fn upload(state: &AppState, arguments: &Value) -> AppResult<Value> {
    let alt_text = text(arguments, "alt_text");
    let filename = text(arguments, "filename");

    let saved = match (
        text(arguments, "source_url"),
        text(arguments, "content_base64"),
    ) {
        (Some(_), Some(_)) => {
            return Err(AppError::Validation(
                "give the address to fetch or the bytes, not both".to_string(),
            ));
        }
        (Some(url), None) => {
            crate::routes::media::fetch_and_store(state, &url, filename, alt_text).await?
        }
        (None, Some(encoded)) => {
            use base64::Engine as _;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(encoded.trim())
                .map_err(|err| {
                    AppError::Validation(format!("content_base64 is not base64: {err}"))
                })?;

            crate::routes::media::store_image(
                state,
                &bytes,
                filename.unwrap_or_else(|| "upload".to_string()),
                alt_text.unwrap_or_default(),
            )
            .await?
        }
        (None, None) => {
            return Err(AppError::Validation(
                "give a source_url to fetch, or the file as content_base64".to_string(),
            ));
        }
    };

    Ok(json!({
        "id": saved.id,
        "filename": saved.filename,
        // What to put in a post. Relative when the site keeps its own files.
        "url": saved.url_path,
        "mime_type": saved.mime_type,
        "size_bytes": saved.size_bytes,
    }))
}

async fn mark_seen(db: &sea_orm::DatabaseConnection, arguments: &Value) -> AppResult<Value> {
    use sea_orm::sea_query::Expr;

    let seen = arguments
        .get("seen")
        .and_then(Value::as_bool)
        .unwrap_or(true);

    let mut change = form_submission::Entity::update_many()
        .col_expr(form_submission::Column::Seen, Expr::value(seen));

    match (text(arguments, "submission"), text(arguments, "form")) {
        (Some(id), _) => {
            let id = Uuid::parse_str(&id)
                .map_err(|_| AppError::Validation(format!("{id} is not a submission id")))?;
            change = change.filter(form_submission::Column::Id.eq(id));
        }
        (None, Some(named)) => {
            let form = form_named(db, &named).await?;
            change = change
                .filter(form_submission::Column::FormId.eq(form.id))
                .filter(form_submission::Column::Seen.eq(!seen));
        }
        (None, None) => {
            return Err(AppError::Validation(
                "say which submission, or which form".to_string(),
            ));
        }
    }

    let result = change.exec(db).await?;
    if result.rows_affected == 0 {
        return Err(AppError::NotFound(
            "nothing there to mark; it may already be that way".to_string(),
        ));
    }

    Ok(json!({ "marked": result.rows_affected, "seen": seen }))
}

async fn throw_away(db: &sea_orm::DatabaseConnection, arguments: &Value) -> AppResult<Value> {
    let id = text(arguments, "submission")
        .ok_or_else(|| AppError::Validation("say which submission".to_string()))?;
    let id = Uuid::parse_str(&id)
        .map_err(|_| AppError::Validation(format!("{id} is not a submission id")))?;

    let result = form_submission::Entity::delete_many()
        .filter(form_submission::Column::Id.eq(id))
        .exec(db)
        .await?;

    if result.rows_affected == 0 {
        return Err(AppError::NotFound(format!("submission {id}")));
    }
    Ok(json!({ "deleted": id }))
}

async fn publish(hosting: &Hosting, resolved: &Resolved) -> AppResult<Value> {
    let (control, tenant) = publishing(hosting, resolved).ok_or_else(|| {
        AppError::Validation(
            "this installation has no pages of its own to build; publishing belongs to a hosted site"
                .to_string(),
        )
    })?;

    let build = crate::publish::request(control, tenant).await?;
    Ok(json!({ "status": build.status, "requested_at": build.requested_at }))
}

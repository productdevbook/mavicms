//! What a site publishes, beyond posts.
//!
//! A training company wants a course to have a price, a length and a level. A
//! letting agent wants rooms and a floor. Neither is a blog post with the
//! numbers typed into the paragraph, and neither is worth a feature of its own
//! — so a site says what its own kinds of thing are, and what each is made of.
//!
//! Nothing here is new machinery. A post already has a `kind`; a form already
//! knows how to describe a set of fields and check something against them.
//! This is those two introduced to each other, which is why a course arrives
//! with translations, SEO, scheduling, a digest and an editor already working.

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, Order,
    QueryFilter, QueryOrder,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    dto::forms::{FormField, clean_fields, clean_submission},
    entities::{content_type, post},
    error::{AppError, AppResult},
};

/// The kinds every site has, whatever else it adds.
pub const POST: &str = "post";
pub const PAGE: &str = "page";

/// More than this and the panel's sidebar is a list of lists.
const MAX_TYPES: usize = 30;

#[derive(Debug, Serialize, ToSchema)]
pub struct ContentTypeResponse {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub plural: String,
    pub fields: Vec<FormField>,
    /// `post` and `page`: their fields can change, they themselves cannot go.
    pub built_in: bool,
    /// How many pieces of content are of this kind.
    pub count: u64,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SaveContentTypeRequest {
    pub name: String,
    /// What a list of them is called. The name, if left out.
    #[serde(default)]
    pub plural: Option<String>,
    /// Left out on creation, made from the name. Never changed afterwards:
    /// it is in the addresses a front end already fetches.
    #[serde(default)]
    pub slug: Option<String>,
    /// What one of these is made of, beyond a title and a body. May be none.
    #[serde(default)]
    pub fields: Vec<FormField>,
}

/// Every kind this site publishes, in the order the panel shows them.
pub async fn all(db: &DatabaseConnection) -> AppResult<Vec<content_type::Model>> {
    Ok(content_type::Entity::find()
        .order_by(content_type::Column::SortOrder, Order::Asc)
        .order_by(content_type::Column::Slug, Order::Asc)
        .all(db)
        .await?)
}

/// The same, with the fields parsed and the content counted.
pub async fn described(db: &DatabaseConnection) -> AppResult<Vec<ContentTypeResponse>> {
    use sea_orm::PaginatorTrait;

    let mut described = Vec::new();
    for kind in all(db).await? {
        let count = post::Entity::find()
            .filter(post::Column::Kind.eq(&kind.slug))
            .count(db)
            .await?;

        described.push(ContentTypeResponse {
            id: kind.id,
            slug: kind.slug,
            name: kind.name,
            plural: kind.plural,
            fields: fields_of(&kind.fields)?,
            built_in: kind.built_in,
            count,
        });
    }
    Ok(described)
}

pub async fn by_slug(db: &DatabaseConnection, slug: &str) -> AppResult<content_type::Model> {
    content_type::Entity::find()
        .filter(content_type::Column::Slug.eq(slug))
        .one(db)
        .await?
        .ok_or_else(|| AppError::Validation(format!("this site has no kind called \"{slug}\"")))
}

/// The names asked for, each one checked against what this site actually has.
///
/// A kind that does not exist is refused rather than ignored: a listing that
/// quietly answered with posts because "cours" was misspelt would look like a
/// site with nothing on it.
pub async fn known(db: &DatabaseConnection, asked: &[String]) -> AppResult<Vec<String>> {
    let held = all(db).await?;
    let mut wanted = Vec::with_capacity(asked.len());

    for name in asked {
        if !held.iter().any(|kind| &kind.slug == name) {
            let names: Vec<&str> = held.iter().map(|kind| kind.slug.as_str()).collect();
            return Err(AppError::Validation(format!(
                "this site has no kind called \"{name}\": {}",
                names.join(", ")
            )));
        }
        if !wanted.contains(name) {
            wanted.push(name.clone());
        }
    }
    Ok(wanted)
}

pub fn fields_of(stored: &str) -> AppResult<Vec<FormField>> {
    serde_json::from_str(stored)
        .map_err(|err| AppError::Internal(format!("could not read the fields: {err}")))
}

/// What a piece of content carries for its kind's fields, checked against them.
///
/// The same function a form's answers go through, for the same reason: a value
/// that does not fit the field it is in is a broken page later, and the person
/// writing it is here now.
///
/// `finished` is the one difference between content and a form. A form is
/// submitted once and completely, so a missing required answer is a refusal.
/// A draft is by definition half-written: the price is missing because nobody
/// has decided it yet. So a required field is required when the thing goes
/// out — published, or scheduled to be — and merely empty until then. A type
/// that does not fit its field is refused either way.
pub fn checked_values(
    fields: &[FormField],
    given: Option<Value>,
    finished: bool,
) -> AppResult<String> {
    let given = match given {
        None | Some(Value::Null) => Map::new(),
        Some(Value::Object(map)) => map,
        Some(_) => {
            return Err(AppError::Validation(
                "fields is an object of field names and values".to_string(),
            ));
        }
    };

    if fields.is_empty() {
        if given.is_empty() {
            return Ok("{}".to_string());
        }
        return Err(AppError::Validation(
            "this kind has no fields of its own".to_string(),
        ));
    }

    let expected: Vec<FormField> = if finished {
        fields.to_vec()
    } else {
        fields
            .iter()
            .map(|field| FormField {
                required: false,
                ..field.clone()
            })
            .collect()
    };

    let cleaned = clean_submission(&expected, given)?;
    serde_json::to_string(&cleaned)
        .map_err(|err| AppError::Internal(format!("could not store the fields: {err}")))
}

/// Whether a status means the thing is out in the world, or on its way.
pub fn is_finished(status: &crate::dto::post::PostStatus) -> bool {
    use crate::dto::post::PostStatus;
    matches!(status, PostStatus::Published | PostStatus::Scheduled)
}

/// The values as they were stored, for handing back.
pub fn read_values(stored: &str) -> Value {
    serde_json::from_str(stored).unwrap_or_else(|_| Value::Object(Map::new()))
}

pub async fn create(
    db: &DatabaseConnection,
    payload: SaveContentTypeRequest,
) -> AppResult<ContentTypeResponse> {
    let name = payload.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::Validation("it needs a name".to_string()));
    }

    if all(db).await?.len() >= MAX_TYPES {
        return Err(AppError::Validation(format!(
            "a site may have {MAX_TYPES} kinds of thing; that is already more than a sidebar can show"
        )));
    }

    let slug = crate::slug::slugify_or(payload.slug.as_deref().unwrap_or(&name), "kind");
    if content_type::Entity::find()
        .filter(content_type::Column::Slug.eq(&slug))
        .one(db)
        .await?
        .is_some()
    {
        return Err(AppError::Conflict(format!(
            "this site already has a kind called \"{slug}\""
        )));
    }

    let fields = definitions(payload.fields)?;
    let now = Utc::now().fixed_offset();
    let order = all(db).await?.len() as i32;

    let made = content_type::ActiveModel {
        id: Set(Uuid::now_v7()),
        slug: Set(slug),
        name: Set(name.clone()),
        plural: Set(plural_of(payload.plural, &name)),
        fields: Set(serde_json::to_string(&fields)
            .map_err(|err| AppError::Internal(format!("could not store the fields: {err}")))?),
        built_in: Set(false),
        sort_order: Set(order),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(db)
    .await?;

    Ok(ContentTypeResponse {
        id: made.id,
        slug: made.slug,
        name: made.name,
        plural: made.plural,
        fields,
        built_in: made.built_in,
        count: 0,
    })
}

pub async fn update(
    db: &DatabaseConnection,
    id: Uuid,
    payload: SaveContentTypeRequest,
) -> AppResult<ContentTypeResponse> {
    use sea_orm::PaginatorTrait;

    let existing = content_type::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("kind {id}")))?;

    let name = payload.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::Validation("it needs a name".to_string()));
    }

    let fields = definitions(payload.fields)?;
    let slug = existing.slug.clone();
    let built_in = existing.built_in;

    // The address is not changed, ever. It is in the addresses a front end
    // already fetches, and renaming it would empty somebody's site without
    // anything having gone wrong.
    let mut model: content_type::ActiveModel = existing.into();
    model.name = Set(name.clone());
    model.plural = Set(plural_of(payload.plural, &name));
    model.fields = Set(serde_json::to_string(&fields)
        .map_err(|err| AppError::Internal(format!("could not store the fields: {err}")))?);
    model.updated_at = Set(Utc::now().fixed_offset());
    let saved = model.update(db).await?;

    let count = post::Entity::find()
        .filter(post::Column::Kind.eq(&slug))
        .count(db)
        .await?;

    Ok(ContentTypeResponse {
        id: saved.id,
        slug,
        name: saved.name,
        plural: saved.plural,
        fields,
        built_in,
        count,
    })
}

/// Removes a kind nothing is written in.
///
/// A kind with content behind it is not removed, because doing so would leave
/// rows describing themselves as something this site has never heard of. Empty
/// it first, and then it is a decision about a name rather than about a
/// hundred pages.
pub async fn remove(db: &DatabaseConnection, id: Uuid) -> AppResult<()> {
    use sea_orm::PaginatorTrait;

    let existing = content_type::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("kind {id}")))?;

    if existing.built_in {
        return Err(AppError::Validation(format!(
            "\"{}\" is one this site always has",
            existing.slug
        )));
    }

    let count = post::Entity::find()
        .filter(post::Column::Kind.eq(&existing.slug))
        .count(db)
        .await?;
    if count > 0 {
        return Err(AppError::Validation(format!(
            "there are {count} of these; move or remove them first"
        )));
    }

    // The bin counts too. Something restored into a kind this site no longer
    // has would be a row nothing can describe, list or open.
    let binned = crate::entities::trash::Entity::find()
        .filter(crate::entities::trash::Column::Kind.eq(&existing.slug))
        .count(db)
        .await?;
    if binned > 0 {
        return Err(AppError::Validation(format!(
            "there are {binned} of these in the bin; empty it first, or they would come back as nothing"
        )));
    }

    content_type::Entity::delete_by_id(id).exec(db).await?;
    Ok(())
}

/// A kind may have no fields at all — a page often does not — which is the one
/// way this differs from a form.
fn definitions(fields: Vec<FormField>) -> AppResult<Vec<FormField>> {
    if fields.is_empty() {
        return Ok(fields);
    }
    clean_fields(fields)
}

fn plural_of(given: Option<String>, name: &str) -> String {
    match given.map(|plural| plural.trim().to_string()) {
        Some(plural) if !plural.is_empty() => plural,
        _ => name.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{checked_values, definitions};
    use crate::dto::forms::{FieldKind, FormField};
    use serde_json::json;

    fn price() -> Vec<FormField> {
        vec![FormField {
            name: "price".to_string(),
            label: "Price".to_string(),
            kind: FieldKind::Number,
            required: true,
            options: Vec::new(),
        }]
    }

    #[test]
    fn a_kind_may_have_no_fields() {
        assert!(definitions(Vec::new()).is_ok());
    }

    #[test]
    fn a_value_is_checked_against_the_field_it_is_in() {
        assert!(checked_values(&price(), Some(json!({ "price": 1200 })), true).is_ok());
        assert!(checked_values(&price(), Some(json!({ "price": "free" })), true).is_err());
        // Wrong is wrong whether or not it is finished.
        assert!(checked_values(&price(), Some(json!({ "price": "free" })), false).is_err());
    }

    #[test]
    fn a_required_field_is_required_once_it_goes_out() {
        assert!(checked_values(&price(), Some(json!({})), true).is_err());
        assert!(checked_values(&price(), None, true).is_err());
    }

    /// A draft is half-written by definition: the price is missing because
    /// nobody has decided it, not because anything is wrong.
    #[test]
    fn a_draft_may_be_missing_it() {
        assert!(checked_values(&price(), Some(json!({})), false).is_ok());
        assert!(checked_values(&price(), None, false).is_ok());
    }

    #[test]
    fn a_field_the_kind_does_not_have_is_refused() {
        assert!(
            checked_values(&price(), Some(json!({ "price": 1, "colour": "red" })), true).is_err()
        );
    }

    /// Posts and pages have no fields of their own until somebody gives them
    /// some, and sending values to one that has none is a mistake worth
    /// hearing about rather than dropping.
    #[test]
    fn a_kind_with_no_fields_takes_none() {
        assert!(checked_values(&[], None, true).is_ok());
        assert!(checked_values(&[], Some(json!({})), true).is_ok());
        assert!(checked_values(&[], Some(json!({ "price": 1 })), true).is_err());
    }
}

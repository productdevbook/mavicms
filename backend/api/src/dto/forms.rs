use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use utoipa::ToSchema;

use crate::error::{AppError, AppResult};

/// The most fields one form may define.
const MAX_FIELDS: usize = 60;
/// The longest a single submitted text value may be.
const MAX_VALUE_CHARS: usize = 10_000;
/// The most choices a `select` field may offer.
const MAX_OPTIONS: usize = 200;

/// What a field accepts.
///
/// Deliberately small. Each kind exists because it changes how a value is
/// *checked*, not how it looks — anything that only changes the look belongs
/// to whoever draws the form, which here is somebody else's software.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    Text,
    Textarea,
    Email,
    Phone,
    Number,
    Checkbox,
    Select,
    Date,
    Url,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct FormField {
    /// The key the submitted JSON uses.
    pub name: String,
    /// What a person filling it in is shown.
    pub label: String,
    #[serde(rename = "type")]
    pub kind: FieldKind,
    #[serde(default)]
    pub required: bool,
    /// The permitted values of a `select`. Ignored by every other kind.
    #[serde(default)]
    pub options: Vec<String>,
}

/// A field name is a JSON key that somebody else's code will type by hand.
fn clean_name(name: &str) -> AppResult<String> {
    let name = name.trim().to_string();
    let usable = (1..=60).contains(&name.chars().count())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if !usable {
        return Err(AppError::Validation(format!(
            "\"{name}\" is not a usable field name: 1-60 letters, numbers, _ or -"
        )));
    }
    Ok(name)
}

/// Checks a set of field definitions and hands back the cleaned version.
pub fn clean_fields(fields: Vec<FormField>) -> AppResult<Vec<FormField>> {
    if fields.is_empty() {
        return Err(AppError::Validation(
            "a form needs at least one field".to_string(),
        ));
    }
    if fields.len() > MAX_FIELDS {
        return Err(AppError::Validation(format!(
            "a form may have at most {MAX_FIELDS} fields"
        )));
    }

    let mut cleaned: Vec<FormField> = Vec::with_capacity(fields.len());
    for mut field in fields {
        field.name = clean_name(&field.name)?;
        if cleaned.iter().any(|other| other.name == field.name) {
            return Err(AppError::Validation(format!(
                "two fields are both called \"{}\"",
                field.name
            )));
        }

        field.label = field.label.trim().to_string();
        if field.label.is_empty() {
            field.label = field.name.clone();
        }

        if field.kind == FieldKind::Select {
            field.options.retain(|option| !option.trim().is_empty());
            if field.options.is_empty() {
                return Err(AppError::Validation(format!(
                    "\"{}\" is a list of choices with nothing to choose from",
                    field.name
                )));
            }
            if field.options.len() > MAX_OPTIONS {
                return Err(AppError::Validation(format!(
                    "\"{}\" may offer at most {MAX_OPTIONS} choices",
                    field.name
                )));
            }
        } else {
            field.options.clear();
        }

        cleaned.push(field);
    }
    Ok(cleaned)
}

fn missing(field: &FormField) -> AppError {
    AppError::Validation(format!("\"{}\" is required", field.name))
}

fn wrong(field: &FormField, wanted: &str) -> AppError {
    AppError::Validation(format!("\"{}\" must be {wanted}", field.name))
}

/// Whether a value counts as "not filled in".
fn empty(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(text) => text.trim().is_empty(),
        Value::Array(items) => items.is_empty(),
        _ => false,
    }
}

fn text_of(field: &FormField, value: &Value) -> AppResult<String> {
    // Numbers and booleans are accepted where text is wanted: a phone number
    // typed into JSON as a number is a mistake on the caller's side that
    // rejecting the whole submission would not help anybody fix.
    let text = match value {
        Value::String(text) => text.trim().to_string(),
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        _ => return Err(wrong(field, "text")),
    };
    if text.chars().count() > MAX_VALUE_CHARS {
        return Err(AppError::Validation(format!(
            "\"{}\" is longer than {MAX_VALUE_CHARS} characters",
            field.name
        )));
    }
    Ok(text)
}

fn check(field: &FormField, value: &Value) -> AppResult<Value> {
    Ok(match field.kind {
        FieldKind::Text | FieldKind::Textarea | FieldKind::Phone => {
            Value::String(text_of(field, value)?)
        }
        FieldKind::Email => {
            let text = text_of(field, value)?;
            // Not a full address grammar, on purpose: the only mistakes worth
            // catching here are the ones a person can see and fix. Whether an
            // address receives mail is a question only sending to it answers.
            if !text.contains('@') || text.starts_with('@') || text.ends_with('@') {
                return Err(wrong(field, "an email address"));
            }
            Value::String(text)
        }
        FieldKind::Url => {
            let text = text_of(field, value)?;
            if !text.starts_with("http://") && !text.starts_with("https://") {
                return Err(wrong(field, "a link starting with http:// or https://"));
            }
            Value::String(text)
        }
        FieldKind::Number => match value {
            Value::Number(number) => Value::Number(number.clone()),
            Value::String(text) => text
                .trim()
                .parse::<f64>()
                .ok()
                .and_then(serde_json::Number::from_f64)
                .map(Value::Number)
                .ok_or_else(|| wrong(field, "a number"))?,
            _ => return Err(wrong(field, "a number")),
        },
        FieldKind::Checkbox => match value {
            Value::Bool(flag) => Value::Bool(*flag),
            Value::String(text) => match text.trim() {
                "true" | "yes" | "on" | "1" => Value::Bool(true),
                "false" | "no" | "off" | "0" | "" => Value::Bool(false),
                _ => return Err(wrong(field, "true or false")),
            },
            _ => return Err(wrong(field, "true or false")),
        },
        FieldKind::Select => {
            let text = text_of(field, value)?;
            if !field.options.iter().any(|option| option == &text) {
                return Err(AppError::Validation(format!(
                    "\"{}\" is not one of the choices for \"{}\"",
                    text, field.name
                )));
            }
            Value::String(text)
        }
        FieldKind::Date => {
            let text = text_of(field, value)?;
            chrono::NaiveDate::parse_from_str(&text, "%Y-%m-%d")
                .map_err(|_| wrong(field, "a date written as YYYY-MM-DD"))?;
            Value::String(text)
        }
    })
}

/// Checks one submission against the form's fields.
///
/// A key the form does not define is refused rather than kept. Storing it
/// would let anyone who knows the address decide what this site's database
/// holds, and a typo in a field name would look like it worked while the
/// value quietly never appeared in the panel.
pub fn clean_submission(fields: &[FormField], submitted: Map<String, Value>) -> AppResult<Value> {
    let unknown: Vec<&String> = submitted
        .keys()
        .filter(|key| !fields.iter().any(|field| &field.name == *key))
        .collect();
    if let Some(first) = unknown.first() {
        return Err(AppError::Validation(format!(
            "this form has no field called \"{first}\""
        )));
    }

    let mut cleaned = Map::new();
    for field in fields {
        match submitted.get(&field.name) {
            Some(value) if !empty(value) => {
                cleaned.insert(field.name.clone(), check(field, value)?);
            }
            _ if field.required => return Err(missing(field)),
            // Left out and not required: recorded as absent rather than as an
            // empty string, so the panel can tell "no answer" from "".
            _ => {
                cleaned.insert(field.name.clone(), Value::Null);
            }
        }
    }
    Ok(Value::Object(cleaned))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SaveFormRequest {
    pub name: String,
    /// Left out on creation, the name is used. Changing it changes the
    /// address other software posts to.
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub description: String,
    pub fields: Vec<FormField>,
    #[serde(default = "yes")]
    pub active: bool,
    /// An address to tell when something comes in. Empty means nobody.
    #[serde(default)]
    pub notify: String,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FormResponse {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub description: String,
    pub fields: Vec<FormField>,
    pub active: bool,
    pub notify: String,
    pub submissions: u64,
    /// How many have not been opened in the panel yet.
    pub unseen: u64,
    pub created_at: String,
    pub updated_at: String,
}

/// What somebody else's software posts: one entry per field.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(transparent)]
#[schema(value_type = Object)]
pub struct SubmissionRequest(pub Map<String, Value>);

/// What a form accepts, for whatever is going to fill it in.
///
/// No account, because the thing that needs this is the page the form is
/// drawn on, and that page is read by people who have none. Nothing here is
/// private: these are the labels and the choices a visitor is already looking
/// at. The form's internal note is deliberately not among them — the panel
/// says that only its owner sees it.
#[derive(Debug, Serialize, ToSchema)]
pub struct FormSchema {
    pub name: String,
    pub slug: String,
    pub fields: Vec<FormField>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SubmissionResponse {
    pub id: String,
    pub form_id: String,
    #[schema(value_type = Object)]
    pub data: Value,
    pub seen: bool,
    pub created_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(name: &str, kind: FieldKind, required: bool) -> FormField {
        FormField {
            name: name.to_string(),
            label: String::new(),
            kind,
            required,
            options: Vec::new(),
        }
    }

    fn sent(pairs: &[(&str, Value)]) -> Map<String, Value> {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_string(), value.clone()))
            .collect()
    }

    #[test]
    fn a_label_falls_back_to_the_key() {
        let cleaned = clean_fields(vec![field("email", FieldKind::Email, true)]).unwrap();
        assert_eq!(cleaned[0].label, "email");
    }

    #[test]
    fn two_fields_may_not_share_a_key() {
        let twice = vec![
            field("name", FieldKind::Text, false),
            field("name", FieldKind::Text, false),
        ];
        assert!(clean_fields(twice).is_err());
    }

    #[test]
    fn a_list_needs_something_to_choose_from() {
        assert!(clean_fields(vec![field("plan", FieldKind::Select, false)]).is_err());
    }

    #[test]
    fn choices_are_dropped_from_the_kinds_that_ignore_them() {
        let mut text = field("note", FieldKind::Text, false);
        text.options = vec!["a".to_string()];
        assert!(clean_fields(vec![text]).unwrap()[0].options.is_empty());
    }

    #[test]
    fn a_key_the_form_does_not_define_is_refused() {
        let fields = clean_fields(vec![field("name", FieldKind::Text, false)]).unwrap();
        let error = clean_submission(&fields, sent(&[("wp_admin", Value::from("x"))])).unwrap_err();
        assert!(error.to_string().contains("wp_admin"));
    }

    #[test]
    fn whitespace_does_not_satisfy_a_required_field() {
        let fields = clean_fields(vec![field("name", FieldKind::Text, true)]).unwrap();
        assert!(clean_submission(&fields, sent(&[("name", Value::from("   "))])).is_err());
    }

    #[test]
    fn a_field_left_out_is_recorded_as_absent() {
        let fields = clean_fields(vec![
            field("name", FieldKind::Text, true),
            field("phone", FieldKind::Phone, false),
        ])
        .unwrap();

        let stored = clean_submission(&fields, sent(&[("name", Value::from("Ada"))])).unwrap();
        assert_eq!(stored["phone"], Value::Null);
    }

    #[test]
    fn a_number_typed_as_text_is_taken_as_a_number() {
        let fields = clean_fields(vec![field("how_many", FieldKind::Number, true)]).unwrap();
        let stored = clean_submission(&fields, sent(&[("how_many", Value::from("42"))])).unwrap();
        assert_eq!(stored["how_many"], Value::from(42.0));
    }

    #[test]
    fn a_choice_outside_the_list_is_refused() {
        let mut select = field("plan", FieldKind::Select, true);
        select.options = vec!["small".to_string(), "large".to_string()];
        let fields = clean_fields(vec![select]).unwrap();

        assert!(clean_submission(&fields, sent(&[("plan", Value::from("huge"))])).is_err());
        assert!(clean_submission(&fields, sent(&[("plan", Value::from("large"))])).is_ok());
    }

    #[test]
    fn a_date_has_to_be_a_date() {
        let fields = clean_fields(vec![field("when", FieldKind::Date, true)]).unwrap();
        assert!(clean_submission(&fields, sent(&[("when", Value::from("31/01/2026"))])).is_err());
        assert!(clean_submission(&fields, sent(&[("when", Value::from("2026-01-31"))])).is_ok());
    }

    #[test]
    fn a_very_long_value_is_refused() {
        let fields = clean_fields(vec![field("note", FieldKind::Textarea, false)]).unwrap();
        let long = "x".repeat(MAX_VALUE_CHARS + 1);
        assert!(clean_submission(&fields, sent(&[("note", Value::from(long))])).is_err());
    }
}

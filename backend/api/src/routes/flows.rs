use axum::{
    Json,
    extract::{Path, Query},
    http::StatusCode,
};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    entities::{flow, flow_run, flow_step},
    error::{AppError, AppResult},
    flows,
    tenants::Site,
};

#[derive(Debug, Serialize, ToSchema)]
pub struct StepResponse {
    pub id: String,
    pub position: i32,
    pub action: String,
    /// The step's own settings, as it was given them.
    pub config: serde_json::Value,
    pub on_error: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FlowResponse {
    pub id: String,
    pub name: String,
    pub trigger_kind: String,
    pub trigger_config: serde_json::Value,
    pub enabled: bool,
    /// Where a webhook trigger listens. Absent for every other kind.
    pub webhook_url: Option<String>,
    pub steps: Vec<StepResponse>,
    pub updated_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RunStepResponse {
    pub position: i32,
    pub action: String,
    pub status: String,
    pub output: serde_json::Value,
    pub error: Option<String>,
    pub finished_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RunResponse {
    pub id: String,
    pub flow_id: String,
    pub status: String,
    pub trigger: serde_json::Value,
    pub error: Option<String>,
    pub created_at: String,
    pub finished_at: Option<String>,
    /// Only when one run was asked for by name.
    pub steps: Vec<RunStepResponse>,
}

/// What a flow can wait for and what it can do, with the values each trigger
/// offers. The panel builds its pickers from this rather than from a list it
/// keeps its own copy of and forgets to update.
#[derive(Debug, Serialize, ToSchema)]
pub struct Vocabulary {
    pub triggers: Vec<Described>,
    pub actions: Vec<Described>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Described {
    pub kind: String,
    /// For a trigger, the values it puts within reach of every step.
    pub offers: Vec<Offered>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Offered {
    pub path: String,
    pub what: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SaveStep {
    pub action: String,
    pub config: serde_json::Value,
    /// "stop" or "continue". Stops when left out.
    pub on_error: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SaveFlow {
    pub name: String,
    pub trigger_kind: String,
    pub trigger_config: Option<serde_json::Value>,
    pub enabled: Option<bool>,
    pub steps: Vec<SaveStep>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct RunQuery {
    /// Only this flow's runs.
    pub flow_id: Option<Uuid>,
    pub limit: Option<u64>,
}

fn described(flow: flow::Model, steps: Vec<flow_step::Model>, host: &str) -> FlowResponse {
    FlowResponse {
        id: flow.id.to_string(),
        name: flow.name,
        webhook_url: flow
            .webhook_key
            .as_ref()
            .map(|key| format!("https://{host}/api/flows/hook/{key}")),
        trigger_kind: flow.trigger_kind,
        trigger_config: serde_json::from_str(&flow.trigger_config)
            .unwrap_or(serde_json::Value::Null),
        enabled: flow.enabled,
        steps: steps
            .into_iter()
            .map(|step| StepResponse {
                id: step.id.to_string(),
                position: step.position,
                action: step.action,
                config: serde_json::from_str(&step.config).unwrap_or(serde_json::Value::Null),
                on_error: step.on_error,
            })
            .collect(),
        updated_at: flow.updated_at.to_rfc3339(),
    }
}

/// What a flow can be made of.
#[utoipa::path(
    get,
    path = "/flows/vocabulary",
    tag = "flows",
    responses((status = 200, description = "Triggers and actions", body = Vocabulary))
)]
pub async fn vocabulary() -> Json<Vocabulary> {
    let describe = |kind: &str| Described {
        kind: kind.to_string(),
        offers: flows::offered_by(kind)
            .into_iter()
            .map(|(path, what)| Offered {
                path: path.to_string(),
                what: what.to_string(),
            })
            .collect(),
    };

    Json(Vocabulary {
        triggers: flows::trigger::ALL.iter().map(|k| describe(k)).collect(),
        actions: flows::action::ALL
            .iter()
            .map(|kind| Described {
                kind: kind.to_string(),
                offers: Vec::new(),
            })
            .collect(),
    })
}

/// Every flow this site has.
#[utoipa::path(
    get,
    path = "/flows",
    tag = "flows",
    responses((status = 200, description = "The flows", body = Vec<FlowResponse>))
)]
pub async fn list_flows(
    Site(state): Site,
    axum::Extension(resolved): axum::Extension<crate::tenants::Resolved>,
) -> AppResult<Json<Vec<FlowResponse>>> {
    let db = state.db_or_unavailable()?;
    let host = host_of(&resolved);

    let mut out = Vec::new();
    for found in flow::Entity::find()
        .order_by_asc(flow::Column::Name)
        .all(db)
        .await?
    {
        let steps = flow_step::Entity::find()
            .filter(flow_step::Column::FlowId.eq(found.id))
            .order_by_asc(flow_step::Column::Position)
            .all(db)
            .await?;
        out.push(described(found, steps, &host));
    }
    Ok(Json(out))
}

/// Writes a flow, steps and all.
///
/// The steps are replaced rather than patched: the panel sends the whole
/// arrangement because that is what somebody rearranging boxes on a canvas
/// produces, and a half-applied rearrangement is a flow nobody wrote.
#[utoipa::path(
    post,
    path = "/flows",
    tag = "flows",
    request_body = SaveFlow,
    responses(
        (status = 201, description = "Made", body = FlowResponse),
        (status = 400, description = "Not something a flow can do", body = crate::error::ErrorBody),
    )
)]
pub async fn create_flow(
    Site(state): Site,
    axum::Extension(resolved): axum::Extension<crate::tenants::Resolved>,
    Json(payload): Json<SaveFlow>,
) -> AppResult<(StatusCode, Json<FlowResponse>)> {
    let db = state.db_or_unavailable()?;
    let host = host_of(&resolved);
    let trigger_kind = flows::known_trigger(payload.trigger_kind.trim())?;

    let id = Uuid::now_v7();
    let now = Utc::now().fixed_offset();
    flow::ActiveModel {
        id: Set(id),
        name: Set(cleaned_name(&payload.name)?),
        trigger_kind: Set(trigger_kind.to_string()),
        trigger_config: Set(payload
            .trigger_config
            .unwrap_or(serde_json::json!({}))
            .to_string()),
        enabled: Set(payload.enabled.unwrap_or(true)),
        // Unguessable and per flow: the address is the whole of the
        // authentication, so it comes from the same source a salt does rather
        // than from the name or the id.
        webhook_key: Set((trigger_kind == flows::trigger::WEBHOOK).then(fresh_key)),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(db)
    .await?;

    write_steps(db, id, &payload.steps).await?;
    let made = flow::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::Internal("the flow went away".to_string()))?;
    let steps = flow_step::Entity::find()
        .filter(flow_step::Column::FlowId.eq(id))
        .order_by_asc(flow_step::Column::Position)
        .all(db)
        .await?;

    Ok((StatusCode::CREATED, Json(described(made, steps, &host))))
}

/// Changes one.
#[utoipa::path(
    put,
    path = "/flows/{id}",
    tag = "flows",
    params(("id" = String, Path, description = "Flow id")),
    request_body = SaveFlow,
    responses(
        (status = 200, description = "Saved", body = FlowResponse),
        (status = 404, description = "No such flow", body = crate::error::ErrorBody),
    )
)]
pub async fn update_flow(
    Site(state): Site,
    axum::Extension(resolved): axum::Extension<crate::tenants::Resolved>,
    Path(id): Path<Uuid>,
    Json(payload): Json<SaveFlow>,
) -> AppResult<Json<FlowResponse>> {
    let db = state.db_or_unavailable()?;
    let host = host_of(&resolved);
    let found = flow::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound("that flow".to_string()))?;
    let trigger_kind = flows::known_trigger(payload.trigger_kind.trim())?;

    let had_key = found.webhook_key.clone();
    let mut row: flow::ActiveModel = found.into();
    row.name = Set(cleaned_name(&payload.name)?);
    row.trigger_kind = Set(trigger_kind.to_string());
    row.trigger_config = Set(payload
        .trigger_config
        .unwrap_or(serde_json::json!({}))
        .to_string());
    row.enabled = Set(payload.enabled.unwrap_or(true));
    // A flow that becomes a webhook gets an address; one that stops being a
    // webhook loses it, so the old address stops working the moment somebody
    // changes their mind about it.
    row.webhook_key = Set(match trigger_kind {
        flows::trigger::WEBHOOK => Some(had_key.unwrap_or_else(fresh_key)),
        _ => None,
    });
    row.updated_at = Set(Utc::now().fixed_offset());
    let saved = row.update(db).await?;

    flow_step::Entity::delete_many()
        .filter(flow_step::Column::FlowId.eq(id))
        .exec(db)
        .await?;
    write_steps(db, id, &payload.steps).await?;

    let steps = flow_step::Entity::find()
        .filter(flow_step::Column::FlowId.eq(id))
        .order_by_asc(flow_step::Column::Position)
        .all(db)
        .await?;
    Ok(Json(described(saved, steps, &host)))
}

/// Removes one, and the record of what it did with it.
#[utoipa::path(
    delete,
    path = "/flows/{id}",
    tag = "flows",
    params(("id" = String, Path, description = "Flow id")),
    responses((status = 204, description = "Gone"))
)]
pub async fn delete_flow(Site(state): Site, Path(id): Path<Uuid>) -> AppResult<StatusCode> {
    let db = state.db_or_unavailable()?;
    flow_step::Entity::delete_many()
        .filter(flow_step::Column::FlowId.eq(id))
        .exec(db)
        .await?;
    flow_run::Entity::delete_many()
        .filter(flow_run::Column::FlowId.eq(id))
        .exec(db)
        .await?;
    flow::Entity::delete_by_id(id).exec(db).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Runs one now, with a trigger somebody made up, and answers with what
/// happened. The difference between an automation you believe in and one you
/// hope about.
#[utoipa::path(
    post,
    path = "/flows/{id}/test",
    tag = "flows",
    params(("id" = String, Path, description = "Flow id")),
    request_body = serde_json::Value,
    responses((status = 200, description = "What happened", body = RunResponse))
)]
pub async fn test_flow(
    Site(state): Site,
    axum::Extension(resolved): axum::Extension<crate::tenants::Resolved>,
    Path(id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> AppResult<Json<RunResponse>> {
    let db = state.db_or_unavailable()?;
    let host = host_of(&resolved);
    let found = flow::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound("that flow".to_string()))?;

    let run = flow_run::ActiveModel {
        id: Set(Uuid::now_v7()),
        flow_id: Set(found.id),
        status: Set(flows::QUEUED.to_string()),
        trigger: Set(payload.to_string()),
        error: Set(None),
        depth: Set(0),
        created_at: Set(Utc::now().fixed_offset()),
        started_at: Set(None),
        finished_at: Set(None),
    }
    .insert(db)
    .await?;

    flows::run_queued(db, &state.secrets, &format!("https://{host}")).await?;
    one_run(db, run.id).await.map(Json)
}

/// What the flows have been doing.
#[utoipa::path(
    get,
    path = "/flows/runs",
    tag = "flows",
    params(RunQuery),
    responses((status = 200, description = "Newest first", body = Vec<RunResponse>))
)]
pub async fn list_runs(
    Site(state): Site,
    Query(query): Query<RunQuery>,
) -> AppResult<Json<Vec<RunResponse>>> {
    let db = state.db_or_unavailable()?;
    let mut find = flow_run::Entity::find();
    if let Some(flow_id) = query.flow_id {
        find = find.filter(flow_run::Column::FlowId.eq(flow_id));
    }

    let rows = find
        .order_by_desc(flow_run::Column::CreatedAt)
        .limit(query.limit.unwrap_or(50).clamp(1, 200))
        .all(db)
        .await?;

    Ok(Json(rows.into_iter().map(shallow).collect()))
}

/// One run, step by step.
#[utoipa::path(
    get,
    path = "/flows/runs/{id}",
    tag = "flows",
    params(("id" = String, Path, description = "Run id")),
    responses((status = 200, description = "What happened", body = RunResponse))
)]
pub async fn get_run(Site(state): Site, Path(id): Path<Uuid>) -> AppResult<Json<RunResponse>> {
    one_run(state.db_or_unavailable()?, id).await.map(Json)
}

/// Where a webhook trigger listens.
///
/// No account: the address is the whole of the authentication, which is why
/// the key is unguessable and why turning the trigger off takes it away. What
/// arrives is not trusted for anything except being written into the run.
#[utoipa::path(
    post,
    path = "/flows/hook/{key}",
    tag = "flows",
    params(("key" = String, Path, description = "The flow's own key")),
    request_body = String,
    responses(
        (status = 202, description = "Queued"),
        (status = 404, description = "No flow listens there", body = crate::error::ErrorBody),
    ),
    security(())
)]
pub async fn receive_hook(
    Site(state): Site,
    Path(key): Path<String>,
    body: String,
) -> AppResult<StatusCode> {
    let db = state.db_or_unavailable()?;
    let found = flow::Entity::find()
        .filter(flow::Column::WebhookKey.eq(key))
        .filter(flow::Column::Enabled.eq(true))
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound("no flow listens there".to_string()))?;

    // Bounded before it is stored: a run record is not a place to put whatever
    // somebody feels like posting.
    let text: String = body.chars().take(16 * 1024).collect();
    let as_json: serde_json::Value = serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);

    flow_run::ActiveModel {
        id: Set(Uuid::now_v7()),
        flow_id: Set(found.id),
        status: Set(flows::QUEUED.to_string()),
        trigger: Set(serde_json::json!({ "body": text, "json": as_json }).to_string()),
        error: Set(None),
        depth: Set(0),
        created_at: Set(Utc::now().fixed_offset()),
        started_at: Set(None),
        finished_at: Set(None),
    }
    .insert(db)
    .await?;

    Ok(StatusCode::ACCEPTED)
}

/// The unguessable part of a webhook's address.
///
/// From the same source a password salt comes from, not from the flow's name
/// or its id: the address is the whole of the authentication, so anything
/// derived from something a caller can see would not be one.
fn fresh_key() -> String {
    use argon2::password_hash::{SaltString, rand_core::OsRng};
    SaltString::generate(&mut OsRng)
        .as_str()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(32)
        .collect()
}

fn host_of(resolved: &crate::tenants::Resolved) -> String {
    match resolved {
        crate::tenants::Resolved::Tenant(tenant) => tenant.host.clone(),
        _ => "localhost".to_string(),
    }
}

fn cleaned_name(name: &str) -> AppResult<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::Validation("a flow needs a name".to_string()));
    }
    Ok(name.chars().take(120).collect())
}

async fn write_steps(
    db: &sea_orm::DatabaseConnection,
    flow_id: Uuid,
    steps: &[SaveStep],
) -> AppResult<()> {
    for (position, step) in steps.iter().enumerate() {
        let action = flows::known_action(step.action.trim())?;
        flow_step::ActiveModel {
            id: Set(Uuid::now_v7()),
            flow_id: Set(flow_id),
            position: Set(position as i32),
            action: Set(action.to_string()),
            config: Set(step.config.to_string()),
            on_error: Set(match step.on_error.as_deref() {
                Some("continue") => "continue".to_string(),
                _ => "stop".to_string(),
            }),
            created_at: Set(Utc::now().fixed_offset()),
        }
        .insert(db)
        .await?;
    }
    Ok(())
}

fn shallow(run: flow_run::Model) -> RunResponse {
    RunResponse {
        id: run.id.to_string(),
        flow_id: run.flow_id.to_string(),
        status: run.status,
        trigger: serde_json::from_str(&run.trigger).unwrap_or(serde_json::Value::Null),
        error: run.error,
        created_at: run.created_at.to_rfc3339(),
        finished_at: run.finished_at.map(|at| at.to_rfc3339()),
        steps: Vec::new(),
    }
}

async fn one_run(db: &sea_orm::DatabaseConnection, id: Uuid) -> AppResult<RunResponse> {
    let run = flow_run::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound("that run".to_string()))?;

    let mut out = shallow(run);
    out.steps = flows::steps_of_run(db, id)
        .await?
        .into_iter()
        .map(|step| RunStepResponse {
            position: step.position,
            action: step.action,
            status: step.status,
            output: serde_json::from_str(&step.output).unwrap_or(serde_json::Value::Null),
            error: step.error,
            finished_at: step.finished_at.to_rfc3339(),
        })
        .collect();
    Ok(out)
}

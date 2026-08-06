//! What an agency can be asked about its sites.
//!
//! This is the console, reached the same way the site tools reach a site. The
//! questions it answers are the ones an agency actually has on a Monday
//! morning and can only answer today by opening fifty tabs: which of these
//! built, which did not, and what has come in.
//!
//! Every tool here is scoped to the agency the token belongs to. A site the
//! agency does not look after is not filtered out of the answer — it is never
//! looked at, because the list it works from is the agency's own.

use serde_json::{Value, json};

use super::Tool;
use crate::{
    console::Operator,
    error::{AppError, AppResult},
    tenants::{Hosting, Tenant},
};

pub const TOOLS: &[Tool] = &[
    Tool {
        name: "sites_list",
        title: "The sites this agency looks after",
        description: "Every site, with the address it answers on, whether it is switched on, \
            and how its last build went. This is the morning's question — which of these needs \
            attention — in one call.",
        writes: false,
        schema: || {
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "failing_only": {
                        "type": "boolean",
                        "description": "Only sites whose last build did not succeed"
                    }
                }
            })
        },
    },
    Tool {
        name: "site_builds",
        title: "How one site has been building",
        description: "The recent builds of one site, newest first, with how long each took and \
            what it said. Use it after sites_list says one is failing.",
        writes: false,
        schema: || {
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["site"],
                "properties": {
                    "site": { "type": "string", "description": "The site's address or its id" },
                    "limit": { "type": "integer", "description": "Up to 20. Default 5." }
                }
            })
        },
    },
    Tool {
        name: "site_publish",
        title: "Build a site's pages",
        description: "Ask one site to build again. A build already waiting is returned rather \
            than a second one queued.",
        writes: true,
        schema: || {
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["site"],
                "properties": { "site": { "type": "string", "description": "The site's address or its id" } }
            })
        },
    },
    Tool {
        name: "sites_add",
        title: "Add a site",
        description: "Make a new site on this server, answering on an address of its own. It \
            starts empty: whoever opens it runs the setup wizard and makes the first account. \
            Adding a site counts against what this agency is allowed.",
        writes: true,
        schema: || {
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["host"],
                "properties": {
                    "host": {
                        "type": "string",
                        "description": "The address it will answer on, such as example.com"
                    }
                }
            })
        },
    },
];

pub async fn call(
    hosting: &Hosting,
    operator: &Operator,
    name: &str,
    arguments: &Value,
) -> AppResult<Value> {
    match name {
        "sites_list" => list(hosting, operator, arguments).await,
        "site_builds" => builds(hosting, operator, arguments).await,
        "site_publish" => publish(hosting, operator, arguments).await,
        "sites_add" => add(hosting, operator, arguments).await,
        other => Err(AppError::NotFound(format!("tool {other}"))),
    }
}

/// This agency's sites, and nobody else's.
async fn owned(hosting: &Hosting, operator: &Operator) -> AppResult<Vec<Tenant>> {
    Ok(hosting
        .registry()?
        .all()
        .await?
        .into_iter()
        .filter(|tenant| tenant.organization_id == Some(operator.organization_id))
        .collect())
}

/// The one meant by an address or an id, from among this agency's own.
async fn named(hosting: &Hosting, operator: &Operator, arguments: &Value) -> AppResult<Tenant> {
    let asked = arguments
        .get("site")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::Validation("say which site".to_string()))?;

    let wanted = asked.to_lowercase();
    owned(hosting, operator)
        .await?
        .into_iter()
        .find(|tenant| {
            tenant.host == wanted || tenant.slug == wanted || tenant.id.to_string() == wanted
        })
        // Not found rather than forbidden: which other sites are on this
        // machine is not this agency's business, and an error that
        // distinguished "not yours" from "does not exist" would say.
        .ok_or_else(|| AppError::NotFound(format!("site {asked}")))
}

async fn list(hosting: &Hosting, operator: &Operator, arguments: &Value) -> AppResult<Value> {
    let control = hosting.registry()?.control();
    let failing_only = arguments
        .get("failing_only")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let mut described = Vec::new();
    for tenant in owned(hosting, operator).await? {
        let last = crate::publish::latest(control, tenant.id, 1).await?;
        let last = last.first();
        let status = last.map(|build| build.status.clone());

        if failing_only && status.as_deref() != Some(crate::publish::FAILED) {
            continue;
        }

        described.push(json!({
            "id": tenant.id,
            "host": tenant.host,
            "name": tenant.slug,
            "switched_on": tenant.active,
            "last_build": last.map(|build| json!({
                "status": build.status,
                "requested_at": build.requested_at,
                "finished_at": build.finished_at,
            })),
        }));
    }

    Ok(json!({ "sites": described }))
}

async fn builds(hosting: &Hosting, operator: &Operator, arguments: &Value) -> AppResult<Value> {
    let tenant = named(hosting, operator, arguments).await?;
    let how_many = arguments
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(5)
        .clamp(1, 20) as u8;

    let recent = crate::publish::latest(hosting.registry()?.control(), tenant.id, how_many).await?;

    Ok(json!({
        "host": tenant.host,
        "builds": recent
            .iter()
            .map(|build| json!({
                "status": build.status,
                "requested_at": build.requested_at,
                "started_at": build.started_at,
                "finished_at": build.finished_at,
                // Trimmed the way the panel trims it: a failing build's log is
                // the reason it failed, and the middle of it rarely is.
                "log": crate::publish::trimmed(&build.log),
            }))
            .collect::<Vec<_>>(),
    }))
}

async fn publish(hosting: &Hosting, operator: &Operator, arguments: &Value) -> AppResult<Value> {
    let tenant = named(hosting, operator, arguments).await?;
    let build = crate::publish::request(hosting.registry()?.control(), tenant.id).await?;

    Ok(json!({
        "host": tenant.host,
        "status": build.status,
        "requested_at": build.requested_at,
    }))
}

async fn add(hosting: &Hosting, operator: &Operator, arguments: &Value) -> AppResult<Value> {
    let host = arguments
        .get("host")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            AppError::Validation("the site needs an address to answer on".to_string())
        })?;

    let tenant = crate::console::add_site(hosting, operator, host).await?;

    Ok(json!({
        "id": tenant.id,
        "host": tenant.host,
        "name": tenant.slug,
        "next": format!(
            "point {} at this server and open it; the setup wizard makes the first account",
            tenant.host
        ),
    }))
}

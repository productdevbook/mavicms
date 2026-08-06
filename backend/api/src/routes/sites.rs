use axum::{Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    error::AppResult,
    tenants::{Hosting, Operator, Tenant},
};

#[derive(Debug, Serialize, ToSchema)]
pub struct SiteResponse {
    pub id: String,
    pub host: String,
    pub slug: String,
    /// The Postgres schema its tables live in.
    pub schema: String,
    /// Empty when the site lives on the server's own database, which is the
    /// normal case.
    pub database_url: String,
    pub active: bool,
}

impl From<Tenant> for SiteResponse {
    fn from(tenant: Tenant) -> Self {
        Self {
            id: tenant.id.to_string(),
            host: tenant.host,
            slug: tenant.slug,
            schema: tenant.schema,
            database_url: tenant.database_url,
            active: tenant.active,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateSiteRequest {
    /// The address this site will answer on.
    pub host: String,
    /// A short name for its folder. Defaults to the host with the dots taken
    /// out, which is usually what someone would have typed anyway.
    #[serde(default)]
    pub slug: Option<String>,
    /// A database server of its own. Empty — the normal case — puts the site
    /// in its own schema on the server's database; a site that outgrows that
    /// can be given an address here and nothing else changes.
    #[serde(default)]
    pub database_url: Option<String>,
}

/// The sites this server hosts.
#[utoipa::path(
    get,
    path = "/sites",
    tag = "sites",
    responses(
        (status = 200, description = "Hosted sites", body = Vec<SiteResponse>),
        (status = 403, description = "Not the server's own address", body = crate::error::ErrorBody),
    )
)]
pub async fn list_sites(
    _operator: Operator,
    State(hosting): State<Hosting>,
) -> AppResult<Json<Vec<SiteResponse>>> {
    Ok(Json(
        hosting
            .registry()?
            .all()
            .await?
            .into_iter()
            .map(SiteResponse::from)
            .collect(),
    ))
}

/// Add a site: its folder, its database with the schema already in place, and
/// the host it answers on.
#[utoipa::path(
    post,
    path = "/sites",
    tag = "sites",
    request_body = CreateSiteRequest,
    responses(
        (status = 201, description = "Site created", body = SiteResponse),
        (status = 403, description = "Not the server's own address", body = crate::error::ErrorBody),
        (status = 409, description = "That host or name is taken", body = crate::error::ErrorBody),
    )
)]
pub async fn create_site(
    _operator: Operator,
    State(hosting): State<Hosting>,
    Json(payload): Json<CreateSiteRequest>,
) -> AppResult<(StatusCode, Json<SiteResponse>)> {
    let slug = payload
        .slug
        .unwrap_or_else(|| payload.host.replace('.', "-"));

    let tenant = hosting
        .registry()?
        .create(
            &payload.host,
            &slug,
            payload.database_url.as_deref().unwrap_or_default(),
        )
        .await?;

    Ok((StatusCode::CREATED, Json(tenant.into())))
}

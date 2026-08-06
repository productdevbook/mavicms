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
            None,
        )
        .await?;

    Ok((StatusCode::CREATED, Json(tenant.into())))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateSiteRequest {
    /// Off leaves the content alone and stops the address serving.
    pub active: Option<bool>,
}

/// Switch a site on or off.
#[utoipa::path(
    put,
    path = "/sites/{id}",
    tag = "sites",
    params(("id" = String, Path, description = "Site id")),
    request_body = UpdateSiteRequest,
    responses((status = 204, description = "Changed"))
)]
pub async fn update_site(
    _operator: Operator,
    State(hosting): State<Hosting>,
    axum::extract::Path(id): axum::extract::Path<uuid::Uuid>,
    Json(payload): Json<UpdateSiteRequest>,
) -> AppResult<StatusCode> {
    if let Some(active) = payload.active {
        hosting.registry()?.set_active(id, active).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Remove a site: its content, its accounts, its uploads.
///
/// The address has to be sent back to confirm it. A site is somebody's work
/// and an id in a URL is not something anybody reads before clicking.
#[derive(Debug, Deserialize, ToSchema)]
pub struct DeleteSiteRequest {
    pub host: String,
}

#[utoipa::path(
    post,
    path = "/sites/{id}/delete",
    tag = "sites",
    params(("id" = String, Path, description = "Site id")),
    request_body = DeleteSiteRequest,
    responses(
        (status = 204, description = "Removed"),
        (status = 400, description = "That is not this site's address", body = crate::error::ErrorBody),
    )
)]
pub async fn delete_site(
    _operator: Operator,
    State(hosting): State<Hosting>,
    axum::extract::Path(id): axum::extract::Path<uuid::Uuid>,
    Json(payload): Json<DeleteSiteRequest>,
) -> AppResult<StatusCode> {
    let registry = hosting.registry()?;
    let tenant = registry
        .all()
        .await?
        .into_iter()
        .find(|tenant| tenant.id == id)
        .ok_or_else(|| crate::error::AppError::NotFound("site".to_string()))?;

    if payload.host.trim().to_lowercase() != tenant.host {
        return Err(crate::error::AppError::Validation(
            "type the site's address to confirm".to_string(),
        ));
    }

    tracing::warn!(site = %tenant.host, "removing a site");
    registry.remove(&tenant).await?;

    Ok(StatusCode::NO_CONTENT)
}

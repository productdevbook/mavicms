use axum::{Extension, Json, extract::State, http::StatusCode};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    error::{AppError, AppResult},
    publish::{self, Build, BuildConfig, SaveBuildConfig},
    tenants::{Hosting, Resolved},
};

#[derive(Debug, Serialize, ToSchema)]
pub struct PublishStatus {
    /// Absent until someone says where the site's pages come from.
    pub config: Option<BuildConfig>,
    /// Most recent first.
    pub builds: Vec<Build>,
    /// The agency that looks after how this site is built, if one does. When
    /// it is set, the settings below are theirs and this site can publish but
    /// not rewire.
    pub managed_by: Option<String>,
}

/// How many past builds the panel shows. Enough to see a pattern, not enough
/// to be a log viewer.
const HISTORY: u8 = 10;

/// Everything the panel shows about publishing one site.
pub async fn status_of(
    db: &sea_orm::DatabaseConnection,
    tenant: &crate::tenants::Tenant,
) -> AppResult<PublishStatus> {
    let managed_by = match tenant.organization_id {
        Some(id) => crate::console::organization(db, id)
            .await?
            .map(|organization| organization.name),
        None => None,
    };

    Ok(PublishStatus {
        config: publish::config(db, tenant.id).await?,
        builds: publish::latest(db, tenant.id, HISTORY).await?,
        managed_by,
    })
}

/// The site this request is for, which publishing is always about.
///
/// The server's own installation has no site to publish: it is the panel, and
/// its pages are this application.
fn site(resolved: &Resolved) -> AppResult<&crate::tenants::Tenant> {
    match resolved {
        Resolved::Tenant(tenant) => Ok(tenant),
        Resolved::Host => Err(AppError::Validation(
            "publishing belongs to a hosted site, not to the server itself".to_string(),
        )),
    }
}

/// Where this site's pages come from, and how the last builds went.
#[utoipa::path(
    get,
    path = "/publish",
    tag = "publish",
    responses((status = 200, description = "Build settings and history", body = PublishStatus))
)]
pub async fn get_publish(
    State(hosting): State<Hosting>,
    Extension(resolved): Extension<Resolved>,
) -> AppResult<Json<PublishStatus>> {
    let tenant = site(&resolved)?;
    let db = hosting.registry()?.control();

    Ok(Json(status_of(db, tenant).await?))
}

/// Say where this site's pages come from.
///
/// Refused when an agency looks after the site: the repository and the build
/// command are their work, and someone writing posts should not be able to
/// stop the site publishing by editing them. A site nobody manages sets its
/// own, because otherwise nobody could.
#[utoipa::path(
    put,
    path = "/publish",
    tag = "publish",
    request_body = SaveBuildConfig,
    responses(
        (status = 200, description = "Saved", body = BuildConfig),
        (status = 400, description = "Not a repository this can build", body = crate::error::ErrorBody),
        (status = 403, description = "An agency looks after this site", body = crate::error::ErrorBody),
    )
)]
pub async fn save_publish(
    State(hosting): State<Hosting>,
    Extension(resolved): Extension<Resolved>,
    Json(payload): Json<SaveBuildConfig>,
) -> AppResult<Json<BuildConfig>> {
    let tenant = site(&resolved)?;
    let registry = hosting.registry()?;

    if let Some(organization) = tenant.organization_id
        && let Some(agency) = crate::console::organization(registry.control(), organization).await?
    {
        return Err(AppError::Forbidden(format!(
            "{} looks after how this site is built",
            agency.name
        )));
    }

    Ok(Json(
        publish::save_config(registry.control(), hosting.secrets(), tenant.id, payload).await?,
    ))
}

/// Publish: build the site's pages again from what is in the CMS now.
#[utoipa::path(
    post,
    path = "/publish",
    tag = "publish",
    responses(
        (status = 202, description = "A build is queued", body = Build),
        (status = 400, description = "The site has no project to build", body = crate::error::ErrorBody),
    )
)]
pub async fn request_publish(
    State(hosting): State<Hosting>,
    Extension(resolved): Extension<Resolved>,
) -> AppResult<(StatusCode, Json<Build>)> {
    let tenant = site(&resolved)?;
    let build = publish::request(hosting.registry()?.control(), tenant.id).await?;

    Ok((StatusCode::ACCEPTED, Json(build)))
}

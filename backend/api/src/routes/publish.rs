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
}

/// How many past builds the panel shows. Enough to see a pattern, not enough
/// to be a log viewer.
const HISTORY: u8 = 10;

/// The site this request is for, which publishing is always about.
///
/// The server's own installation has no site to publish: it is the panel, and
/// its pages are this application.
fn tenant_id(resolved: &Resolved) -> AppResult<uuid::Uuid> {
    match resolved {
        Resolved::Tenant(tenant) => Ok(tenant.id),
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
    let id = tenant_id(&resolved)?;
    let db = hosting.registry()?.control();

    Ok(Json(PublishStatus {
        config: publish::config(db, id).await?,
        builds: publish::latest(db, id, HISTORY).await?,
    }))
}

/// Say where this site's pages come from.
#[utoipa::path(
    put,
    path = "/publish",
    tag = "publish",
    request_body = SaveBuildConfig,
    responses(
        (status = 200, description = "Saved", body = BuildConfig),
        (status = 400, description = "Not a repository this can build", body = crate::error::ErrorBody),
    )
)]
pub async fn save_publish(
    State(hosting): State<Hosting>,
    Extension(resolved): Extension<Resolved>,
    Json(payload): Json<SaveBuildConfig>,
) -> AppResult<Json<BuildConfig>> {
    let id = tenant_id(&resolved)?;
    let registry = hosting.registry()?;

    Ok(Json(
        publish::save_config(registry.control(), hosting.secrets(), id, payload).await?,
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
    let id = tenant_id(&resolved)?;
    let build = publish::request(hosting.registry()?.control(), id).await?;

    Ok((StatusCode::ACCEPTED, Json(build)))
}

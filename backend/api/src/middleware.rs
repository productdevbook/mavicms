use axum::{
    extract::Request,
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::{error::AppError, tenants::Site};

/// Rejects requests with 503 until the setup wizard has configured a
/// database. Applied to every route except health checks and `/setup/*`.
pub async fn require_database(Site(state): Site, request: Request, next: Next) -> Response {
    if state.db.is_none() {
        return AppError::Unavailable("database is not configured yet".to_string()).into_response();
    }
    next.run(request).await
}

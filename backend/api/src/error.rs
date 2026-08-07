use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0} not found")]
    NotFound(String),
    // No prefix: the sentence is written to be read by whoever is looking at
    // the panel, and "validation error: that is not an email address" is two
    // sentences where one was meant. The status code already says which kind
    // of refusal it is.
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    Conflict(String),
    #[error("database error: {0}")]
    Database(sea_orm::DbErr),
    #[error("{0}")]
    Unavailable(String),
    #[error("internal error: {0}")]
    Internal(String),
    #[error("{0}")]
    Unauthorized(String),
    #[error("{0}")]
    Forbidden(String),
    /// Too many wrong answers. Carries the seconds to wait, which goes out as
    /// `Retry-After` as well as in the sentence — a person reads one and a
    /// client reads the other.
    #[error("too many attempts — wait {0} seconds and try again")]
    TooManyRequests(u64),
}

#[derive(Serialize, ToSchema)]
pub struct ErrorBody {
    pub error: String,
}

impl From<sea_orm::DbErr> for AppError {
    /// A unique-constraint violation is a client problem (duplicate slug,
    /// duplicate tag), not a server fault — surfacing it as 500 is both wrong
    /// and unactionable. sqlx classifies it per-driver, so this works the same
    /// on SQLite, Postgres and MySQL.
    fn from(err: sea_orm::DbErr) -> Self {
        use sea_orm::{DbErr, RuntimeErr};

        let runtime = match &err {
            DbErr::Exec(runtime) | DbErr::Query(runtime) | DbErr::Conn(runtime) => Some(runtime),
            _ => None,
        };

        if let Some(RuntimeErr::SqlxError(sqlx_err)) = runtime
            && sqlx_err
                .as_database_error()
                .is_some_and(|db| db.is_unique_violation())
        {
            return AppError::Conflict("that value is already taken".to_string());
        }

        AppError::Database(err)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match &self {
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::Validation(_) => StatusCode::BAD_REQUEST,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            AppError::Forbidden(_) => StatusCode::FORBIDDEN,
            AppError::TooManyRequests(_) => StatusCode::TOO_MANY_REQUESTS,
        };

        if let AppError::Database(err) = &self {
            tracing::error!(error = %err, "database error");
        }
        if let AppError::Internal(err) = &self {
            tracing::error!(error = %err, "internal error");
        }

        let retry_after = match &self {
            AppError::TooManyRequests(seconds) => Some(*seconds),
            _ => None,
        };

        let mut response = (
            status,
            Json(ErrorBody {
                error: self.to_string(),
            }),
        )
            .into_response();

        if let Some(seconds) = retry_after
            && let Ok(value) = seconds.to_string().parse()
        {
            response.headers_mut().insert("retry-after", value);
        }

        response
    }
}

pub type AppResult<T> = Result<T, AppError>;

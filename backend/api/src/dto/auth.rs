use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserResponse {
    pub username: String,
    pub email: String,
    pub role: String,
    /// Whether this account is administering the server itself rather than one
    /// of the sites it hosts — which is what decides whether the panel offers
    /// to manage sites at all.
    pub operator: bool,
}

use serde::Serialize;
use utoipa::ToSchema;

/// A key this site has out, as the panel lists it. The key itself is not here:
/// it is shown once, when it is made, and not stored anywhere it can be read
/// back.
#[derive(Debug, Serialize, ToSchema)]
pub struct AssistantKey {
    pub id: String,
    pub created_at: String,
    pub expires_at: String,
}

/// A key and the document to paste it with.
#[derive(Debug, Serialize, ToSchema)]
pub struct Handover {
    /// So the panel can offer to take this one back without listing again.
    pub id: String,
    pub token: String,
    pub expires_at: String,
    /// The whole thing, key first. What the copy button puts on the clipboard.
    pub text: String,
}

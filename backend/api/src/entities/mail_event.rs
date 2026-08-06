use sea_orm::entity::prelude::*;

/// What Amazon said happened to a message.
///
/// `campaign_events` is what this program did; this is what the world did
/// back. A bounce arrives here minutes after the send, whether the message
/// was a campaign or a form notification.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "mail_events")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    /// send, delivery, bounce, complaint, reject, open, click, delivery_delay,
    /// rendering_failure or subscription.
    pub kind: String,
    pub address: String,
    /// Amazon's id, so two events about one message can be put together.
    pub message_id: String,
    /// From the tags put on the send, when it was one of ours to tag.
    pub campaign_id: Option<Uuid>,
    pub subscriber_id: Option<Uuid>,
    #[sea_orm(column_type = "Text")]
    pub detail: String,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

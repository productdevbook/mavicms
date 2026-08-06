use sea_orm::entity::prelude::*;

/// What happened to one message: that it went, that it did not, that somebody
/// opened it or followed a link in it.
///
/// The "sent" rows are also how a resumed campaign knows where it was. A
/// unique index on (campaign, subscriber, kind) is what stops anybody being
/// written to twice.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "campaign_events")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub campaign_id: Uuid,
    pub subscriber_id: Uuid,
    /// sent, failed, open or click.
    pub kind: String,
    #[sea_orm(column_type = "Text")]
    pub detail: String,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

use sea_orm_migration::prelude::*;

/// What Amazon tells us happened to each message.
///
// Separate from `campaign_events`, which is what this program did: this is
// what the world did back. A delivery, a bounce, a complaint and a delay all
// arrive here whether the message was a campaign or a form notification.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(MailEvents::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(MailEvents::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    // send, delivery, bounce, complaint, reject, open, click,
                    // delivery_delay, rendering_failure, subscription.
                    .col(ColumnDef::new(MailEvents::Kind).string_len(30).not_null())
                    .col(
                        ColumnDef::new(MailEvents::Address)
                            .string_len(320)
                            .not_null(),
                    )
                    // Amazon's id for the message, so two events about one
                    // message can be put together.
                    .col(
                        ColumnDef::new(MailEvents::MessageId)
                            .string_len(120)
                            .not_null(),
                    )
                    // From the tags this program puts on a send, when it was
                    // one of ours to tag.
                    .col(ColumnDef::new(MailEvents::CampaignId).uuid())
                    .col(ColumnDef::new(MailEvents::SubscriberId).uuid())
                    // Why, or where a click went.
                    .col(ColumnDef::new(MailEvents::Detail).text().not_null())
                    .col(
                        ColumnDef::new(MailEvents::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx-mail_events-created_at")
                    .table(MailEvents::Table)
                    .col(MailEvents::CreatedAt)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx-mail_events-kind")
                    .table(MailEvents::Table)
                    .col(MailEvents::Kind)
                    .col(MailEvents::CreatedAt)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(MailEvents::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum MailEvents {
    Table,
    Id,
    Kind,
    Address,
    MessageId,
    CampaignId,
    SubscriberId,
    Detail,
    CreatedAt,
}

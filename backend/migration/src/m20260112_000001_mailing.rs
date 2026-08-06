use sea_orm_migration::prelude::*;

/// Everything a site needs to keep a mailing list and send to it.
///
/// One migration because it is one feature: a campaign without lists is not
/// half a feature, it is a table nobody can use.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let timestamp = |name: MailLists| ColumnDef::new(name).timestamp_with_time_zone().not_null();

        manager
            .create_table(
                Table::create()
                    .table(MailLists::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(MailLists::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(MailLists::Name).text().not_null())
                    .col(ColumnDef::new(MailLists::Slug).string_len(255).not_null())
                    .col(ColumnDef::new(MailLists::Description).text().not_null())
                    // "single" or "double". Double asks the subscriber to
                    // confirm by mail before anything else is sent to them.
                    .col(ColumnDef::new(MailLists::OptIn).string_len(10).not_null())
                    /// Whether somebody may join it from the site itself.
                    .col(
                        ColumnDef::new(MailLists::Public)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(timestamp(MailLists::CreatedAt))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx-mail_lists-slug")
                    .table(MailLists::Table)
                    .col(MailLists::Slug)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Subscribers::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Subscribers::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Subscribers::Email).string_len(320).not_null())
                    .col(ColumnDef::new(Subscribers::Name).text().not_null())
                    // "enabled" or "blocked". Blocked is what a bounce or a
                    // spam complaint leaves behind, and it outranks every
                    // list they are on.
                    .col(
                        ColumnDef::new(Subscribers::Status)
                            .string_len(20)
                            .not_null(),
                    )
                    /// Whatever else is known about them, as JSON — a town, an
                    /// order number, whatever the site collected.
                    .col(ColumnDef::new(Subscribers::Attributes).text().not_null())
                    .col(
                        ColumnDef::new(Subscribers::CreatedAt)
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
                    .name("idx-subscribers-email")
                    .table(Subscribers::Table)
                    .col(Subscribers::Email)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(SubscriberLists::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(SubscriberLists::SubscriberId)
                            .uuid()
                            .not_null(),
                    )
                    .col(ColumnDef::new(SubscriberLists::ListId).uuid().not_null())
                    // "unconfirmed", "confirmed" or "unsubscribed". Kept per
                    // list rather than per person: leaving the newsletter is
                    // not leaving the order notifications.
                    .col(
                        ColumnDef::new(SubscriberLists::Status)
                            .string_len(20)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(SubscriberLists::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .primary_key(
                        Index::create()
                            .col(SubscriberLists::SubscriberId)
                            .col(SubscriberLists::ListId),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx-subscriber_lists-list")
                    .table(SubscriberLists::Table)
                    .col(SubscriberLists::ListId)
                    .col(SubscriberLists::Status)
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(MailTemplates::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(MailTemplates::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(MailTemplates::Name).text().not_null())
                    .col(ColumnDef::new(MailTemplates::Subject).text().not_null())
                    /// The HTML around a campaign's own body, with
                    /// placeholders. See `mailing::render`.
                    .col(ColumnDef::new(MailTemplates::Body).text().not_null())
                    .col(
                        ColumnDef::new(MailTemplates::IsDefault)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(MailTemplates::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Campaigns::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Campaigns::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(Campaigns::Name).text().not_null())
                    .col(ColumnDef::new(Campaigns::Subject).text().not_null())
                    .col(ColumnDef::new(Campaigns::Body).text().not_null())
                    .col(ColumnDef::new(Campaigns::TemplateId).uuid())
                    // draft, scheduled, running, paused, finished, cancelled.
                    .col(ColumnDef::new(Campaigns::Status).string_len(20).not_null())
                    .col(ColumnDef::new(Campaigns::SendAt).timestamp_with_time_zone())
                    .col(ColumnDef::new(Campaigns::StartedAt).timestamp_with_time_zone())
                    .col(ColumnDef::new(Campaigns::FinishedAt).timestamp_with_time_zone())
                    /// How many it is meant to reach, counted when it starts.
                    .col(
                        ColumnDef::new(Campaigns::ToSend)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(ColumnDef::new(Campaigns::Sent).integer().not_null().default(0))
                    .col(
                        ColumnDef::new(Campaigns::Failed)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(Campaigns::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(CampaignLists::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(CampaignLists::CampaignId).uuid().not_null())
                    .col(ColumnDef::new(CampaignLists::ListId).uuid().not_null())
                    .primary_key(
                        Index::create()
                            .col(CampaignLists::CampaignId)
                            .col(CampaignLists::ListId),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(CampaignEvents::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(CampaignEvents::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(CampaignEvents::CampaignId).uuid().not_null())
                    .col(ColumnDef::new(CampaignEvents::SubscriberId).uuid().not_null())
                    // sent, failed, open, click.
                    .col(ColumnDef::new(CampaignEvents::Kind).string_len(20).not_null())
                    /// Where a click went, or what a failure said.
                    .col(ColumnDef::new(CampaignEvents::Detail).text().not_null())
                    .col(
                        ColumnDef::new(CampaignEvents::CreatedAt)
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
                    .name("idx-campaign_events-campaign")
                    .table(CampaignEvents::Table)
                    .col(CampaignEvents::CampaignId)
                    .col(CampaignEvents::Kind)
                    .to_owned(),
            )
            .await?;

        // Sent once per campaign per subscriber. The unique index is what
        // makes a resumed campaign not write to anybody twice: the worker
        // stops on a duplicate rather than counting on having remembered
        // where it was.
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx-campaign_events-once")
                    .table(CampaignEvents::Table)
                    .col(CampaignEvents::CampaignId)
                    .col(CampaignEvents::SubscriberId)
                    .col(CampaignEvents::Kind)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(EmailLog::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(EmailLog::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(EmailLog::ToAddress).string_len(320).not_null())
                    .col(ColumnDef::new(EmailLog::Subject).text().not_null())
                    // "sent" or "failed".
                    .col(ColumnDef::new(EmailLog::Status).string_len(20).not_null())
                    .col(ColumnDef::new(EmailLog::Detail).text().not_null())
                    .col(
                        ColumnDef::new(EmailLog::CreatedAt)
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
                    .name("idx-email_log-created_at")
                    .table(EmailLog::Table)
                    .col(EmailLog::CreatedAt)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for table in [
            Table::drop().table(EmailLog::Table).to_owned(),
            Table::drop().table(CampaignEvents::Table).to_owned(),
            Table::drop().table(CampaignLists::Table).to_owned(),
            Table::drop().table(Campaigns::Table).to_owned(),
            Table::drop().table(MailTemplates::Table).to_owned(),
            Table::drop().table(SubscriberLists::Table).to_owned(),
            Table::drop().table(Subscribers::Table).to_owned(),
            Table::drop().table(MailLists::Table).to_owned(),
        ] {
            manager.drop_table(table).await?;
        }
        Ok(())
    }
}

#[derive(DeriveIden)]
enum MailLists {
    Table,
    Id,
    Name,
    Slug,
    Description,
    OptIn,
    Public,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Subscribers {
    Table,
    Id,
    Email,
    Name,
    Status,
    Attributes,
    CreatedAt,
}

#[derive(DeriveIden)]
enum SubscriberLists {
    Table,
    SubscriberId,
    ListId,
    Status,
    CreatedAt,
}

#[derive(DeriveIden)]
enum MailTemplates {
    Table,
    Id,
    Name,
    Subject,
    Body,
    IsDefault,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Campaigns {
    Table,
    Id,
    Name,
    Subject,
    Body,
    TemplateId,
    Status,
    SendAt,
    StartedAt,
    FinishedAt,
    ToSend,
    Sent,
    Failed,
    CreatedAt,
}

#[derive(DeriveIden)]
enum CampaignLists {
    Table,
    CampaignId,
    ListId,
}

#[derive(DeriveIden)]
enum CampaignEvents {
    Table,
    Id,
    CampaignId,
    SubscriberId,
    Kind,
    Detail,
    CreatedAt,
}

#[derive(DeriveIden)]
enum EmailLog {
    Table,
    Id,
    ToAddress,
    Subject,
    Status,
    Detail,
    CreatedAt,
}

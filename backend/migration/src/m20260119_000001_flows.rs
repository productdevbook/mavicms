use sea_orm_migration::prelude::*;

/// Flows: something happened, so do these things.
///
/// A contact form that emails whoever answers it, a post going out that pings
/// a channel, a nightly request somewhere. Every site wants two or three of
/// these and none of them is worth its own feature, so a site says what should
/// happen and the server does it.
///
/// Four tables rather than one because a flow and a run of it are different
/// things: the flow is what somebody wrote, the run is what happened, and the
/// second is the only way to answer "did it work" — which is the question
/// automation is always asked and rarely able to answer.
///
/// Credentials are their own table for the same reason a site's S3 keys are:
/// one mail account is used by five flows, and rotating it should be one edit
/// rather than five.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Flows::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Flows::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(Flows::Name).text().not_null())
                    // "form.submitted", "post.published", "schedule", "webhook".
                    .col(ColumnDef::new(Flows::TriggerKind).string_len(40).not_null())
                    // Which form, which cron, which secret — the trigger's own
                    // settings, as JSON, because every kind wants different ones.
                    .col(ColumnDef::new(Flows::TriggerConfig).text().not_null())
                    .col(
                        ColumnDef::new(Flows::Enabled)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    // Set for a webhook trigger: the unguessable part of the
                    // address it answers on. A column rather than a value in
                    // the config so that finding the flow for an incoming
                    // request is an index lookup and not a scan.
                    .col(ColumnDef::new(Flows::WebhookKey).string_len(64).null())
                    .col(
                        ColumnDef::new(Flows::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Flows::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(FlowSteps::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(FlowSteps::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(FlowSteps::FlowId).uuid().not_null())
                    .col(ColumnDef::new(FlowSteps::Position).integer().not_null())
                    // "mail.send", "http.request", "branch".
                    .col(ColumnDef::new(FlowSteps::Action).string_len(40).not_null())
                    .col(ColumnDef::new(FlowSteps::Config).text().not_null())
                    // "stop" or "continue". A step that emails somebody and a
                    // step that pings a dashboard fail differently: one should
                    // stop the flow and the other should not.
                    .col(
                        ColumnDef::new(FlowSteps::OnError)
                            .string_len(20)
                            .not_null()
                            .default("stop"),
                    )
                    .col(
                        ColumnDef::new(FlowSteps::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(FlowRuns::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(FlowRuns::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(FlowRuns::FlowId).uuid().not_null())
                    // "queued", "running", "succeeded", "failed".
                    .col(ColumnDef::new(FlowRuns::Status).string_len(20).not_null())
                    // What set it off, as JSON. The steps read their values out
                    // of this, and it is also what makes a failed run worth
                    // keeping: it can be looked at, and run again.
                    .col(ColumnDef::new(FlowRuns::Trigger).text().not_null())
                    .col(ColumnDef::new(FlowRuns::Error).text().null())
                    // How many flows deep this is. A flow that publishes a post
                    // and a flow that runs when a post is published are a loop,
                    // and this is what ends it.
                    .col(
                        ColumnDef::new(FlowRuns::Depth)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(FlowRuns::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(FlowRuns::StartedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(FlowRuns::FinishedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(FlowRunSteps::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(FlowRunSteps::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(FlowRunSteps::RunId).uuid().not_null())
                    .col(ColumnDef::new(FlowRunSteps::Position).integer().not_null())
                    .col(
                        ColumnDef::new(FlowRunSteps::Action)
                            .string_len(40)
                            .not_null(),
                    )
                    // "succeeded", "failed", "skipped".
                    .col(
                        ColumnDef::new(FlowRunSteps::Status)
                            .string_len(20)
                            .not_null(),
                    )
                    // What the step produced, for the next step to read and for
                    // somebody to look at afterwards. Never a credential: what
                    // goes in here is written by the step, not copied from its
                    // settings.
                    .col(ColumnDef::new(FlowRunSteps::Output).text().not_null())
                    .col(ColumnDef::new(FlowRunSteps::Error).text().null())
                    .col(
                        ColumnDef::new(FlowRunSteps::FinishedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(FlowCredentials::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(FlowCredentials::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(FlowCredentials::Name).text().not_null())
                    // "smtp" for now. "gmail" when OAuth exists.
                    .col(
                        ColumnDef::new(FlowCredentials::Kind)
                            .string_len(40)
                            .not_null(),
                    )
                    // Encrypted with the site's key, like every other secret
                    // here: a copy of the database on its own is not a copy of
                    // anybody's mail account.
                    .col(ColumnDef::new(FlowCredentials::Secret).text().not_null())
                    .col(
                        ColumnDef::new(FlowCredentials::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        for (name, table, columns) in [
            (
                "idx-flow-steps-flow",
                "flow_steps",
                vec![
                    FlowSteps::FlowId.to_string(),
                    FlowSteps::Position.to_string(),
                ],
            ),
            (
                "idx-flow-runs-flow",
                "flow_runs",
                vec![
                    FlowRuns::FlowId.to_string(),
                    FlowRuns::CreatedAt.to_string(),
                ],
            ),
            (
                "idx-flow-runs-status",
                "flow_runs",
                vec![FlowRuns::Status.to_string()],
            ),
            (
                "idx-flow-run-steps-run",
                "flow_run_steps",
                vec![
                    FlowRunSteps::RunId.to_string(),
                    FlowRunSteps::Position.to_string(),
                ],
            ),
            (
                "idx-flows-webhook",
                "flows",
                vec![Flows::WebhookKey.to_string()],
            ),
        ] {
            if manager.has_index(table, name).await? {
                continue;
            }
            let mut index = Index::create();
            index.name(name).table(Alias::new(table));
            for column in columns {
                index.col(Alias::new(column));
            }
            manager.create_index(index.to_owned()).await?;
        }

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for table in [
            FlowRunSteps::Table.to_string(),
            FlowRuns::Table.to_string(),
            FlowSteps::Table.to_string(),
            FlowCredentials::Table.to_string(),
            Flows::Table.to_string(),
        ] {
            manager
                .drop_table(Table::drop().table(Alias::new(table)).to_owned())
                .await?;
        }
        Ok(())
    }
}

#[derive(DeriveIden)]
enum Flows {
    Table,
    Id,
    Name,
    TriggerKind,
    TriggerConfig,
    Enabled,
    WebhookKey,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum FlowSteps {
    Table,
    Id,
    FlowId,
    Position,
    Action,
    Config,
    OnError,
    CreatedAt,
}

#[derive(DeriveIden)]
enum FlowRuns {
    Table,
    Id,
    FlowId,
    Status,
    Trigger,
    Error,
    Depth,
    CreatedAt,
    StartedAt,
    FinishedAt,
}

#[derive(DeriveIden)]
enum FlowRunSteps {
    Table,
    Id,
    RunId,
    Position,
    Action,
    Status,
    Output,
    Error,
    FinishedAt,
}

#[derive(DeriveIden)]
enum FlowCredentials {
    Table,
    Id,
    Name,
    Kind,
    Secret,
    CreatedAt,
}

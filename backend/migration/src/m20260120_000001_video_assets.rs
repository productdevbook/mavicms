use sea_orm_migration::prelude::*;

/// Somewhere to keep what a site has uploaded, without keeping the video.
///
/// The host is on the row rather than read from the plugin settings when it is
/// needed. Those settings hold one host; a site that moves from one to the
/// other would otherwise lose the ability to play, or even to delete, every
/// video it uploaded before the move — the id would be looked up against the
/// wrong account and simply not be found.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(VideoAssets::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(VideoAssets::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(VideoAssets::Host).string_len(40).not_null())
                    .col(ColumnDef::new(VideoAssets::HostId).text().not_null())
                    .col(ColumnDef::new(VideoAssets::Title).text().not_null())
                    .col(
                        ColumnDef::new(VideoAssets::Status)
                            .string_len(20)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(VideoAssets::DurationSeconds)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(ColumnDef::new(VideoAssets::ThumbnailUrl).text().not_null())
                    .col(
                        ColumnDef::new(VideoAssets::SizeBytes)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(ColumnDef::new(VideoAssets::Error).text().not_null())
                    .col(
                        ColumnDef::new(VideoAssets::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(VideoAssets::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        // The webhook arrives knowing the host's id and nothing else, so this
        // is the lookup on the one path where a slow query would mean a
        // transcode that finished and a panel that never noticed.
        if !manager
            .has_index("video_assets", "idx-video-assets-host-id")
            .await?
        {
            manager
                .create_index(
                    Index::create()
                        .name("idx-video-assets-host-id")
                        .table(VideoAssets::Table)
                        .col(VideoAssets::HostId)
                        .to_owned(),
                )
                .await?;
        }

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(VideoAssets::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum VideoAssets {
    Table,
    Id,
    Host,
    HostId,
    Title,
    Status,
    DurationSeconds,
    ThumbnailUrl,
    SizeBytes,
    Error,
    CreatedAt,
    UpdatedAt,
}

use sea_orm_migration::prelude::*;

/// What a site needs to let somebody connect an assistant to it without
/// handing it a password.
///
/// Three tables and no more: the programs that have asked to connect, the
/// short-lived codes handed out mid-flow, and the connections that came of
/// them. The access token itself is not here — it is a row in `sessions`, the
/// same as every other way into this site, so that one place decides who a
/// caller is and one place takes it away again.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(OauthClients::Table)
                    .if_not_exists()
                    // The client registers itself and is given this. Public
                    // clients only: an assistant on somebody's laptop cannot
                    // keep a secret, so it is not issued one and PKCE is what
                    // proves the code came back to whoever asked for it.
                    .col(
                        ColumnDef::new(OauthClients::Id)
                            .string_len(64)
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(OauthClients::Name).text().not_null())
                    // Newline-separated. Only these may be redirected to.
                    .col(ColumnDef::new(OauthClients::RedirectUris).text().not_null())
                    .col(
                        ColumnDef::new(OauthClients::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(OauthCodes::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(OauthCodes::Code)
                            .string_len(64)
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(OauthCodes::ClientId)
                            .string_len(64)
                            .not_null(),
                    )
                    .col(ColumnDef::new(OauthCodes::UserId).uuid().not_null())
                    .col(ColumnDef::new(OauthCodes::RedirectUri).text().not_null())
                    // The S256 challenge. The code is only good to whoever can
                    // produce the verifier behind it.
                    .col(ColumnDef::new(OauthCodes::Challenge).text().not_null())
                    .col(
                        ColumnDef::new(OauthCodes::ExpiresAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(OauthGrants::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(OauthGrants::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(OauthGrants::ClientId)
                            .string_len(64)
                            .not_null(),
                    )
                    .col(ColumnDef::new(OauthGrants::UserId).uuid().not_null())
                    // The session handed out as the access token. Revoking the
                    // connection deletes it, and the connection is then gone
                    // everywhere at once.
                    .col(ColumnDef::new(OauthGrants::SessionId).uuid().not_null())
                    // Rotated on every refresh, so a stolen one stops working
                    // the moment the real client next asks.
                    .col(
                        ColumnDef::new(OauthGrants::Refresh)
                            .string_len(64)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(OauthGrants::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(OauthGrants::UsedAt)
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
                    .name("idx-oauth_grants-refresh")
                    .table(OauthGrants::Table)
                    .col(OauthGrants::Refresh)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(OauthGrants::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(OauthCodes::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(OauthClients::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum OauthClients {
    Table,
    Id,
    Name,
    RedirectUris,
    CreatedAt,
}

#[derive(DeriveIden)]
enum OauthCodes {
    Table,
    Code,
    ClientId,
    UserId,
    RedirectUri,
    Challenge,
    ExpiresAt,
}

#[derive(DeriveIden)]
enum OauthGrants {
    Table,
    Id,
    ClientId,
    UserId,
    SessionId,
    Refresh,
    CreatedAt,
    UsedAt,
}

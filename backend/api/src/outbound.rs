//! Which account a site's mail leaves by, and how much of it may.
//!
//! A site with its own Amazon keys sends with them and nothing here applies.
//! A site the server lends its account to sends as its own Amazon tenant, and
//! is held to a number of messages a day — because a shared account is only
//! shared until somebody empties it.

use chrono::Utc;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter};

use crate::{
    email::EmailConfig,
    error::{AppError, AppResult},
    platform::{self, Sends},
    state::AppState,
};

/// How a site is going to send this message.
pub struct Post {
    pub config: EmailConfig,
    /// Messages a day, when the account is the server's. `None` when the site
    /// sends with its own, which the server does not meter.
    pub a_day: Option<i64>,
    /// Whether the account belongs to the site.
    pub own: bool,
    /// Whether the mail still goes out under the server's own address because
    /// the site has not named one of its own. True is a working state and a
    /// temporary one: the site's name is not on its own mail yet.
    pub as_the_server: bool,
}

/// Works out which account a site sends with.
///
/// Its own comes first: a site that has gone to the trouble of putting its own
/// keys in meant to use them, and the server's account is the fallback rather
/// than an override.
pub async fn how(state: &AppState) -> AppResult<Post> {
    let db = state.db_or_unavailable()?;
    let (Some(control), Some(secrets), Some(tenant_id)) = (
        state.control.as_ref(),
        state.control_secrets.as_ref(),
        state.tenant_id,
    ) else {
        // The server's own installation. It has nobody to borrow from, so its
        // own settings are the only answer.
        return own_only(db, &state.secrets).await;
    };
    how_for(db, &state.secrets, control, secrets, tenant_id).await
}

async fn own_only(db: &DatabaseConnection, secrets: &crate::crypto::SecretBox) -> AppResult<Post> {
    let stored =
        crate::plugins::load::<EmailConfig>(db, secrets, crate::plugins::EMAIL_PLUGIN).await?;
    match stored {
        Some(stored) if stored.enabled && !stored.config.access_key_id.trim().is_empty() => {
            Ok(Post {
                config: stored.config,
                a_day: None,
                own: true,
                as_the_server: false,
            })
        }
        _ => Err(AppError::Validation(
            "this site has no mail settings yet".to_string(),
        )),
    }
}

/// The same question, asked with the pieces rather than with a site's state —
/// which is what a background worker has.
pub async fn how_for(
    db: &DatabaseConnection,
    site_secrets: &crate::crypto::SecretBox,
    control: &DatabaseConnection,
    control_secrets: &crate::crypto::SecretBox,
    tenant_id: uuid::Uuid,
) -> AppResult<Post> {
    if let Ok(own) = own_only(db, site_secrets).await {
        return Ok(own);
    }

    let secrets = control_secrets;

    let allowed = platform::allowance(control, tenant_id)
        .await?
        .ok_or_else(|| {
            AppError::Validation(
                "this site has no mail settings, and the server has not lent it an account"
                    .to_string(),
            )
        })?;
    if Sends::read(&allowed.sends) != Sends::Shared {
        return Err(AppError::Validation(
            "this site has no mail settings yet".to_string(),
        ));
    }

    let mut config = platform::ses(control, secrets)
        .await?
        .ok_or_else(|| AppError::Validation("the server has no mail account set up".to_string()))?;

    // Whose mail this is. Without it every site's sending would be judged
    // together, which is the thing a shared account cannot afford.
    config.tenant = allowed.ses_tenant.clone().ok_or_else(|| {
        AppError::Validation(
            "this site is allowed to send but has not been given an Amazon tenant yet".to_string(),
        )
    })?;

    // The server lends the account, not the name on the letter. Without this
    // every site the server sends for would appear to be the server, and a
    // reply to a customer's contact form would go to whoever runs the machine.
    let as_the_server = !overlay_sender(db, site_secrets, &mut config).await?;

    Ok(Post {
        config,
        a_day: Some(allowed.a_day),
        own: false,
        as_the_server,
    })
}

/// Puts the site's own sender on the server's account. Says whether it had one.
///
/// A site that has named no sender still sends — under the server's address,
/// with replies pointed back at the site — because a contact form that fails
/// silently until somebody edits DNS is worse than one that works and says so.
async fn overlay_sender(
    db: &DatabaseConnection,
    secrets: &crate::crypto::SecretBox,
    config: &mut EmailConfig,
) -> AppResult<bool> {
    let mine = crate::plugins::load::<EmailConfig>(db, secrets, crate::plugins::EMAIL_PLUGIN)
        .await?
        .map(|stored| stored.config)
        .unwrap_or_default();

    if mine.from_address.trim().is_empty() {
        // Nothing to send as. Keep the server's address, but the name in front
        // of it is the site's: whoever reads this knows the business, not the
        // machine it happens to be hosted on.
        if let Some(name) = whose(db, &mine).await? {
            config.from_name = name;
        }
        if !mine.reply_to.trim().is_empty() {
            config.reply_to = mine.reply_to;
        }
        return Ok(false);
    }

    config.from_address = mine.from_address;
    if !mine.from_name.trim().is_empty() {
        config.from_name = mine.from_name;
    }
    config.reply_to = mine.reply_to;
    config.senders = mine.senders;
    Ok(true)
}

/// What to call a site that has not said. Its own title, or nothing.
async fn whose(db: &DatabaseConnection, mine: &EmailConfig) -> AppResult<Option<String>> {
    if !mine.from_name.trim().is_empty() {
        return Ok(Some(mine.from_name.clone()));
    }
    Ok(crate::entities::site_settings::Entity::find()
        .one(db)
        .await?
        .map(|settings| settings.site_title)
        .filter(|title| !title.trim().is_empty()))
}

/// How many messages this site has sent since midnight, whatever asked for
/// them — a contact form, a campaign, a flow. Counted from the site's own log
/// because that is the one record every send passes through.
pub async fn sent_today(db: &DatabaseConnection) -> AppResult<u64> {
    let midnight = Utc::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap_or_default()
        .and_utc()
        .fixed_offset();

    Ok(crate::entities::email_log::Entity::find()
        .filter(crate::entities::email_log::Column::CreatedAt.gte(midnight))
        .filter(crate::entities::email_log::Column::Status.eq(crate::mailing::SENT))
        .count(db)
        .await?)
}

/// Refuses before anything is sent, and says the number rather than "no".
///
/// Counted as a whole rather than per message: a campaign asking to send four
/// hundred when eighty are left should be stopped at the door, not halfway
/// down the list with two hundred people wondering why they got half a
/// newsletter.
pub async fn may_send(post: &Post, db: &DatabaseConnection, wanted: u64) -> AppResult<()> {
    let Some(a_day) = post.a_day else {
        return Ok(());
    };
    if a_day <= 0 {
        return Err(AppError::Validation(
            "sending is switched off for this site".to_string(),
        ));
    }

    let already = sent_today(db).await?;
    let left = (a_day as u64).saturating_sub(already);
    if wanted > left {
        return Err(AppError::Validation(format!(
            "this site may send {a_day} messages a day through the server's account and has \
             {left} left today"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Post;
    use crate::email::EmailConfig;

    fn shared(a_day: i64) -> Post {
        Post {
            config: EmailConfig::default(),
            a_day: Some(a_day),
            own: false,
            as_the_server: false,
        }
    }

    #[tokio::test]
    async fn a_site_with_its_own_account_is_not_metered() {
        let db = sea_orm::Database::connect("sqlite::memory:").await.unwrap();
        use migration::{Migrator, MigratorTrait};
        Migrator::up(&db, None).await.unwrap();

        let own = Post {
            config: EmailConfig::default(),
            a_day: None,
            own: true,
            as_the_server: false,
        };
        // A number no shared site would be given, to show the ceiling is not
        // merely large but absent.
        assert!(super::may_send(&own, &db, 1_000_000).await.is_ok());
    }

    #[tokio::test]
    async fn a_whole_campaign_is_refused_rather_than_half_sent() {
        let db = sea_orm::Database::connect("sqlite::memory:").await.unwrap();
        use migration::{Migrator, MigratorTrait};
        Migrator::up(&db, None).await.unwrap();

        assert!(super::may_send(&shared(200), &db, 200).await.is_ok());
        let refused = super::may_send(&shared(200), &db, 201).await;
        assert!(refused.is_err());
        // The number, not "no": somebody reading this has to know what to ask
        // for.
        assert!(refused.unwrap_err().to_string().contains("200"));
    }

    #[tokio::test]
    async fn nothing_at_all_is_a_site_that_has_been_stopped() {
        let db = sea_orm::Database::connect("sqlite::memory:").await.unwrap();
        use migration::{Migrator, MigratorTrait};
        Migrator::up(&db, None).await.unwrap();

        assert!(super::may_send(&shared(0), &db, 1).await.is_err());
    }
}

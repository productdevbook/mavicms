use axum::{
    extract::Request,
    http::Method,
    middleware::Next,
    response::{IntoResponse, Response},
};
use chrono::{Duration, Utc};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection, EntityTrait};
use tower_cookies::{Cookie, Cookies, cookie::SameSite};
use uuid::Uuid;

use crate::{
    entities::{session, user},
    error::AppError,
    state::AppState,
    tenants::Site,
};

pub const SESSION_COOKIE: &str = "mavicms_session";
const SESSION_TTL_DAYS: i64 = 30;

/// Creates a session row for `user_id` and sets the session cookie. Used by
/// both `/login` and the setup wizard's final step (which signs the newly
/// created administrator in immediately).
pub async fn create_session(
    db: &DatabaseConnection,
    cookies: &Cookies,
    user_id: Uuid,
) -> Result<(), AppError> {
    let now = Utc::now().fixed_offset();
    let id = Uuid::now_v7();

    let record = session::ActiveModel {
        id: Set(id),
        user_id: Set(user_id),
        expires_at: Set((Utc::now() + Duration::days(SESSION_TTL_DAYS)).fixed_offset()),
        created_at: Set(now),
    };
    record.insert(db).await?;

    let mut cookie = Cookie::new(SESSION_COOKIE, id.to_string());
    cookie.set_http_only(true);
    cookie.set_secure(true);
    cookie.set_same_site(SameSite::Lax);
    cookie.set_path("/");
    cookies.add(cookie);
    Ok(())
}

/// Clears the session cookie and, best-effort, its backing row. Never fails
/// — logging out with no active session is a no-op, not an error.
pub async fn clear_session(state: &AppState, cookies: &Cookies) {
    if let Some(cookie) = cookies.get(SESSION_COOKIE)
        && let (Some(db), Ok(id)) = (state.db.as_ref(), Uuid::parse_str(cookie.value()))
    {
        let _ = session::Entity::delete_by_id(id).exec(db).await;
    }

    let mut removal = Cookie::new(SESSION_COOKIE, "");
    removal.set_path("/");
    cookies.remove(removal);
}

/// Rejects requests with 401 unless a valid, unexpired session cookie is
/// present, and makes the signed-in user available to handlers via
/// `Extension<user::Model>`.
pub async fn require_auth(
    Site(state): Site,
    cookies: Cookies,
    mut request: Request,
    next: Next,
) -> Response {
    let bearer = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::to_string);

    match authenticate(&state, &cookies, bearer.as_deref()).await {
        Ok(user) => {
            // A build reads; it does not write. The token exists so that a
            // build does not need somebody's password, and a read-only token
            // that can still delete posts would not have been worth having.
            if user.role == BUILDER && !matches!(*request.method(), Method::GET | Method::HEAD) {
                return AppError::Forbidden(
                    "a build token can read this site and nothing else".to_string(),
                )
                .into_response();
            }

            if user.role == ASSISTANT
                && let Some(why) = off_limits(request.uri().path(), request.method())
            {
                return AppError::Forbidden(why.to_string()).into_response();
            }

            request.extensions_mut().insert(user);
            next.run(request).await
        }
        Err(err) => err.into_response(),
    }
}

/// The role a build runs as. It reads the site to build its pages and has no
/// reason to change anything.
pub const BUILDER: &str = "builder";

/// The role a day-long assistant key runs as.
///
/// It writes — that is the whole point of it, and a key that could not would
/// be the read-only one this site already had. What it may not do is outlive
/// itself or reach past the site, which is what [`off_limits`] settles.
pub const ASSISTANT: &str = "assistant";

/// Why an assistant key is being refused this address, or `None` to let it by.
///
/// A key like this is pasted into somebody else's program. Two things follow
/// from that. It must not be able to hand out a second credential, or change
/// a password, or read the keys to an S3 bucket and a mail account — take any
/// of those and the day it lasts stops meaning anything. And it must not be
/// able to destroy something there is no way back from: a deleted post waits
/// thirty days in the bin, a purged one does not, and a campaign that has gone
/// out has gone out.
fn off_limits(path: &str, method: &Method) -> Option<&'static str> {
    // Prefix rather than exact, because each of these has addresses under it,
    // and a new one under any of them should be refused the day it is written
    // rather than the day somebody remembers this list exists.
    const ACCOUNTS: &str = "an assistant key cannot reach the accounts on this site or the \
                            invitations to it. It lasts a day, and an account made with it \
                            would last longer than that.";
    const CREDENTIALS: &str = "an assistant key cannot make another key. Ask whoever gave you \
                               this one.";
    const SECRETS: &str = "an assistant key cannot reach the storage and mail settings or the \
                           backups: those hold credentials for other services, and a backup \
                           holds the whole database.";
    const ELSEWHERE: &str = "an assistant key works on the site it was made for, and not on the \
                             others on this server.";

    const SEALED: [(&str, &str); 11] = [
        ("/users", ACCOUNTS),
        ("/me/password", ACCOUNTS),
        ("/invites", ACCOUNTS),
        ("/agencies", ACCOUNTS),
        ("/assistant", CREDENTIALS),
        ("/development/tokens", CREDENTIALS),
        ("/oauth", CREDENTIALS),
        ("/plugins", SECRETS),
        ("/sites", ELSEWHERE),
        ("/enter", ELSEWHERE),
        ("/console", ELSEWHERE),
    ];

    if let Some((_, why)) = SEALED.iter().find(|(prefix, _)| under(path, prefix)) {
        return Some(why);
    }

    // Bins a post, and it can be taken back out for thirty days: allowed.
    // Empties the bin, and it cannot: not.
    if method == Method::DELETE && path.starts_with("/trash/") {
        return Some(
            "an assistant key can put something in the bin, where it waits thirty days, but not \
             empty it. Deleting the post itself does what was meant here.",
        );
    }

    // Writing the campaign is the work; sending it is the part that cannot be
    // taken back, and it goes to everyone at once.
    if path.starts_with("/mail/campaigns/") && path.ends_with("/send") {
        return Some(
            "an assistant key can write a campaign but cannot send it. Somebody should look at \
             it first; send it from the panel.",
        );
    }

    None
}

/// `/plugins` covers `/plugins/s3` and does not cover `/pluginsomething`.
fn under(path: &str, prefix: &str) -> bool {
    path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with('/'))
}

pub(crate) async fn authenticate(
    state: &AppState,
    cookies: &Cookies,
    bearer: Option<&str>,
) -> Result<user::Model, AppError> {
    let db = state.db_or_unavailable()?;

    // A bearer token is the same session id by another route, for something
    // holding a token rather than a cookie jar — a build, chiefly.
    if let Some(token) = bearer
        && let Ok(session_id) = Uuid::parse_str(token)
    {
        return by_session(db, session_id).await;
    }

    let token = cookies
        .get(SESSION_COOKIE)
        .ok_or_else(|| AppError::Unauthorized("not signed in".to_string()))?;
    let session_id = Uuid::parse_str(token.value())
        .map_err(|_| AppError::Unauthorized("not signed in".to_string()))?;

    by_session(db, session_id).await
}

async fn by_session(db: &DatabaseConnection, session_id: Uuid) -> Result<user::Model, AppError> {
    let session_row = session::Entity::find_by_id(session_id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::Unauthorized("session expired".to_string()))?;

    if session_row.expires_at < Utc::now() {
        return Err(AppError::Unauthorized("session expired".to_string()));
    }

    user::Entity::find_by_id(session_row.user_id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::Unauthorized("session expired".to_string()))
}

/// The role every account has today, and the one the dangerous things ask for.
pub const ADMINISTRATOR: &str = "administrator";

/// Proof that the signed-in account is an administrator of this site.
///
/// Everyone is one today, so this changes nothing yet. It is here because the
/// things it guards — replacing every row a site has — are the things that
/// must not quietly become available to an editor the day editors exist.
pub struct Administrator(pub user::Model);

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for Administrator {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let user = parts
            .extensions
            .get::<user::Model>()
            .cloned()
            .ok_or_else(|| AppError::Unauthorized("not signed in".to_string()))?;

        // An assistant key passes here. Its limits are drawn at the door, in
        // [`off_limits`], and drawing them again in every place that asks for
        // an administrator would mean listing what it may do — which is nearly
        // everything a site is for, and would go stale by the next feature.
        if user.role != ADMINISTRATOR && user.role != ASSISTANT {
            return Err(AppError::Forbidden(
                "only an administrator of this site can do that".to_string(),
            ));
        }
        Ok(Administrator(user))
    }
}

#[cfg(test)]
mod tests {
    use axum::http::Method;

    use super::off_limits;

    #[test]
    fn an_assistant_key_may_do_the_work_it_was_made_for() {
        for (method, path) in [
            (Method::POST, "/posts"),
            (Method::PUT, "/posts/1"),
            (Method::DELETE, "/posts/1"),
            (Method::POST, "/media"),
            (Method::POST, "/forms"),
            (Method::PUT, "/forms/1"),
            (Method::POST, "/content-types"),
            (Method::POST, "/categories"),
            (Method::POST, "/languages"),
            (Method::POST, "/publish"),
            (Method::POST, "/trash/1/restore"),
            (Method::POST, "/mail/campaigns"),
            (Method::GET, "/llms.txt"),
        ] {
            assert!(off_limits(path, &method).is_none(), "{method} {path}");
        }
    }

    #[test]
    fn it_may_not_hand_itself_something_that_outlives_it() {
        for path in [
            "/users",
            "/users/1",
            "/me/password",
            "/invites",
            "/agencies/1",
            "/enter",
            "/assistant/tokens",
            "/assistant/handover",
            "/development/tokens",
            "/development/tokens/1",
            "/oauth/connections",
            "/plugins",
            "/plugins/s3",
            "/plugins/backup/run",
            "/sites",
        ] {
            assert!(off_limits(path, &Method::POST).is_some(), "{path}");
        }
    }

    /// Reading them is no better than writing them: the settings hold an S3
    /// key and an AWS key, and a backup holds every password hash on the site.
    #[test]
    fn reading_the_sealed_ones_is_refused_too() {
        assert!(off_limits("/plugins/s3", &Method::GET).is_some());
        assert!(off_limits("/users", &Method::GET).is_some());
    }

    #[test]
    fn a_prefix_is_a_path_segment_and_not_a_string() {
        assert!(off_limits("/pluginsomething", &Method::GET).is_none());
        assert!(off_limits("/sites-map", &Method::GET).is_none());
    }

    #[test]
    fn the_bin_may_be_filled_but_not_emptied() {
        assert!(off_limits("/trash", &Method::GET).is_none());
        assert!(off_limits("/trash/1/restore", &Method::POST).is_none());
        assert!(off_limits("/trash/1", &Method::DELETE).is_some());
    }

    #[test]
    fn a_campaign_may_be_written_but_not_sent() {
        assert!(off_limits("/mail/campaigns", &Method::POST).is_none());
        assert!(off_limits("/mail/campaigns/1", &Method::PUT).is_none());
        assert!(off_limits("/mail/campaigns/1/send", &Method::POST).is_some());
    }
}

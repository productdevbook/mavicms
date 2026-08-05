pub mod auth;
pub mod categories;
pub mod health;
pub mod languages;
pub mod media;
pub mod plugins;
pub mod posts;
pub mod setup;
pub mod tags;

use axum::{extract::DefaultBodyLimit, middleware::from_fn_with_state};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    auth::require_auth,
    middleware::require_database,
    state::AppState,
};

pub fn router(state: AppState) -> OpenApiRouter<AppState> {
    let public = OpenApiRouter::new()
        .routes(routes!(health::health))
        .routes(routes!(setup::setup_status))
        .routes(routes!(setup::run_setup))
        .routes(routes!(setup::configure_database))
        .routes(routes!(auth::logout));

    let needs_db = OpenApiRouter::new()
        .routes(routes!(auth::login))
        .layer(from_fn_with_state(state.clone(), require_database));

    let protected = OpenApiRouter::new()
        .routes(routes!(auth::me))
        .routes(routes!(posts::list_posts, posts::create_post))
        .routes(routes!(
            posts::get_post,
            posts::update_post,
            posts::delete_post
        ))
        .routes(routes!(posts::set_translation_group))
        .routes(routes!(categories::list_categories, categories::create_category))
        .routes(routes!(
            categories::update_category,
            categories::delete_category
        ))
        .routes(routes!(tags::list_tags, tags::create_tag))
        .routes(routes!(tags::delete_tag))
        .routes(routes!(tags::set_tag_translation_group))
        .routes(routes!(media::delete_media))
        .routes(routes!(languages::list_languages, languages::create_language))
        .routes(routes!(
            languages::update_language,
            languages::delete_language
        ))
        .routes(routes!(plugins::list_plugins))
        .routes(routes!(
            plugins::get_s3_settings,
            plugins::update_s3_settings
        ))
        .routes(routes!(plugins::test_s3_settings))
        .merge(
            OpenApiRouter::new()
                .routes(routes!(media::list_media, media::upload_media))
                .layer(DefaultBodyLimit::max(media::MAX_UPLOAD_BYTES)),
        )
        .layer(from_fn_with_state(state.clone(), require_auth))
        .layer(from_fn_with_state(state, require_database));

    public.merge(needs_db).merge(protected)
}

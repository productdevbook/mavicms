pub mod auth;
pub mod categories;
pub mod health;
pub mod languages;
pub mod media;
pub mod plugins;
pub mod posts;
pub mod setup;
pub mod console;
pub mod publish;
pub mod sites;
pub mod slug;
pub mod tags;

use axum::{
    extract::DefaultBodyLimit,
    middleware::{from_fn, from_fn_with_state},
};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    auth::require_auth,
    middleware::require_database,
    tenants::{Hosting, resolve},
};

pub fn router(hosting: Hosting) -> OpenApiRouter<Hosting> {
    let public = OpenApiRouter::new()
        .routes(routes!(health::health))
        .routes(routes!(setup::setup_status))
        .routes(routes!(setup::run_setup))
        .routes(routes!(setup::configure_database))
        .routes(routes!(auth::logout))
        .routes(routes!(console::register))
        .routes(routes!(console::login))
        .routes(routes!(console::logout))
        .routes(routes!(console::me))
        .routes(routes!(console::list_sites, console::create_site))
        .routes(routes!(console::create_entry))
        .routes(routes!(console::enter));

    let needs_db = OpenApiRouter::new()
        .routes(routes!(auth::login))
        .layer(from_fn(require_database));

    let protected = OpenApiRouter::new()
        .routes(routes!(auth::me))
        .routes(routes!(posts::list_posts, posts::create_post))
        .routes(routes!(
            posts::get_post,
            posts::update_post,
            posts::delete_post
        ))
        .routes(routes!(posts::set_translation_group))
        .routes(routes!(
            categories::list_categories,
            categories::create_category
        ))
        .routes(routes!(
            categories::update_category,
            categories::delete_category
        ))
        .routes(routes!(tags::list_tags, tags::create_tag))
        .routes(routes!(tags::delete_tag))
        .routes(routes!(tags::set_tag_translation_group))
        .routes(routes!(slug::make_slug))
        .routes(routes!(sites::list_sites, sites::create_site))
        .routes(routes!(media::delete_media))
        .routes(routes!(media::import_media))
        .routes(routes!(
            languages::list_languages,
            languages::create_language
        ))
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
        .routes(routes!(
            plugins::get_backup_settings,
            plugins::update_backup_settings
        ))
        .routes(routes!(plugins::run_backup))
        .routes(routes!(plugins::delete_backup))
        .routes(routes!(
            publish::get_publish,
            publish::save_publish,
            publish::request_publish
        ))
        .merge(
            OpenApiRouter::new()
                .routes(routes!(media::list_media, media::upload_media))
                .layer(DefaultBodyLimit::max(media::MAX_UPLOAD_BYTES)),
        )
        .layer(from_fn(require_auth))
        .layer(from_fn(require_database));

    // Resolving the site is the outermost thing that happens: everything
    // below it, authentication included, is a question about one site.
    public
        .merge(needs_db)
        .merge(protected)
        .layer(from_fn_with_state(hosting, resolve))
}

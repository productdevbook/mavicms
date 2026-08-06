pub mod auth;
pub mod categories;
pub mod console;
pub mod health;
pub mod languages;
pub mod media;
pub mod plugins;
pub mod posts;
pub mod publish;
pub mod setup;
pub mod sites;
pub mod slug;
pub mod tags;
pub mod users;

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
        .routes(routes!(console::update_account))
        .routes(routes!(console::list_sites, console::create_site))
        .routes(routes!(console::create_entry))
        .routes(routes!(
            console::get_site_publish,
            console::save_site_publish,
            console::request_site_publish
        ))
        .routes(routes!(console::get_site_s3, console::save_site_s3))
        .routes(routes!(console::get_site_backup, console::save_site_backup))
        .routes(routes!(console::run_site_backup))
        .routes(routes!(console::restore_site_backup))
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
        .routes(routes!(sites::update_site))
        .routes(routes!(sites::delete_site))
        .routes(routes!(console::list_agencies))
        .routes(routes!(console::update_agency))
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
        .routes(routes!(plugins::restore_backup))
        .routes(routes!(users::list_users, users::create_user))
        .routes(routes!(users::update_user, users::delete_user))
        .routes(routes!(users::change_own_password))
        .merge(
            OpenApiRouter::new()
                .routes(routes!(plugins::import_backup))
                // An archive with a site's media in it is not a small file.
                .layer(DefaultBodyLimit::max(crate::backup::MAX_IMPORT_BYTES)),
        )
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

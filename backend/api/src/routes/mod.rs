pub mod auth;
pub mod categories;
pub mod console;
pub mod content_types;
pub mod development;
pub mod forms;
pub mod health;
pub mod languages;
pub mod llms;
pub mod mailing;
pub mod media;
pub mod oauth;
pub mod plugins;
pub mod posts;
pub mod publish;
pub mod setup;
pub mod sites;
pub mod slug;
pub mod tags;
pub mod trash;
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
        .routes(routes!(console::registration))
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
        .routes(routes!(console::get_site_email, console::save_site_email))
        .routes(routes!(console::test_site_email))
        .routes(routes!(console::get_site_email_account))
        .routes(routes!(console::request_site_production_access))
        .routes(routes!(
            console::list_site_email_identities,
            console::add_site_email_identity
        ))
        .routes(routes!(console::delete_site_email_identity))
        .routes(routes!(console::list_site_email_suppressed))
        .routes(routes!(console::delete_site_email_suppressed))
        .routes(routes!(console::get_site_email_health))
        .routes(routes!(console::set_site_email_sending))
        .routes(routes!(console::list_site_email_configuration_sets))
        .routes(routes!(console::request_site_quota_increase))
        .routes(routes!(console::list_site_email_requests))
        .routes(routes!(console::set_site_email_mail_from))
        .routes(routes!(console::create_site_email_configuration_set))
        .routes(routes!(console::setup_site_email_events))
        .routes(routes!(console::get_site_email_deliverability))
        .routes(routes!(console::get_site_backup, console::save_site_backup))
        .routes(routes!(console::get_site_development))
        .routes(routes!(console::create_site_development_token))
        .routes(routes!(console::delete_site_development_token))
        .routes(routes!(console::run_site_backup))
        .routes(routes!(console::restore_site_backup))
        .routes(routes!(console::list_tokens, console::create_token))
        .routes(routes!(console::delete_token))
        .routes(routes!(console::enter));

    // The one address a site's own pages talk to without an account: the
    // visitor filling in a contact form has none. Kept to its own router so
    // the body limit applies to it and nothing else.
    // Its own router: it does its own authentication, because a refusal has
    // to be a JSON-RPC message rather than this application's error body, and
    // because which tools a caller is offered depends on the credential.
    let mcp = OpenApiRouter::new()
        .route(
            "/mcp",
            axum::routing::post(crate::mcp::endpoint)
                .get(crate::mcp::gone)
                .delete(crate::mcp::gone),
        )
        .layer(DefaultBodyLimit::max(crate::mcp::MAX_MESSAGE_BYTES))
        .layer(from_fn(require_database));

    // Answering to anybody, because that is what they are for: a client that
    // has not been given anything yet reads these to find out how to ask.
    let discovery = OpenApiRouter::new()
        .routes(routes!(oauth::protected_resource))
        .routes(routes!(oauth::authorization_server))
        .routes(routes!(oauth::register))
        .routes(routes!(oauth::authorize))
        .routes(routes!(oauth::request))
        .routes(routes!(oauth::grant))
        .routes(routes!(oauth::token))
        .layer(from_fn(require_database));

    let open = OpenApiRouter::new()
        .routes(routes!(llms::llms_txt))
        .routes(routes!(forms::form_schema))
        .routes(routes!(forms::submit_form))
        .routes(routes!(mailing::subscribe))
        .routes(routes!(mailing::confirm))
        .routes(routes!(mailing::unsubscribe_page, mailing::unsubscribe))
        .routes(routes!(mailing::open_pixel))
        .routes(routes!(mailing::click))
        .routes(routes!(mailing::receive_events))
        .layer(DefaultBodyLimit::max(forms::MAX_SUBMISSION_BYTES))
        .layer(from_fn(require_database));

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
        .routes(routes!(trash::list_trash))
        .routes(routes!(trash::restore_trash))
        .routes(routes!(trash::purge_trash))
        .routes(routes!(tags::delete_tag))
        .routes(routes!(tags::set_tag_translation_group))
        .routes(routes!(slug::make_slug))
        .routes(routes!(sites::list_sites, sites::create_site))
        .routes(routes!(sites::update_site))
        .routes(routes!(sites::delete_site))
        .routes(routes!(console::list_agencies))
        .routes(routes!(console::create_invite, console::list_invites))
        .routes(routes!(console::revoke_invite))
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
        .routes(routes!(development::list_tokens, development::create_token))
        .routes(routes!(development::delete_token))
        .routes(routes!(oauth::connections))
        .routes(routes!(oauth::disconnect))
        .routes(routes!(
            content_types::list_content_types,
            content_types::create_content_type
        ))
        .routes(routes!(
            content_types::update_content_type,
            content_types::delete_content_type
        ))
        .routes(routes!(plugins::list_plugins))
        .routes(routes!(
            plugins::get_s3_settings,
            plugins::update_s3_settings
        ))
        .routes(routes!(plugins::test_s3_settings))
        .routes(routes!(
            plugins::get_email_settings,
            plugins::update_email_settings
        ))
        .routes(routes!(plugins::test_email_settings))
        .routes(routes!(plugins::get_email_account))
        .routes(routes!(plugins::request_email_production_access))
        .routes(routes!(
            plugins::list_email_identities,
            plugins::add_email_identity
        ))
        .routes(routes!(plugins::delete_email_identity))
        .routes(routes!(plugins::list_email_suppressed))
        .routes(routes!(plugins::delete_email_suppressed))
        .routes(routes!(plugins::get_email_health))
        .routes(routes!(plugins::set_email_sending))
        .routes(routes!(plugins::list_email_configuration_sets))
        .routes(routes!(plugins::request_email_quota_increase))
        .routes(routes!(plugins::list_email_requests))
        .routes(routes!(plugins::set_email_mail_from))
        .routes(routes!(plugins::create_email_configuration_set))
        .routes(routes!(plugins::setup_email_events))
        .routes(routes!(plugins::get_email_deliverability))
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
        .routes(routes!(forms::list_forms, forms::create_form))
        .routes(routes!(
            forms::get_form,
            forms::update_form,
            forms::delete_form
        ))
        .routes(routes!(forms::list_submissions))
        .routes(routes!(forms::mark_seen))
        .routes(routes!(forms::delete_submission))
        .routes(routes!(mailing::list_lists, mailing::create_list))
        .routes(routes!(mailing::update_list, mailing::delete_list))
        .routes(routes!(
            mailing::list_subscribers,
            mailing::create_subscriber
        ))
        .routes(routes!(
            mailing::update_subscriber,
            mailing::delete_subscriber
        ))
        .routes(routes!(mailing::export_subscribers))
        .routes(routes!(mailing::list_templates, mailing::create_template))
        .routes(routes!(mailing::update_template, mailing::delete_template))
        .routes(routes!(mailing::list_campaigns, mailing::create_campaign))
        .routes(routes!(
            mailing::get_campaign,
            mailing::update_campaign,
            mailing::delete_campaign
        ))
        .routes(routes!(mailing::send_campaign))
        .routes(routes!(mailing::pause_campaign))
        .routes(routes!(mailing::cancel_campaign))
        .routes(routes!(mailing::test_campaign))
        .routes(routes!(mailing::list_log))
        .routes(routes!(mailing::list_events))
        .merge(
            OpenApiRouter::new()
                .routes(routes!(mailing::import_subscribers))
                // A list of a hundred thousand people is a few megabytes.
                .layer(DefaultBodyLimit::max(mailing::MAX_IMPORT)),
        )
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

    /// Headers every answer carries, whatever it is.
    ///
    /// The panel and this API answer on the same address, and a browser applies
    /// these to what it was given rather than to what it asked for — so setting
    /// them in one place and not the other leaves half the surface bare.
    async fn security_headers(
        request: axum::extract::Request,
        next: axum::middleware::Next,
    ) -> axum::response::Response {
        use axum::http::HeaderValue;

        let mut response = next.run(request).await;
        let headers = response.headers_mut();

        for (name, value) in [
            (
                "strict-transport-security",
                "max-age=31536000; includeSubDomains",
            ),
            ("x-content-type-options", "nosniff"),
            ("referrer-policy", "same-origin"),
            ("content-security-policy", "frame-ancestors 'none'"),
        ] {
            headers.insert(name, HeaderValue::from_static(value));
        }

        response
    }

    // Resolving the site is the outermost thing that happens: everything
    // below it, authentication included, is a question about one site.
    public
        .merge(discovery)
        .merge(mcp)
        .merge(open)
        .merge(needs_db)
        .merge(protected)
        .layer(from_fn_with_state(hosting, resolve))
        .layer(from_fn(security_headers))
}

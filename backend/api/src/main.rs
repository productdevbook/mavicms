use axum::{
    Json, Router,
    http::{HeaderValue, header},
    routing::get,
};
use tower::ServiceBuilder;
use tower_cookies::CookieManagerLayer;
use tower_http::{
    cors::CorsLayer, services::ServeDir, set_header::SetResponseHeaderLayer, trace::TraceLayer,
};
use utoipa::OpenApi as _;
use utoipa_axum::router::OpenApiRouter;
use utoipa_scalar::{Scalar, Servable};

use mavicms_api::state::AppState;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let config = mavicms_api::config::Config::from_env();

    let db = match &config.database_url {
        Some(url) => Some(
            mavicms_api::db::connect(url)
                .await
                .expect("failed to connect to database and run migrations"),
        ),
        None => {
            tracing::warn!(
                "no database configured (DATABASE_URL unset, no persisted config) — waiting for the setup wizard"
            );
            None
        }
    };

    let secrets = mavicms_api::crypto::SecretBox::load_or_create(&config.data_dir)
        .unwrap_or_else(|err| panic!("failed to initialise the master key: {err}"));

    let state = AppState {
        db,
        data_dir: config.data_dir.clone(),
        media_root: config.media_root.clone(),
        secrets: std::sync::Arc::new(secrets),
    };

    // Started before the state is handed to the router, which consumes it.
    if state.db.is_some() {
        mavicms_api::backup::spawn_scheduler(state.clone());
    }

    // The list of sites lives in the server's own database, so it is backed up
    // and restored with everything else rather than in a file of its own that
    // has to be remembered separately. On a server hosting one site it stays
    // empty and every request falls through to the installation already there.
    let registry = match (&state.db, &config.database_url) {
        (Some(db), Some(url)) => Some(std::sync::Arc::new(
            mavicms_api::tenants::Registry::new(
                db.clone(),
                url.clone(),
                config.data_dir.join("sites"),
            )
            .await
            .unwrap_or_else(|err| panic!("failed to prepare the site registry: {err}")),
        )),
        _ => None,
    };

    let hosting = mavicms_api::tenants::Hosting {
        registry,
        default_state: state,
        console_hosts: config.console_hosts.clone().into(),
    };

    mavicms_api::schedule::spawn(hosting.clone());

    let (router, api) = OpenApiRouter::<mavicms_api::tenants::Hosting>::with_openapi(
        mavicms_api::openapi::ApiDoc::openapi(),
    )
    .merge(mavicms_api::routes::router(hosting.clone()))
    .with_state(hosting)
    .split_for_parts();

    let openapi_json = api.clone();

    let app: Router = router
        .merge(Scalar::with_url("/scalar", api))
        .route(
            "/api-docs/openapi.json",
            get(move || {
                let doc = openapi_json.clone();
                async move { Json(doc) }
            }),
        )
        // Uploaded media is served publicly (no auth) — the same way readers
        // view any other image on a published post. Mounted at /uploads
        // rather than /media to avoid colliding with the /media/{id} API route.
        //
        // These files are attacker-influenced and served same-origin, so they
        // are locked down: uploads are already restricted to sniffed raster
        // images (see routes/media.rs), and nosniff + a no-privileges CSP stop
        // anything that slips through from being interpreted as a document.
        // Content-Disposition is deliberately *not* forced to `attachment` —
        // that would break inline <img> rendering in the editor and gallery.
        .nest_service(
            "/uploads",
            ServiceBuilder::new()
                .layer(SetResponseHeaderLayer::overriding(
                    header::CONTENT_SECURITY_POLICY,
                    HeaderValue::from_static("default-src 'none'; sandbox"),
                ))
                .layer(SetResponseHeaderLayer::overriding(
                    header::X_CONTENT_TYPE_OPTIONS,
                    HeaderValue::from_static("nosniff"),
                ))
                .service(ServeDir::new(&config.media_root)),
        )
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .layer(CookieManagerLayer::new());

    let addr = format!("{}:{}", config.host, config.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|err| panic!("failed to bind {addr}: {err}"));

    tracing::info!("mavicms-api listening on http://{addr}");
    tracing::info!("API docs available at http://{addr}/scalar");

    axum::serve(listener, app).await.expect("server error");
}

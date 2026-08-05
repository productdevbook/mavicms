mod auth;
mod config;
mod crypto;
mod db;
mod dto;
mod entities;
mod error;
mod languages;
mod middleware;
mod openapi;
mod plugins;
mod routes;
mod slug;
mod state;
mod storage;

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

use crate::state::AppState;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let config = config::Config::from_env();

    let db = match &config.database_url {
        Some(url) => Some(
            db::connect(url)
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

    let secrets = crypto::SecretBox::load_or_create(&config.data_dir)
        .unwrap_or_else(|err| panic!("failed to initialise the master key: {err}"));

    let state = AppState {
        db,
        data_dir: config.data_dir.clone(),
        media_root: config.media_root.clone(),
        secrets: std::sync::Arc::new(secrets),
    };

    let (router, api) = OpenApiRouter::<AppState>::with_openapi(openapi::ApiDoc::openapi())
        .merge(routes::router(state.clone()))
        .with_state(state)
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

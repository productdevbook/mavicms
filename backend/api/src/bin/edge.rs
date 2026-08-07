//! Serves the pages a build published.
//!
//! One process for every site on the server: it reads the address a request
//! came in on, finds which site that is, and answers from that site's folder
//! in the bucket. Adding a site adds no deployment, no container and no
//! configuration file — the site already exists in the control plane, and its
//! pages appear the first time it is built.
//!
//! What it holds in memory is a small cache of recently asked-for files. A
//! static page is bytes that do not change until the next build, so the first
//! reader pays for the fetch and the rest do not.

use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    Router,
    body::Body,
    extract::{Request, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use mavicms_api::{
    db,
    error::{AppError, AppResult},
    storage::{MediaStorage, S3Config},
};
use sea_orm::{ConnectionTrait, DatabaseConnection, Statement};
use tokio::sync::RwLock;

/// How long a file is served from memory before it is fetched again.
///
/// Short, because publishing has to feel immediate: a build finishes and the
/// pages change within the minute without anybody restarting anything.
const CACHE_FOR: Duration = Duration::from_secs(60);

/// How many files to keep. Whole pages and their assets, so this is sized for
/// the busy handful rather than the whole site.
const CACHE_LIMIT: usize = 512;

/// The largest file kept in memory. Bigger ones are still served, just fetched
/// each time — a video in the cache would evict a thousand pages.
const CACHE_MAX_BYTES: usize = 512 * 1024;

struct Cached {
    bytes: Vec<u8>,
    mime: String,
    stored_at: Instant,
}

struct Edge {
    control: DatabaseConnection,
    storage: MediaStorage,
    hosts: RwLock<HashMap<String, Option<String>>>,
    files: RwLock<HashMap<String, Cached>>,
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let required = |name: &str| {
        std::env::var(name)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| panic!("{name} is not set"))
    };

    let control = db::connect_plain(&required("DATABASE_URL"))
        .await
        .unwrap_or_else(|err| panic!("could not open the database: {err}"));

    let edge = Arc::new(Edge {
        control,
        storage: published_storage()
            .unwrap_or_else(|err| panic!("nowhere to read published pages from: {err}")),
        hosts: RwLock::new(HashMap::new()),
        files: RwLock::new(HashMap::new()),
    });

    let app = Router::new().fallback(serve).with_state(edge);

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(8081);
    let address = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .unwrap_or_else(|err| panic!("failed to bind {address}: {err}"));

    tracing::info!("edge serving published sites on http://{address}");
    axum::serve(listener, app).await.expect("server error");
}

async fn serve(State(edge): State<Arc<Edge>>, request: Request) -> Response {
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.split(':').next().unwrap_or(value).to_lowercase())
        .unwrap_or_default();

    let Some(slug) = edge.slug_for(&host).await else {
        return (StatusCode::NOT_FOUND, "no site answers on this address").into_response();
    };

    // The path becomes a key, and a key becomes a file path when pages are
    // kept on disk — so a request that walks upwards out of the site's folder
    // is refused before it can be one.
    let Some(path) = safe_path(request.uri().path()) else {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    };

    for candidate in candidates(&path) {
        if let Some(response) = edge.file(&slug, &candidate).await {
            return response;
        }
    }

    // The site's own not-found page if it built one, so a wrong address still
    // looks like the site rather than like the server.
    match edge.file(&slug, "404.html").await {
        Some(mut response) => {
            *response.status_mut() = StatusCode::NOT_FOUND;
            response
        }
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

/// The request path, decoded, or nothing if it points anywhere but inside the
/// site.
fn safe_path(path: &str) -> Option<String> {
    let decoded = percent_encoding::percent_decode_str(path)
        .decode_utf8()
        .ok()?;

    let refused = decoded.split('/').any(|segment| segment == "..")
        || decoded.contains('\\')
        || decoded.contains('\0');
    if refused {
        return None;
    }
    Some(decoded.into_owned())
}

/// The files a request path could mean, in the order to try them.
///
/// Static site generators write a folder per page, so `/about` is usually
/// `/about/index.html`; asking for the file itself first keeps real files
/// working when both exist.
fn candidates(path: &str) -> Vec<String> {
    let trimmed = path.trim_start_matches('/').trim_end_matches('/');

    if trimmed.is_empty() {
        return vec!["index.html".to_string()];
    }
    // A request that already names a file is not also a folder.
    if trimmed
        .rsplit('/')
        .next()
        .is_some_and(|last| last.contains('.'))
    {
        return vec![trimmed.to_string()];
    }
    vec![
        format!("{trimmed}/index.html"),
        format!("{trimmed}.html"),
        trimmed.to_string(),
    ]
}

impl Edge {
    /// Which site answers on this address.
    ///
    /// Answers are remembered including the absence of one, so a flood of
    /// requests for a hostname that is not here does not become a flood of
    /// queries.
    async fn slug_for(&self, host: &str) -> Option<String> {
        if let Some(known) = self.hosts.read().await.get(host) {
            return known.clone();
        }

        let found = self.lookup(host).await.unwrap_or_else(|err| {
            tracing::error!(host, error = %err, "could not look up the site");
            None
        });

        self.hosts
            .write()
            .await
            .insert(host.to_string(), found.clone());
        found
    }

    async fn lookup(&self, host: &str) -> Result<Option<String>, AppError> {
        let row = self
            .control
            .query_one_raw(Statement::from_sql_and_values(
                self.control.get_database_backend(),
                "SELECT slug FROM tenants WHERE host = $1 AND active = 1".to_string(),
                [host.into()],
            ))
            .await?;

        row.map(|row| row.try_get::<String>("", "slug"))
            .transpose()
            .map_err(Into::into)
    }

    async fn file(&self, slug: &str, path: &str) -> Option<Response> {
        let key = format!("{slug}/{path}");

        if let Some(cached) = self.files.read().await.get(&key)
            && cached.stored_at.elapsed() < CACHE_FOR
        {
            return Some(respond(cached.bytes.clone(), &cached.mime));
        }

        let bytes = self.storage.read(&key).await?;
        let mime = mime_of(path).to_string();

        if bytes.len() <= CACHE_MAX_BYTES {
            let mut files = self.files.write().await;
            if files.len() >= CACHE_LIMIT {
                // Everything at once rather than the oldest one: this is a
                // cache with a short life, and finding a victim each time
                // costs more than the misses that follow.
                files.clear();
            }
            files.insert(
                key,
                Cached {
                    bytes: bytes.clone(),
                    mime: mime.clone(),
                    stored_at: Instant::now(),
                },
            );
        }

        Some(respond(bytes, &mime))
    }
}

fn respond(bytes: Vec<u8>, mime: &str) -> Response {
    let mut response = Response::new(Body::from(bytes));

    if let Ok(value) = HeaderValue::from_str(mime) {
        response.headers_mut().insert(header::CONTENT_TYPE, value);
    }
    // Pages are rechecked; the files a build fingerprints are not, because
    // their names change when their contents do.
    let caching = if mime.starts_with("text/html") {
        "public, max-age=0, must-revalidate"
    } else {
        "public, max-age=31536000, immutable"
    };
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static(caching));

    // A published site is somebody's public address, reached over TLS the
    // ingress ends. Framing is left alone here — a site has no session to
    // steal clicks from, and an owner may well want to embed their own pages.
    for (name, value) in [
        (
            "strict-transport-security",
            "max-age=31536000; includeSubDomains",
        ),
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "strict-origin-when-cross-origin"),
    ] {
        response
            .headers_mut()
            .insert(name, HeaderValue::from_static(value));
    }

    response
}

fn mime_of(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" => "application/json",
        "xml" => "application/xml",
        "txt" => "text/plain; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "pdf" => "application/pdf",
        "webmanifest" => "application/manifest+json",
        _ => "application/octet-stream",
    }
}

/// Where published pages are read from.
///
/// A bucket when one is configured, and a folder otherwise — the same choice
/// the builder makes, from the same variables, so the two cannot disagree
/// about where a site's pages went.
fn published_storage() -> AppResult<MediaStorage> {
    let read = |name: &str| {
        std::env::var(name)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    };

    let Some(bucket) = read("PUBLISH_S3_BUCKET") else {
        let root = std::path::PathBuf::from(
            read("PUBLISH_DIR").unwrap_or_else(|| "/published".to_string()),
        );
        return Ok(MediaStorage::Local { root });
    };

    let required =
        |name: &str| read(name).ok_or_else(|| AppError::Internal(format!("{name} is not set")));
    let config = S3Config {
        endpoint: required("PUBLISH_S3_ENDPOINT")?,
        region: read("PUBLISH_S3_REGION").unwrap_or_else(|| "auto".to_string()),
        bucket,
        access_key_id: required("PUBLISH_S3_ACCESS_KEY_ID")?,
        secret_access_key: required("PUBLISH_S3_SECRET_ACCESS_KEY")?,
        public_base_url: String::new(),
        path_prefix: read("PUBLISH_S3_PREFIX").unwrap_or_else(|| "sites".to_string()),
    };
    config.validate()?;

    Ok(MediaStorage::S3(Box::new(config)))
}

#[cfg(test)]
mod tests {
    use super::{candidates, safe_path};

    #[test]
    fn a_folder_path_tries_its_index() {
        assert_eq!(candidates("/"), ["index.html"]);
        assert_eq!(
            candidates("/hakkimda"),
            ["hakkimda/index.html", "hakkimda.html", "hakkimda"]
        );
        assert_eq!(
            candidates("/yazi/merhaba/"),
            [
                "yazi/merhaba/index.html",
                "yazi/merhaba.html",
                "yazi/merhaba"
            ]
        );
    }

    #[test]
    fn a_path_cannot_walk_out_of_the_site() {
        // With pages on disk this is the difference between serving a page and
        // serving whatever else the process can read.
        assert!(safe_path("/../../etc/passwd").is_none());
        assert!(safe_path("/yazi/../../../etc/passwd").is_none());
        assert!(safe_path("/%2e%2e/%2e%2e/etc/passwd").is_none());
        assert!(safe_path(r"/..\windows").is_none());

        assert_eq!(safe_path("/yazi/merhaba").unwrap(), "/yazi/merhaba");
        // A dot inside a name is a name, not a step upwards.
        assert_eq!(safe_path("/a..b/c").unwrap(), "/a..b/c");
    }

    #[test]
    fn a_path_naming_a_file_is_taken_at_its_word() {
        assert_eq!(candidates("/assets/app.css"), ["assets/app.css"]);
        assert_eq!(candidates("/favicon.ico"), ["favicon.ico"]);
    }
}

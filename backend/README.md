# Mavi CMS API

Rust backend for Mavi CMS. Axum + SeaORM, OpenAPI docs served via [Scalar](https://scalar.com).

**Bring your own database** — SeaORM dispatches on the `DATABASE_URL` scheme, so
the exact same binary works against Postgres, MySQL or SQLite. No code
changes, no rebuild.

## Run locally

```bash
cp .env.example .env      # defaults to a local SQLite file
cargo run -p mavicms-api
```

The server applies pending migrations automatically on startup, then listens
on `http://0.0.0.0:8080` (override with `HOST`/`PORT`).

- Scalar API reference: `http://localhost:8080/scalar`
- Raw OpenAPI spec: `http://localhost:8080/api-docs/openapi.json`
- Health check: `http://localhost:8080/health`

## First-run setup and the database

If `DATABASE_URL` isn't set (and no database has been configured yet via the
wizard — see below), the server still boots, but only `/health` and
`/setup/*` are reachable; every other route returns `503` until a database is
configured. `GET /setup/status` reports `database_configured` and
`installed` so the frontend wizard knows which step to show.

`POST /setup/database` configures the database from the wizard: either a raw
connection URL (`{ "url": "postgres://..." }`) or structured fields
(`{ "engine": "postgres" | "mysql" | "sqlite", "host", "port", "database",
"username", "password" }`). It connects and runs migrations against what's
given *before* persisting anything — a bad connection returns `400` and
changes nothing. On success it writes the URL to `{MAVICMS_DATA_DIR}/database_url`
and the process exits; the container/pod's restart policy brings it back up,
at which point `config.rs` picks up the persisted URL exactly like an
explicit `DATABASE_URL` env var (which always takes priority if set — this
is the path for docker-compose/k8s users who prefer to configure the
database at deploy time instead of through the wizard).

Once a database is available, `POST /setup` creates the site record and the
first administrator account (username, email, password — hashed with Argon2)
in one step, and signs them in (see Sessions below). Returns `409 Conflict`
if setup has already run.

## Connecting your own database

Either set `DATABASE_URL` in `.env` (or as a real environment variable), or
leave it unset and use the setup wizard's database step. Either way it's one
of:

```bash
# Postgres
DATABASE_URL=postgres://user:password@host:5432/dbname

# MySQL
DATABASE_URL=mysql://user:password@host:3306/dbname

# SQLite (file-based)
DATABASE_URL=sqlite://data.db?mode=rwc
```

That's the only thing that changes. Migrations run against whichever
database the URL points to.

## Sessions

`POST /login` (username + password) and the final step of `POST /setup` both
set an `HttpOnly` session cookie (`mavicms_session`, backed by a `sessions`
table — no JWT). `GET /me` returns the signed-in user or `401`. `POST
/logout` always succeeds, clearing the cookie and best-effort deleting the
session row. Every route except `/health`, `/setup/*` and `/login` requires
a valid session.

## Categories, tags and media

`/categories` and `/tags` are plain CRUD (`POST /tags` is get-or-create by
name, matching free-form tagging). Posts link to categories via
`category_ids` on create/update (validated to exist, replaces the full set
each time) and to tags by name (get-or-create, same replace-the-set
semantics) — both are additive to the existing free-text `category`/`tags`
fields on `posts`, not a replacement for them.

`POST /media` accepts a `multipart/form-data` upload (field name `file`,
images only, 10MB max). The type is decided by sniffing the file's magic bytes,
not the client-supplied `Content-Type`; SVG is rejected because uploads are
served same-origin and SVG can carry script.

By default files go to `{MAVICMS_DATA_DIR}/media/{year}/{month}/` and are served
publicly (no auth) at `/uploads/...` via `tower_http::services::ServeDir` —
mounted at `/uploads` rather than `/media` specifically to avoid colliding with
the `/media/{id}` management route, and wrapped in `nosniff` +
`Content-Security-Policy: default-src 'none'; sandbox`.

`GET /media` lists, `DELETE /media/{id}` removes both the row and the file. Each
row records `storage_backend`/`storage_key`, so deletion still finds the file
after the active backend changes.

## Plugins and secret storage

`/plugins` exposes the built-in integrations. Today that is `s3_storage`:
`GET|PUT /plugins/s3` reads and writes the configuration, `POST /plugins/s3/test`
round-trips a small object to verify credentials *and* write permission.

When enabled, new uploads go to the bucket and their public URL (the configured
`public_base_url`, i.e. the bucket's public address or a CDN) is stored in the
post HTML. That URL has to stay valid forever, which is why presigned URLs are
not used. Files already on local disk stay there and keep working.

Plugin configuration is **encrypted at rest** (`api/src/crypto.rs`, AES-256-GCM):
an S3 secret key must be replayable to sign requests, so unlike a password it
cannot be hashed. The master key comes from `MAVICMS_SECRET_KEY` (base64, 32
bytes) when set; otherwise one is generated and stored at
`{MAVICMS_DATA_DIR}/secret_key` with `0600`. Lose it and stored credentials
simply have to be re-entered — the app reports that instead of failing to boot.
The secret is never returned by the API; `GET /plugins/s3` only reports
`has_secret_access_key`, and submitting an empty secret keeps the stored one.

> Note: the `endpoint` field lets an administrator point the server at an
> arbitrary URL (an SSRF surface). This is inherent to "bring your own S3" and
> is reachable only by an authenticated administrator, who already controls the
> server's configuration.

## Project layout

```
backend/
  api/          the HTTP server (Axum, routes, DTOs, OpenAPI)
  migration/    SeaORM migrations, runnable standalone via `cargo run -p migration`
  Dockerfile    multi-stage build producing a ~150MB runtime image
```

## Migrations

Migrations run automatically when `mavicms-api` starts. To manage them by
hand (e.g. to roll back):

```bash
cargo run -p migration -- up      # apply all pending migrations
cargo run -p migration -- down    # revert the last migration
cargo run -p migration -- status  # list applied/pending migrations
```

## Adding an endpoint

1. Add fields/entities under `api/src/entities/` (and a migration under
   `migration/src/` if the schema changes).
2. Add request/response DTOs in `api/src/dto/` with `#[derive(ToSchema)]`.
3. Write the handler in `api/src/routes/`, annotated with `#[utoipa::path(...)]`.
4. Register it in `api/src/routes/mod.rs`'s `router()` via the `routes!` macro.

The OpenAPI spec and Scalar docs update automatically — there's nothing to
regenerate by hand.

## Docker

```bash
docker build -t mavicms-api .
docker run -p 8080:8080 -e DATABASE_URL=postgres://... mavicms-api
```

See the repo root for `docker-compose.yml`, which wires this up with a
bundled Postgres instance and the frontend.

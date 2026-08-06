# Mavi CMS

A self-hosted CMS with a Rust backend and a Tiptap editor. Bring your own
database — Postgres, MySQL or SQLite — and your own object storage, or none.

- **Rust backend** — Axum, SeaORM, OpenAPI docs, argon2 password hashing,
  session cookies.
- **Any database** — one `DATABASE_URL` away from Postgres, MySQL or SQLite.
  Migrations run at boot and are tested on all three.
- **Multilingual content** — the same post in as many languages as you like,
  each with its own title, slug, content and SEO, linked to its siblings.
  Categories and tags are translated too.
- **Media** — local disk by default, or any S3-compatible bucket (AWS S3,
  Cloudflare R2, MinIO) configured from the admin panel, with the credentials
  encrypted at rest.
- **Editor** — Tiptap 3 with slash commands, tables, code blocks, find and
  replace, a table of contents, autosave and Markdown import/export.
- **Moving from WordPress** — a [plugin](#migrating-from-wordpress) that sends
  your posts, taxonomies and images across, keeping dates and permalinks.
- **[Many sites on one server](#many-sites-on-one-server)** — each on its own
  address, in its own Postgres schema, from a single instance.

## Quick start

```bash
curl -O https://raw.githubusercontent.com/productdevbook/mavicms/main/docker-compose.yml
docker compose up -d
```

Open <http://localhost:8081> and the setup wizard takes it from there: pick a
language, point it at a database, name the site, create the first account.

The compose file runs a bundled Postgres. To use your own database instead, set
`DATABASE_URL` and drop the `postgres` service:

```bash
DATABASE_URL=postgres://user:password@your-host:5432/mavicms docker compose up -d
```

SQLite needs no server at all and is a reasonable choice for a small site:

```
DATABASE_URL=sqlite:///data/mavicms.db?mode=rwc
```

### Images

| | |
|---|---|
| Backend | `ghcr.io/productdevbook/mavicms-backend` |
| Frontend | `ghcr.io/productdevbook/mavicms-frontend` |

Both are built for `linux/amd64` and `linux/arm64`.

### Configuration

The backend reads these; everything else is configured from the admin panel.

| Variable | Default | Notes |
|---|---|---|
| `DATABASE_URL` | — | Postgres, MySQL or SQLite. Set at first run through the wizard if absent. |
| `MAVICMS_DATA_DIR` | `/data` | Uploaded media and the encryption key. **Must be a persistent volume.** |
| `HOST` / `PORT` | `0.0.0.0` / `8080` | |
| `RUST_LOG` | `info` | |

The frontend is static files behind nginx, which proxies `/api`, `/uploads` and
`/scalar` to the backend.

## Many sites on one server

A single instance can host hundreds of sites, each answering on its own
address. Add one from **Sites** in the admin panel, or:

```bash
curl -X POST https://your-server/api/sites -b cookies.txt \
  -H 'content-type: application/json' \
  -d '{"host": "example.com"}'
```

Point the address at the server and open it — the new site runs the setup
wizard like any fresh install.

Hosting more than one site needs Postgres. Every site gets a **schema of its
own** — its tables, its accounts, its own migration history — rather than
sharing tables keyed by a site id. Nothing has to remember to filter by tenant,
and a site's connection cannot see another site's tables: the search path holds
only its own schema, so a missing table is an error rather than a quiet read of
someone else's. Uploads and the encryption key sit beside it in
`MAVICMS_DATA_DIR/sites/<name>/`. A site that outgrows the shared server can be
given a `database_url` of its own, with nothing else changing.

Memory is bounded by how many sites are *busy*, not how many exist. Sites open
on demand, at most 32 stay open, one that has served no request for ten minutes
is closed, and each holds at most two connections. Four hundred sites measure
at 98 MB resident — unchanged from two hundred — and never hold more than 67
Postgres connections, however the traffic is spread. Empty sites cost about
400 KB each in the database.

Managing sites is the server operator's alone. An administrator of a hosted
site administers that site: they cannot list the other sites on the machine,
add sites, or reach the database wizard — which restarts the process, and so
would take every other site down with it.

## Migrating from WordPress

Install [the plugin](https://github.com/productdevbook/mavicms/releases/latest)
on the WordPress site you are leaving, then go to **Tools → Migrate to Mavi CMS**,
enter this site's address and sign in.

It moves posts with their original dates, permalinks and status, the category
tree, tags, featured images and the images inside post content — rewriting them
to point at their new copies. Polylang and WPML languages are carried across and
translations of the same post are linked together.

Nothing on the WordPress side is changed or deleted, and the migration is
resumable: a post that has already been sent is skipped, so you can stop and
continue whenever.

## Development

Requires [Bun](https://bun.sh) and a Rust toolchain.

```bash
bun install
bun run dev          # http://localhost:5173, proxies the API to :8080

cd backend
cargo run            # http://localhost:8080, API docs at /scalar
```

The frontend expects the backend on `:8080`; point it elsewhere with
`VITE_API_PROXY_TARGET`.

```bash
bun run build        # builds, then typechecks — vite generates the route tree
bun run typecheck
bun run lint
bun run extract      # pull new translatable strings into src/locales/*/messages.po

cd backend
cargo clippy --all-targets -- -D warnings
cargo test
```

Or run the whole thing in containers, built from your checkout:

```bash
docker compose -f docker-compose.dev.yml up --build
```

### Layout

```
src/                     React 19, Vite, TanStack Router, Tailwind 4, Tiptap 3
backend/api/             Axum handlers, DTOs, SeaORM entities
backend/migration/       schema migrations, run automatically at boot
wordpress-plugin/        the WordPress migration plugin (GPLv2+)
```

The admin interface is English and Turkish, via [Lingui](https://lingui.dev).
Interface language is independent of the language your content is written in.

## API

Interactive docs are served from a running instance at `/scalar`, and the
OpenAPI document at `/api/api-docs/openapi.json`.

## License

MIT — see [LICENSE](LICENSE). The WordPress plugin is GPL-2.0-or-later, as
WordPress plugins must be.

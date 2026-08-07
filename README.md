# Mavi CMS

A self-hosted CMS with a Rust backend and a Tiptap editor. Bring your own
database — Postgres, MySQL or SQLite — and your own object storage, or none.

- **Rust backend** — Axum, SeaORM, OpenAPI docs, argon2 password hashing,
  session cookies.
- **Any database** — one `DATABASE_URL` away from Postgres, MySQL or SQLite.
  Migrations run at boot and are tested on all three.
- **Scheduling** — a post given a date goes out on it, and the site is asked
  to build. Within the minute, whether or not anybody is looking.
- **Multilingual content** — the same post in as many languages as you like,
  each with its own title, slug, content and SEO, linked to its siblings.
  Categories and tags are translated too.
- **Media** — local disk by default, or any S3-compatible bucket (AWS S3,
  Cloudflare R2, MinIO) configured from the admin panel, with the credentials
  encrypted at rest.
- **Editor** — Tiptap 3 with slash commands, tables, code blocks, find and
  replace, a table of contents, autosave and Markdown import/export.
- **[Assistants](#assistants)** — every site, and every agency console, answers
  the Model Context Protocol. Point an assistant at it and ask it to do the
  work rather than tell it how.
- **[Front ends](#connecting-a-front-end)** — every site publishes an
  `llms.txt` describing its own API, with a working Astro connection in it,
  and posts carry a digest so a build only rebuilds what changed.
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

### Agencies

Three kinds of account, deliberately separate rather than one with flags:

| | Signs in at | Sees |
|---|---|---|
| Whoever runs the server | its own address, **Sites** | every site on the machine |
| An agency | `/console` | its own sites, and no others |
| The people who write a site | that site's `/dashboard` | that site |

An agency opens an account at `/console/register`, adds sites up to its limit,
and opens any of them from the console. That last step does not make the
agency's password work on the site: the console mints a token good for one use
and two minutes, and the site trades it for a session of its own. So the
account an agency writes as on a site has no password at all — closing the
console account closes every site with it, rather than leaving a working login
behind on fifty of them.

An administrator of a hosted site administers that site and nothing else: they
cannot list the other sites on the machine, add sites, reach the console, or
reach the database wizard — which restarts the process, and so would take every
other site down with it.

## Connecting a front end

Pages are built somewhere else, out of the API. What that takes is a page of
documentation, and half of any page of documentation does not apply to your
installation — so each site writes its own:

```bash
curl https://your-site/api/llms.txt
```

That address answers to anybody and holds the whole of it: this site's
addresses, how to get a read-only token, what a post looks like, and a working
Astro connection to paste in — a content loader, the collection, the config and
the page. Fetched with an account it also lists this site's languages and the
forms it is taking answers on. **API** in the panel has a button that copies it,
which is the short way to hand a site to an assistant.

Two things make a rebuild cheap:

- Every post carries a **`digest`** — a fingerprint of what it renders to. It
  changes when the post changes and not when somebody opens it and saves it
  again, so a build keyed on it rebuilds the pages that moved and restores the
  rest from its cache. It is on the listing whether or not the bodies were
  asked for, so finding out what changed does not mean downloading the archive.
- A listing carries an **`ETag`**. Send it back as `If-None-Match` and an
  archive that has not changed answers `304` with no body.

The Astro connection in `llms.txt` uses both, and Astro 7.2's
`experimental.incrementalBuild` with `cacheKey` on top of them. A build after a
change to one post fetches one post and regenerates one page.

Ask for `status=published` unless you mean not to. Without it, every draft on
the site is in the answer.

A post given a status of **scheduled** and a date is published when that date
arrives — the server checks every minute — and the site is asked to build, so
a post written on Friday for Monday morning is on the site on Monday morning.
A build does not have to do anything for this; it will be asked to run.

## Assistants

Every site answers the [Model Context Protocol](https://modelcontextprotocol.io)
at `https://your-site/api/mcp`, and so does an agency console at the server's
own address. Which of the two you get is decided by the address you reach and
the token you send, the same way everything else here is.

In anything that asks for a URL — Claude's connector dialog, ChatGPT's — paste
the address and nothing else:

```
https://your-site/api/mcp
```

The site is its own authorization server. Whoever is connecting is sent to
this site to sign in, asked plainly whether to allow it, and the program is
handed a credential they never see and cannot paste anywhere. **API** in the
panel lists what is connected, and disconnecting one stops it immediately.

In a terminal, where a header is easier than a browser:

```bash
claude mcp add --transport http mavicms https://your-site/api/mcp \
  --header "Authorization: Bearer $CMS_TOKEN"
```

A site offers finding and reading posts, writing and correcting them, its
categories, languages and uploaded files, what has come in through its forms,
and building its pages. A console offers the questions an agency has about
fifty sites at once: which built, which did not, what the failing one said,
and adding a new site.

Two rules make it safe to leave connected:

- **The token decides the tools.** A build token can read a site and change
  nothing, so it is offered only the tools that read — fewer tools rather than
  tools that refuse, because a tool an assistant cannot use is one it should
  not have been told about. An agency's token acts for the agency and is not a
  way into any site's content.
- **Nothing deletes.** An assistant that misreads an instruction and writes a
  bad paragraph has done something a person can read and undo. One that
  misreads it and removes an archive has not.

Tokens are made in **API** on a site, and on **your account** in the console.
Both are shown once and can be taken back. A connection made through the
sign-in flow needs no token at all — which is the point of it: the person
connecting an assistant to their own site should not have to handle a
credential, and an agency should not be sending one to a customer.

The protocol is spoken at revision `2026-07-28`, which is stateless — no
handshake, no session. Clients that still open with `initialize` are answered
in the revision they ask for.

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

Every dependency has been checked against that, and what was deliberately not
borrowed is written down too: [LICENSES.md](LICENSES.md).

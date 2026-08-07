use utoipa::{
    Modify, OpenApi,
    openapi::security::{Http, HttpAuthScheme, SecurityScheme},
};

/// What the interactive documentation needs in order to be usable.
///
/// Without this there is no Authorize button on the page, so every endpoint on
/// it answers 401 and there is nothing a reader can do about it from where
/// they are standing — which is most of the way to the documentation being
/// decorative.
struct Credentials;

impl Modify for Credentials {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi
            .components
            .as_mut()
            .expect("the document always has components");

        components.add_security_scheme(
            "token",
            SecurityScheme::Http(
                utoipa::openapi::security::HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .description(Some(
                        "A build token, made in the panel under API. It can read this site and \
                         change nothing. Paste it here to try the read endpoints below.",
                    ))
                    .build(),
            ),
        );

        components.add_security_scheme(
            "session",
            SecurityScheme::Http(
                Http::builder()
                    .scheme(HttpAuthScheme::Bearer)
                    .description(Some(
                        "The id of a session from POST /login, sent as a bearer token. This is \
                         an account, so it can write. A browser sends the same value as the \
                         mavicms_session cookie instead.",
                    ))
                    .build(),
            ),
        );
    }
}

#[derive(OpenApi)]
#[openapi(
    modifiers(&Credentials),
    // Everything wants a credential unless the endpoint says otherwise, which
    // is the honest default: the exceptions are few and each one is marked.
    security(("token" = [], "session" = [])),
    info(
        title = "Mavi CMS",
        description = "\
This is one site's API. Which site depends on the address you are reading it \
on: a single server answers for many sites and the host name is the whole of \
the difference between them, so the addresses here are this site's and no \
other's.

## Getting in

Reading needs an account. A build is not a person and has none — it is handed \
a **build token**, made in the panel under **API**, which can read this site \
and change nothing. Send it as `Authorization: Bearer …`, which is also what \
the Authorize button at the top of this page takes.

A browser signs in at `POST /login` and gets a cookie. An assistant connects \
over OAuth and never sees a credential at all; see `GET /mcp`.

Two things answer to anybody, because the visitor filling in a contact form \
or a newsletter box has no account here: the form endpoints and \
`POST /mail/subscribe`.

## Before you write a client

`GET /llms.txt` is this same API written out as prose, with this site's own \
addresses in it, the shape of a post field by field, and a working Astro \
connection to paste in. It is shorter than reading this page, and it is what \
to hand an assistant.

## Two things that make a build cheap

Every post carries a `digest`, a fingerprint of what it renders to; it changes \
when the post changes and not when it is merely saved again, so a build keyed \
on it rebuilds only what moved. And `GET /posts` carries an `ETag` — send it \
back as `If-None-Match` and an unchanged listing answers `304` with no body.

Ask for `status=published` unless you mean not to. Without it every draft on \
the site is in the answer.",
        license(name = "MIT", url = "https://github.com/productdevbook/mavicms/blob/main/LICENSE"),
    ),
    tags(
        (name = "posts", description = "Writing, reading and listing posts. `GET /posts` is the one a site generator lives on: narrow it with `status`, `locale` and `q`, ask for bodies with `include=content` and for the head strings with `include=seo`, and page with `limit` and `offset` until you have `total`."),
        (name = "taxonomy", description = "Categories and tags. Both are per-language: a Turkish post takes Turkish categories, and the same category in two languages is linked by a translation group rather than duplicated."),
        (name = "languages", description = "The languages this site writes content in. Independent of the language the panel is displayed in."),
        (name = "media", description = "Uploaded files. Addresses that begin with `/` are on this server; when the site uploads to object storage they are already absolute."),
        (name = "forms", description = "Forms, and what visitors send through them. A form is made in the panel, so which fields it takes cannot be written down here: ask `GET /forms/{slug}/schema`, then post to `/forms/{slug}/submit`. Both work without an account, because the visitor filling one in has none."),
        (name = "mail", description = "Mailing lists, subscribers, templates and campaigns. `POST /mail/subscribe` is open, and answers 202 whether or not anything was sent — on a double opt-in list nothing goes out until the address confirms."),
        (name = "oauth", description = "How an assistant connects to this site without being given a credential. A client registers itself, sends somebody here to sign in and agree, and is handed a token for the account that agreed. The panel lists the connections and can take one back."),
        (name = "mcp", description = "This site as a set of tools, over the Model Context Protocol. What is offered depends on the credential: a build token is offered only the tools that read, and nothing here deletes anything."),
        (name = "development", description = "Wiring a front end to this site: the read-only tokens a build runs with, and `GET /llms.txt`, which is the whole of what a program needs in order to build pages from it."),
        (name = "publish", description = "Asking this site to build its pages again. Belongs to a hosted site; the server's own installation is the panel and has no pages of its own."),
        (name = "auth", description = "Signing in, signing out, and who the current session belongs to."),
        (name = "users", description = "The accounts that can write this site."),
        (name = "setup", description = "First run: the database, the site's name, and the first account. Answered only while the site has none of them."),
        (name = "sites", description = "The sites this server hosts. Reached on the server's own address, by whoever runs it."),
        (name = "console", description = "An agency's own account and the sites it looks after, at `/console`. Separate from a site's panel on purpose: an agency signing in here cannot write any site's content without going through it."),
        (name = "plugins", description = "Built-in integrations — object storage, email, backups — configured per site, with their credentials encrypted at rest."),
        (name = "health", description = "Whether the service is up."),
    )
)]
pub struct ApiDoc;

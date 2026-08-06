use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Mavi CMS API",
        description = "REST API for managing Mavi CMS blog posts. Bring your own database — set DATABASE_URL to a Postgres, MySQL or SQLite connection string."
    ),
    tags(
        (name = "setup", description = "First-run installation: site info and admin account"),
        (name = "auth", description = "Sign in, sign out and the current session"),
        (name = "posts", description = "Create, read, update and delete blog posts"),
        (name = "taxonomy", description = "Categories and tags"),
        (name = "languages", description = "Content languages"),
        (name = "mail", description = "Mailing lists, subscribers, templates and campaigns"),
        (name = "forms", description = "Forms, and what visitors send through them. A form is made in the panel, so which fields it takes cannot be written down here: ask GET /forms/{slug}/schema, then POST to /forms/{slug}/submit. Both work without an account."),
        (name = "media", description = "Upload and manage media files"),
        (name = "plugins", description = "Built-in integrations, such as S3 storage"),
        (name = "development", description = "Wiring a front end to a site: the read-only tokens a build runs with, and GET /llms.txt, which is the whole of what a program needs to build pages from this site."),
        (name = "health", description = "Service health checks"),
    )
)]
pub struct ApiDoc;

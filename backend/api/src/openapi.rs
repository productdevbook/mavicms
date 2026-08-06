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
        (name = "forms", description = "Forms, and what visitors send through them"),
        (name = "media", description = "Upload and manage media files"),
        (name = "plugins", description = "Built-in integrations, such as S3 storage"),
        (name = "health", description = "Service health checks"),
    )
)]
pub struct ApiDoc;

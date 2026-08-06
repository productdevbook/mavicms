export class ApiError extends Error {
  status: number

  constructor(status: number, message: string) {
    super(message)
    this.status = status
  }
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`/api${path}`, {
    headers: { "Content-Type": "application/json" },
    ...init,
  })

  if (!response.ok) {
    const body = await response
      .json()
      .catch(() => ({ error: response.statusText }))
    throw new ApiError(response.status, body.error ?? response.statusText)
  }

  if (response.status === 204) {
    return undefined as T
  }

  return response.json() as Promise<T>
}

export interface SetupStatus {
  database_configured: boolean
  installed: boolean
  site_title: string | null
}

export type DatabaseEngine = "postgres" | "mysql" | "sqlite"

export interface DatabasePayload {
  url?: string
  engine?: DatabaseEngine
  host?: string
  port?: number
  database?: string
  username?: string
  password?: string
}

export interface SetupPayload {
  site_title: string
  tagline: string
  locale: string
  admin_username: string
  admin_email: string
  admin_password: string
}

export interface SetupResult {
  site_title: string
  admin_username: string
}

export function getSetupStatus(): Promise<SetupStatus> {
  return request<SetupStatus>("/setup/status")
}

export function configureDatabase(
  payload: DatabasePayload
): Promise<{ database_configured: boolean }> {
  return request("/setup/database", {
    method: "POST",
    body: JSON.stringify(payload),
  })
}

export function submitSetup(payload: SetupPayload): Promise<SetupResult> {
  return request<SetupResult>("/setup", {
    method: "POST",
    body: JSON.stringify(payload),
  })
}

export interface CurrentUser {
  username: string
  email: string
  role: string
  /** Administering the server rather than one of the sites on it. */
  operator: boolean
}

export function getCurrentUser(): Promise<CurrentUser> {
  return request<CurrentUser>("/me")
}

export function login(
  username: string,
  password: string
): Promise<CurrentUser> {
  return request<CurrentUser>("/login", {
    method: "POST",
    body: JSON.stringify({ username, password }),
  })
}

export function logout(): Promise<void> {
  return request<void>("/logout", { method: "POST" })
}

export interface Category {
  id: string
  name: string
  slug: string
  parent_id: string | null
  description: string
  locale: string
  translation_group_id: string
  /** "complete" or "needs_translation" (an auto-created stub). */
  translation_status: string
}

export function getCategories(locale?: string): Promise<Category[]> {
  return request<Category[]>(`/categories${locale ? `?locale=${locale}` : ""}`)
}

export function createCategory(
  name: string,
  locale?: string,
  description = "",
  parentId?: string | null
): Promise<Category> {
  return request<Category>("/categories", {
    method: "POST",
    body: JSON.stringify({ name, description, locale, parent_id: parentId }),
  })
}

export function updateCategory(
  id: string,
  payload: { name?: string; description?: string; parent_id?: string | null }
): Promise<Category> {
  return request<Category>(`/categories/${id}`, {
    method: "PUT",
    body: JSON.stringify(payload),
  })
}

export interface Tag {
  id: string
  name: string
  slug: string
  locale: string
  translation_group_id: string
  translation_status: string
}

export function deleteCategory(id: string): Promise<void> {
  return request<void>(`/categories/${id}`, { method: "DELETE" })
}

export function getTags(locale?: string): Promise<Tag[]> {
  return request<Tag[]>(`/tags${locale ? `?locale=${locale}` : ""}`)
}

export function createTag(name: string, locale?: string): Promise<Tag> {
  return request<Tag>("/tags", {
    method: "POST",
    body: JSON.stringify({ name, locale }),
  })
}

export function deleteTag(id: string): Promise<void> {
  return request<void>(`/tags/${id}`, { method: "DELETE" })
}

export function setTagTranslationGroup(
  id: string,
  payload: { join: string } | { detach: true }
): Promise<Tag> {
  return request<Tag>(`/tags/${id}/translation-group`, {
    method: "PATCH",
    body: JSON.stringify(payload),
  })
}

export interface MediaItem {
  id: string
  filename: string
  url: string
  mime_type: string
  size_bytes: number
  alt_text: string
  uploaded_at: string
}

export function getMedia(): Promise<MediaItem[]> {
  return request<MediaItem[]>("/media")
}

export async function uploadMedia(file: File): Promise<MediaItem> {
  const formData = new FormData()
  formData.append("file", file)
  // No Content-Type header here — the browser sets the multipart boundary
  // itself, which request()'s forced application/json header would break.
  const response = await fetch("/api/media", { method: "POST", body: formData })

  if (!response.ok) {
    const body = await response
      .json()
      .catch(() => ({ error: response.statusText }))
    throw new ApiError(response.status, body.error ?? response.statusText)
  }

  return response.json() as Promise<MediaItem>
}

export function deleteMedia(id: string): Promise<void> {
  return request<void>(`/media/${id}`, { method: "DELETE" })
}

export type PostStatus = "draft" | "review" | "scheduled" | "published"

export interface Post {
  id: string
  title: string
  slug: string
  excerpt: string
  status: PostStatus
  publish_at: string | null
  author: string
  category: string
  category_ids: string[]
  tags: string[]
  cover_url: string
  seo_title: string
  seo_description: string
  canonical: string
  featured: boolean
  allow_comments: boolean
  content_html: string
  /** The canonical form; null on posts written before the move to Markdown. */
  content_markdown: string | null
  locale: string
  translation_group_id: string
  /** Languages this post exists in, including its own. */
  locales: string[]
  /** Sibling language versions; empty on the list endpoint. */
  translations: PostTranslation[]
  created_at: string
  updated_at: string
}

export interface PostTranslation {
  id: string
  locale: string
  title: string
  slug: string
  status: PostStatus
}

export interface PostPayload {
  title: string
  slug: string
  excerpt?: string
  status?: PostStatus
  publish_at?: string | null
  /** Preserved when importing from another CMS; defaults to now. */
  created_at?: string
  author?: string
  category?: string
  category_ids?: string[]
  tags?: string[]
  cover_url?: string
  seo_title?: string
  seo_description?: string
  canonical?: string
  featured?: boolean
  allow_comments?: boolean
  content_html?: string
  content_markdown?: string
  locale?: string
  /** Id of an existing post this is a translation of. */
  translation_of?: string
}

/** A listing omits `content_html`; the full body comes from `getPost`. */
export type PostSummary = Omit<
  Post,
  | "content_html"
  | "translations"
  | "seo_title"
  | "seo_description"
  | "canonical"
  | "allow_comments"
>

export interface PostPage {
  items: PostSummary[]
  total: number
  limit: number
  offset: number
  /** Counts across every page, not just this one. */
  counts: Record<PostStatus, number>
}

export function getPosts(
  locale?: string,
  options: { limit?: number; offset?: number } = {}
): Promise<PostPage> {
  const params = new URLSearchParams()
  if (locale) params.set("locale", locale)
  if (options.limit !== undefined) params.set("limit", String(options.limit))
  if (options.offset !== undefined) params.set("offset", String(options.offset))
  const query = params.toString()
  return request<PostPage>(`/posts${query ? `?${query}` : ""}`)
}

/**
 * Asks the server for the address of a piece of text.
 *
 * The server is the only place that decides what a slug looks like, so the
 * address shown while typing is the one that will actually be stored.
 */
export function getSlug(text: string): Promise<string> {
  return request<{ slug: string }>(
    `/slug?text=${encodeURIComponent(text)}`
  ).then((response) => response.slug)
}

export function getPost(id: string): Promise<Post> {
  return request<Post>(`/posts/${id}`)
}

export function createPost(payload: PostPayload): Promise<Post> {
  return request<Post>("/posts", {
    method: "POST",
    body: JSON.stringify(payload),
  })
}

export function updatePost(id: string, payload: PostPayload): Promise<Post> {
  return request<Post>(`/posts/${id}`, {
    method: "PUT",
    body: JSON.stringify(payload),
  })
}

export function deletePost(id: string): Promise<void> {
  return request<void>(`/posts/${id}`, { method: "DELETE" })
}

export function setTranslationGroup(
  id: string,
  payload: { join: string } | { detach: true }
): Promise<Post> {
  return request<Post>(`/posts/${id}/translation-group`, {
    method: "PATCH",
    body: JSON.stringify(payload),
  })
}

export interface Language {
  code: string
  name: string
  native_name: string
  direction: string
  is_default: boolean
  is_active: boolean
  sort_order: number
}

export function getLanguages(): Promise<Language[]> {
  return request<Language[]>("/languages")
}

export function createLanguage(payload: {
  code: string
  name?: string
  native_name?: string
  direction?: string
}): Promise<Language> {
  return request<Language>("/languages", {
    method: "POST",
    body: JSON.stringify(payload),
  })
}

export function updateLanguage(
  code: string,
  payload: Partial<
    Pick<
      Language,
      "name" | "native_name" | "direction" | "is_active" | "is_default"
    >
  >
): Promise<Language> {
  return request<Language>(`/languages/${code}`, {
    method: "PUT",
    body: JSON.stringify(payload),
  })
}

/** `force` is required once the language holds content — see the API docs. */
export function deleteLanguage(
  code: string,
  force?: { mode: "reassign"; to: string } | { mode: "delete_content" }
): Promise<void> {
  const params = force
    ? `?force=${force.mode}${force.mode === "reassign" ? `&to=${force.to}` : ""}`
    : ""
  return request<void>(`/languages/${code}${params}`, { method: "DELETE" })
}

export interface Plugin {
  id: string
  name: string
  description: string
  enabled: boolean
  configured: boolean
}

export function getPlugins(): Promise<Plugin[]> {
  return request<Plugin[]>("/plugins")
}

export interface S3Settings {
  enabled: boolean
  endpoint: string
  region: string
  bucket: string
  access_key_id: string
  public_base_url: string
  path_prefix: string
  has_secret_access_key: boolean
}

/** `secret_access_key` is omitted to keep the stored one unchanged. */
export interface S3SettingsPayload {
  enabled: boolean
  endpoint: string
  region: string
  bucket: string
  access_key_id: string
  secret_access_key?: string
  public_base_url: string
  path_prefix: string
}

export interface ConnectionTest {
  ok: boolean
  message: string
}

export function getS3Settings(): Promise<S3Settings> {
  return request<S3Settings>("/plugins/s3")
}

export function saveS3Settings(
  payload: S3SettingsPayload
): Promise<S3Settings> {
  return request<S3Settings>("/plugins/s3", {
    method: "PUT",
    body: JSON.stringify(payload),
  })
}

export function testS3Settings(
  payload: S3SettingsPayload
): Promise<ConnectionTest> {
  return request<ConnectionTest>("/plugins/s3/test", {
    method: "POST",
    body: JSON.stringify(payload),
  })
}

export type BackupDestination = "local" | "s3"
export type BackupSchedule = "off" | "hourly" | "daily" | "weekly"

export interface BackupConfig {
  destination: BackupDestination
  folder: string
  include_media: boolean
  schedule: BackupSchedule
  keep: number
  last_run_at: string | null
  last_error: string | null
}

export interface BackupFile {
  name: string
  size_bytes: number
  created_at: string
}

export interface BackupSettings {
  enabled: boolean
  config: BackupConfig
  backups: BackupFile[]
  /** S3 is only offered as a destination once that plugin is set up. */
  s3_available: boolean
  s3_bucket: string | null
}

export function getBackupSettings(): Promise<BackupSettings> {
  return request<BackupSettings>("/plugins/backup")
}

export function saveBackupSettings(
  enabled: boolean,
  config: BackupConfig
): Promise<BackupSettings> {
  return request<BackupSettings>("/plugins/backup", {
    method: "PUT",
    body: JSON.stringify({ enabled, config }),
  })
}

export function runBackup(): Promise<BackupFile> {
  return request<BackupFile>("/plugins/backup/run", { method: "POST" })
}

export function deleteBackup(name: string): Promise<void> {
  return request<void>(`/plugins/backup/${encodeURIComponent(name)}`, {
    method: "DELETE",
  })
}

export interface Site {
  id: string
  host: string
  slug: string
  schema: string
  database_url: string
  active: boolean
}

export function getSites(): Promise<Site[]> {
  return request<Site[]>("/sites")
}

export function createSite(
  host: string,
  databaseUrl: string
): Promise<Site> {
  return request<Site>("/sites", {
    method: "POST",
    body: JSON.stringify({ host, database_url: databaseUrl }),
  })
}

export interface ConsoleAccount {
  name: string
  email: string
  organization_name: string
  site_limit: number
}

export interface ConsoleSite {
  id: string
  host: string
  slug: string
  active: boolean
}

export function consoleRegister(payload: {
  organization_name: string
  name: string
  email: string
  password: string
}): Promise<ConsoleAccount> {
  return request<ConsoleAccount>("/console/register", {
    method: "POST",
    body: JSON.stringify(payload),
  })
}

export function consoleLogin(
  email: string,
  password: string
): Promise<ConsoleAccount> {
  return request<ConsoleAccount>("/console/login", {
    method: "POST",
    body: JSON.stringify({ email, password }),
  })
}

export function consoleLogout(): Promise<void> {
  return request<void>("/console/logout", { method: "POST" })
}

export function getConsoleAccount(): Promise<ConsoleAccount> {
  return request<ConsoleAccount>("/console/me")
}

export function getConsoleSites(): Promise<ConsoleSite[]> {
  return request<ConsoleSite[]>("/console/sites")
}

export function createConsoleSite(host: string): Promise<ConsoleSite> {
  return request<ConsoleSite>("/console/sites", {
    method: "POST",
    body: JSON.stringify({ host }),
  })
}

/** A one-time link that opens the site already signed in. */
export function createSiteEntry(id: string): Promise<{ url: string }> {
  return request<{ url: string }>(`/console/sites/${id}/entry`, {
    method: "POST",
  })
}

export function enterSite(token: string): Promise<void> {
  return request<void>("/enter", {
    method: "POST",
    body: JSON.stringify({ token }),
  })
}

export interface BuildConfig {
  repository: string
  branch: string
  build_command: string
  output_dir: string
  /** Whether a token is stored; the token itself never comes back. */
  has_token: boolean
  /** The names the build runs with. The values stay on the server. */
  environment_keys: string[]
}

export type BuildStatus = "queued" | "running" | "succeeded" | "failed"

export interface Build {
  id: string
  status: BuildStatus
  log: string
  requested_at: string
  started_at: string | null
  finished_at: string | null
}

export interface PublishStatus {
  config: BuildConfig | null
  builds: Build[]
}

export function getPublish(): Promise<PublishStatus> {
  return request<PublishStatus>("/publish")
}

export function savePublish(payload: {
  repository: string
  branch?: string
  build_command?: string
  output_dir?: string
  /** Left out keeps the stored token; empty removes it. */
  token?: string
  /** Left out keeps what is stored. */
  environment?: Record<string, string>
}): Promise<BuildConfig> {
  return request<BuildConfig>("/publish", {
    method: "PUT",
    body: JSON.stringify(payload),
  })
}

export function requestPublish(): Promise<Build> {
  return request<Build>("/publish", { method: "POST" })
}

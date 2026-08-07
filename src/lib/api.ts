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
  /** This address is the server itself, not one of the sites it hosts. */
  server: boolean
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

export interface RestoreReport {
  taken_at: string
  /** Rows written, by table. */
  tables: Record<string, number>
  media_files: number
}

export function restoreBackup(name: string): Promise<RestoreReport> {
  return request<RestoreReport>(
    `/plugins/backup/${encodeURIComponent(name)}/restore`,
    { method: "POST" }
  )
}

/** Sends an archive taken somewhere else. Replaces everything this site has. */
export async function importBackup(file: File): Promise<RestoreReport> {
  const response = await fetch("/api/plugins/backup/import", {
    method: "POST",
    headers: { "Content-Type": "application/gzip" },
    body: file,
  })

  if (!response.ok) {
    const body = await response
      .json()
      .catch(() => ({ error: response.statusText }))
    throw new ApiError(response.status, body.error ?? response.statusText)
  }

  return response.json() as Promise<RestoreReport>
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

export function registrationIsOpen(): Promise<{ open: boolean }> {
  return request<{ open: boolean }>("/console/registration")
}

export function consoleRegister(payload: {
  organization_name: string
  name: string
  email: string
  password: string
  invite: string
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
  /** Null for a site whose agency keeps these settings — see managed_by. */
  config: BuildConfig | null
  /** Whether publishing has somewhere to build from, config visible or not. */
  configured: boolean
  builds: Build[]
  /** The agency that looks after how this site is built, if one does. */
  managed_by: string | null
}

export interface SavePublish {
  repository: string
  branch?: string
  build_command?: string
  output_dir?: string
  /** Left out keeps the stored token; empty removes it. */
  token?: string
  /** Replaces every variable. Left out keeps them. */
  environment?: Record<string, string>
  /** Adds these, leaving the rest alone. */
  environment_set?: Record<string, string>
  /** Removes these by name. */
  environment_remove?: string[]
}

export function getPublish(): Promise<PublishStatus> {
  return request<PublishStatus>("/publish")
}

export function savePublish(payload: SavePublish): Promise<BuildConfig> {
  return request<BuildConfig>("/publish", {
    method: "PUT",
    body: JSON.stringify(payload),
  })
}

export function requestPublish(): Promise<Build> {
  return request<Build>("/publish", { method: "POST" })
}

export function getSitePublish(id: string): Promise<PublishStatus> {
  return request<PublishStatus>(`/console/sites/${id}/publish`)
}

export function saveSitePublish(
  id: string,
  payload: SavePublish
): Promise<BuildConfig> {
  return request<BuildConfig>(`/console/sites/${id}/publish`, {
    method: "PUT",
    body: JSON.stringify(payload),
  })
}

export function requestSitePublish(id: string): Promise<Build> {
  return request<Build>(`/console/sites/${id}/publish`, { method: "POST" })
}

export interface SiteUser {
  id: string
  username: string
  email: string
  role: string
  /** False for the account an agency arrives on: no password, link only. */
  can_sign_in: boolean
  /** The server's own account, not this site's: shown but not editable. */
  managed: boolean
  created_at: string
}

export function getUsers(): Promise<SiteUser[]> {
  return request<SiteUser[]>("/users")
}

export function createUser(payload: {
  username: string
  email: string
  password: string
}): Promise<SiteUser> {
  return request<SiteUser>("/users", {
    method: "POST",
    body: JSON.stringify(payload),
  })
}

export function updateUser(
  id: string,
  payload: { email?: string; password?: string }
): Promise<SiteUser> {
  return request<SiteUser>(`/users/${id}`, {
    method: "PUT",
    body: JSON.stringify(payload),
  })
}

export function deleteUser(id: string): Promise<void> {
  return request<void>(`/users/${id}`, { method: "DELETE" })
}

export function changeOwnPassword(
  currentPassword: string,
  newPassword: string
): Promise<void> {
  return request<void>("/me/password", {
    method: "POST",
    body: JSON.stringify({
      current_password: currentPassword,
      new_password: newPassword,
    }),
  })
}

export function updateConsoleAccount(payload: {
  current_password: string
  name?: string
  email?: string
  new_password?: string
}): Promise<ConsoleAccount> {
  return request<ConsoleAccount>("/console/me", {
    method: "PUT",
    body: JSON.stringify(payload),
  })
}

export interface Agency {
  id: string
  name: string
  email: string
  site_limit: number
  active: boolean
  sites: number
}

export function getAgencies(): Promise<Agency[]> {
  return request<Agency[]>("/agencies")
}

export interface Invite {
  token: string
  site_limit: number
  note: string
  created_at: string
  expires_at: string
  used: boolean
  organization: string | null
}

export function getInvites(): Promise<Invite[]> {
  return request<Invite[]>("/invites")
}

export function createInvite(payload: {
  site_limit?: number
  note?: string
  days?: number
}): Promise<Invite> {
  return request<Invite>("/invites", {
    method: "POST",
    body: JSON.stringify(payload),
  })
}

export function revokeInvite(token: string): Promise<void> {
  return request<void>(`/invites/${token}`, { method: "DELETE" })
}

export function updateAgency(
  id: string,
  payload: { site_limit?: number; active?: boolean }
): Promise<void> {
  return request<void>(`/agencies/${id}`, {
    method: "PUT",
    body: JSON.stringify(payload),
  })
}

export function updateSite(
  id: string,
  payload: { active?: boolean }
): Promise<void> {
  return request<void>(`/sites/${id}`, {
    method: "PUT",
    body: JSON.stringify(payload),
  })
}

/** The address has to be typed back: a site is somebody's work. */
export function deleteSite(id: string, host: string): Promise<void> {
  return request<void>(`/sites/${id}/delete`, {
    method: "POST",
    body: JSON.stringify({ host }),
  })
}

export function getSiteS3(id: string): Promise<S3Settings> {
  return request<S3Settings>(`/console/sites/${id}/plugins/s3`)
}

export function saveSiteS3(
  id: string,
  payload: S3SettingsPayload
): Promise<S3Settings> {
  return request<S3Settings>(`/console/sites/${id}/plugins/s3`, {
    method: "PUT",
    body: JSON.stringify(payload),
  })
}

export function getSiteBackup(id: string): Promise<BackupSettings> {
  return request<BackupSettings>(`/console/sites/${id}/plugins/backup`)
}

export function saveSiteBackup(
  id: string,
  enabled: boolean,
  config: BackupConfig
): Promise<BackupSettings> {
  return request<BackupSettings>(`/console/sites/${id}/plugins/backup`, {
    method: "PUT",
    body: JSON.stringify({ enabled, config }),
  })
}

export function runSiteBackup(id: string): Promise<BackupFile> {
  return request<BackupFile>(`/console/sites/${id}/plugins/backup/run`, {
    method: "POST",
  })
}

export function restoreSiteBackup(
  id: string,
  name: string
): Promise<RestoreReport> {
  return request<RestoreReport>(
    `/console/sites/${id}/plugins/backup/${encodeURIComponent(name)}/restore`,
    { method: "POST" }
  )
}

export type FormFieldKind =
  | "text"
  | "textarea"
  | "email"
  | "phone"
  | "number"
  | "checkbox"
  | "select"
  | "date"
  | "url"

export interface FormField {
  name: string
  label: string
  type: FormFieldKind
  required: boolean
  options: string[]
}

export interface SiteForm {
  id: string
  name: string
  /** Where a submission is announced. Empty means nowhere. */
  notify: string
  /** The last part of the address other software posts to. */
  slug: string
  description: string
  fields: FormField[]
  active: boolean
  submissions: number
  unseen: number
  created_at: string
  updated_at: string
}

export interface FormSubmission {
  id: string
  form_id: string
  data: Record<string, unknown>
  seen: boolean
  created_at: string
}

export interface SaveFormPayload {
  name: string
  notify: string
  slug?: string
  description: string
  fields: FormField[]
  active: boolean
}

export function getForms(): Promise<SiteForm[]> {
  return request<SiteForm[]>("/forms")
}

export function getForm(id: string): Promise<SiteForm> {
  return request<SiteForm>(`/forms/${id}`)
}

export function createForm(payload: SaveFormPayload): Promise<SiteForm> {
  return request<SiteForm>("/forms", {
    method: "POST",
    body: JSON.stringify(payload),
  })
}

export function updateForm(
  id: string,
  payload: SaveFormPayload
): Promise<SiteForm> {
  return request<SiteForm>(`/forms/${id}`, {
    method: "PUT",
    body: JSON.stringify(payload),
  })
}

export function deleteForm(id: string): Promise<void> {
  return request<void>(`/forms/${id}`, { method: "DELETE" })
}

export function getFormSubmissions(id: string): Promise<FormSubmission[]> {
  return request<FormSubmission[]>(`/forms/${id}/submissions`)
}

export function markFormSeen(id: string): Promise<void> {
  return request<void>(`/forms/${id}/seen`, { method: "POST" })
}

export function deleteFormSubmission(
  formId: string,
  submissionId: string
): Promise<void> {
  return request<void>(`/forms/${formId}/submissions/${submissionId}`, {
    method: "DELETE",
  })
}

export interface DevelopmentToken {
  id: string
  created_at: string
  expires_at: string
}

export interface SiteDevelopment {
  api_url: string
  site_url: string
  /** Names only — the values never leave the server. */
  variables: string[]
  tokens: DevelopmentToken[]
}

export function getSiteDevelopment(id: string): Promise<SiteDevelopment> {
  return request<SiteDevelopment>(`/console/sites/${id}/development`)
}

export function createSiteDevelopmentToken(
  id: string
): Promise<{ token: string; expires_at: string }> {
  return request<{ token: string; expires_at: string }>(
    `/console/sites/${id}/development/tokens`,
    { method: "POST" }
  )
}

export function deleteSiteDevelopmentToken(
  id: string,
  tokenId: string
): Promise<void> {
  return request<void>(`/console/sites/${id}/development/tokens/${tokenId}`, {
    method: "DELETE",
  })
}

export interface ConnectionRequest {
  /** The program's own word for itself. Not something the site can check. */
  client_name: string
  redirect_uri: string
  site_title: string
  username: string
}

/**
 * What a program is asking for, so somebody can be shown it before agreeing.
 *
 * The query is passed through untouched rather than parsed and rebuilt: the
 * server checks it, and a parameter this panel did not know about would
 * otherwise be silently dropped on the way.
 */
export function describeConnectionRequest(
  query: string
): Promise<ConnectionRequest> {
  return request<ConnectionRequest>(`/oauth/request${query}`)
}

export function approveConnection(
  query: string
): Promise<{ redirect: string }> {
  const asked = new URLSearchParams(query)
  return request<{ redirect: string }>("/oauth/grant", {
    method: "POST",
    body: JSON.stringify(Object.fromEntries(asked)),
  })
}

export interface Connection {
  id: string
  client_name: string
  username: string
  created_at: string
  renewed_at: string
}

/** The assistants connected to this site. */
export function listConnections(): Promise<Connection[]> {
  return request<Connection[]>("/oauth/connections")
}

export function disconnect(id: string): Promise<void> {
  return request<void>(`/oauth/connections/${id}`, { method: "DELETE" })
}

/** What an agency hands to an assistant. Acts for the agency, not for a site. */
export function listConsoleTokens(): Promise<DevelopmentToken[]> {
  return request<DevelopmentToken[]>("/console/tokens")
}

export function createConsoleToken(): Promise<{
  token: string
  expires_at: string
}> {
  return request<{ token: string; expires_at: string }>("/console/tokens", {
    method: "POST",
  })
}

export function deleteConsoleToken(id: string): Promise<void> {
  return request<void>(`/console/tokens/${id}`, { method: "DELETE" })
}

/** The same tokens, from the site's own panel rather than an agency console. */
export function listBuildTokens(): Promise<DevelopmentToken[]> {
  return request<DevelopmentToken[]>("/development/tokens")
}

export function createBuildToken(): Promise<{
  token: string
  expires_at: string
}> {
  return request<{ token: string; expires_at: string }>("/development/tokens", {
    method: "POST",
  })
}

export function deleteBuildToken(tokenId: string): Promise<void> {
  return request<void>(`/development/tokens/${tokenId}`, { method: "DELETE" })
}

/**
 * What this site tells a program about itself, as text.
 *
 * Not `request`, which speaks JSON. Fetched with the session so the copy
 * carries this site's own languages and forms; fetched by anything else it is
 * the same document without them.
 */
export async function getLlmsText(): Promise<string> {
  const response = await fetch("/api/llms.txt")
  if (!response.ok) {
    throw new ApiError(response.status, response.statusText)
  }
  return response.text()
}

export interface Sender {
  address: string
  name: string
}

export interface EmailSettings {
  enabled: boolean
  region: string
  access_key_id: string
  from_address: string
  from_name: string
  reply_to: string
  configuration_set: string
  /** The secret itself never comes back; this says whether one is stored. */
  has_secret_access_key: boolean
  /** The addresses beyond the default one. */
  senders: Sender[]
  /** The unguessable part of the address Amazon posts events to. */
  events_token: string
}

export interface EmailSettingsPayload {
  enabled: boolean
  region: string
  access_key_id: string
  /** Left out to keep the stored secret. */
  secret_access_key?: string
  from_address: string
  from_name: string
  reply_to: string
  configuration_set: string
  senders: Sender[]
}

export function getEmailSettings(): Promise<EmailSettings> {
  return request<EmailSettings>("/plugins/email")
}

export function updateEmailSettings(
  payload: EmailSettingsPayload
): Promise<EmailSettings> {
  return request<EmailSettings>("/plugins/email", {
    method: "PUT",
    body: JSON.stringify(payload),
  })
}

export function testEmailSettings(to: string): Promise<ConnectionTest> {
  return request<ConnectionTest>("/plugins/email/test", {
    method: "POST",
    body: JSON.stringify({ to }),
  })
}

export function getSiteEmailSettings(id: string): Promise<EmailSettings> {
  return request<EmailSettings>(`/console/sites/${id}/plugins/email`)
}

export function saveSiteEmailSettings(
  id: string,
  payload: EmailSettingsPayload
): Promise<EmailSettings> {
  return request<EmailSettings>(`/console/sites/${id}/plugins/email`, {
    method: "PUT",
    body: JSON.stringify(payload),
  })
}

export function testSiteEmailSettings(
  id: string,
  to: string
): Promise<ConnectionTest> {
  return request<ConnectionTest>(`/console/sites/${id}/plugins/email/test`, {
    method: "POST",
    body: JSON.stringify({ to }),
  })
}

export interface SesAccount {
  production_access: boolean
  sending_enabled: boolean
  enforcement_status: string
  max_24_hour_send: number
  max_send_rate: number
  sent_last_24_hours: number
  mail_type: string
  website_url: string
  use_case_description: string
  contact_language: string
  additional_contacts: string[]
  review_status: string
}

export interface SesDnsRecord {
  kind: string
  host: string
  value: string
  /** "dkim", "mail_from" or "dmarc". */
  purpose: string
  /** "verified", "waiting", "failed", or "unchecked" for DMARC. */
  status: string
  required: boolean
}

export interface SesIdentity {
  name: string
  kind: string
  verified: boolean
  dkim_status: string
  mail_from_domain: string
  mail_from_status: string
  /** Everything to publish at the registrar, in one list. */
  records: SesDnsRecord[]
}

export interface SesSuppressed {
  address: string
  reason: string
  since: string
}

export interface ProductionAccessPayload {
  mail_type: string
  website_url: string
  use_case_description: string
  contact_language: string
  additional_contacts: string[]
}

/**
 * The SES account, from whichever side is asking.
 *
 * A site reads its own under Plugins; an agency reads its sites' from the
 * console. Same questions, same answers, so the calls are one shape with the
 * site id supplied or not.
 */
const mailBase = (siteId?: string) =>
  siteId ? `/console/sites/${siteId}/plugins/email` : "/plugins/email"

export function getSesAccount(siteId?: string): Promise<SesAccount> {
  return request<SesAccount>(`${mailBase(siteId)}/account`)
}

export function requestProductionAccess(
  payload: ProductionAccessPayload,
  siteId?: string
): Promise<void> {
  return request<void>(`${mailBase(siteId)}/production-access`, {
    method: "POST",
    body: JSON.stringify(payload),
  })
}

export function getSesIdentities(siteId?: string): Promise<SesIdentity[]> {
  return request<SesIdentity[]>(`${mailBase(siteId)}/identities`)
}

export function addSesIdentity(name: string, siteId?: string): Promise<void> {
  return request<void>(`${mailBase(siteId)}/identities`, {
    method: "POST",
    body: JSON.stringify({ name }),
  })
}

export function deleteSesIdentity(
  name: string,
  siteId?: string
): Promise<void> {
  return request<void>(
    `${mailBase(siteId)}/identities/${encodeURIComponent(name)}`,
    { method: "DELETE" }
  )
}

export function setSesMailFrom(
  identity: string,
  subdomain: string,
  siteId?: string
): Promise<void> {
  return request<void>(
    `${mailBase(siteId)}/identities/${encodeURIComponent(identity)}/mail-from`,
    { method: "POST", body: JSON.stringify({ subdomain }) }
  )
}

export function createSesConfigurationSet(
  name: string,
  siteId?: string
): Promise<void> {
  return request<void>(`${mailBase(siteId)}/configuration-sets`, {
    method: "POST",
    body: JSON.stringify({ name }),
  })
}

export function getSesSuppressed(siteId?: string): Promise<SesSuppressed[]> {
  return request<SesSuppressed[]>(`${mailBase(siteId)}/suppressed`)
}

export function unsuppressSesAddress(
  address: string,
  siteId?: string
): Promise<void> {
  return request<void>(
    `${mailBase(siteId)}/suppressed/${encodeURIComponent(address)}`,
    { method: "DELETE" }
  )
}

export interface MailList {
  id: string
  name: string
  slug: string
  description: string
  opt_in: string
  public: boolean
  confirmed: number
  unconfirmed: number
  unsubscribed: number
  created_at: string
}

export interface Subscriber {
  id: string
  email: string
  name: string
  status: string
  attributes: Record<string, unknown>
  lists: { list_id: string; status: string }[]
  created_at: string
}

export interface MailTemplate {
  id: string
  name: string
  subject: string
  body: string
  is_default: boolean
  created_at: string
}

export interface Campaign {
  id: string
  name: string
  subject: string
  body: string
  template_id: string | null
  /** Which of the site's addresses it goes out as. Empty for the default. */
  from_address: string
  status: string
  lists: string[]
  send_at: string | null
  started_at: string | null
  finished_at: string | null
  to_send: number
  sent: number
  failed: number
  opened: number
  clicked: number
  created_at: string
}

export interface MailLogEntry {
  id: string
  to_address: string
  subject: string
  status: string
  detail: string
  created_at: string
}

export interface ImportReport {
  added: number
  updated: number
  skipped: string[]
}

export function getMailLists(): Promise<MailList[]> {
  return request<MailList[]>("/mail/lists")
}

export function saveMailList(
  payload: { name: string; description: string; opt_in: string; public: boolean },
  id?: string
): Promise<MailList> {
  return request<MailList>(id ? `/mail/lists/${id}` : "/mail/lists", {
    method: id ? "PUT" : "POST",
    body: JSON.stringify(payload),
  })
}

export function deleteMailList(id: string): Promise<void> {
  return request<void>(`/mail/lists/${id}`, { method: "DELETE" })
}

export function getSubscribers(params: {
  q?: string
  list?: string
}): Promise<Subscriber[]> {
  const search = new URLSearchParams()
  if (params.q) search.set("q", params.q)
  if (params.list) search.set("list", params.list)
  const query = search.toString()
  return request<Subscriber[]>(`/mail/subscribers${query ? `?${query}` : ""}`)
}

export function saveSubscriber(
  payload: {
    email: string
    name: string
    lists: string[]
    attributes: Record<string, unknown>
    status?: string
  },
  id?: string
): Promise<Subscriber> {
  return request<Subscriber>(
    id ? `/mail/subscribers/${id}` : "/mail/subscribers",
    { method: id ? "PUT" : "POST", body: JSON.stringify(payload) }
  )
}

export function deleteSubscriber(id: string): Promise<void> {
  return request<void>(`/mail/subscribers/${id}`, { method: "DELETE" })
}

export async function importSubscribers(
  listId: string,
  csv: string
): Promise<ImportReport> {
  const response = await fetch(
    `/api/mail/subscribers/import?list=${encodeURIComponent(listId)}`,
    { method: "POST", headers: { "Content-Type": "text/csv" }, body: csv }
  )
  if (!response.ok) {
    const body = await response.json().catch(() => ({}))
    throw new ApiError(response.status, body.error ?? response.statusText)
  }
  return response.json() as Promise<ImportReport>
}

/** The address the browser downloads from; not fetched here. */
export function subscriberExportUrl(listId?: string): string {
  return `/api/mail/subscribers/export${listId ? `?list=${encodeURIComponent(listId)}` : ""}`
}

export function getMailTemplates(): Promise<MailTemplate[]> {
  return request<MailTemplate[]>("/mail/templates")
}

export function saveMailTemplate(
  payload: { name: string; subject: string; body: string; is_default: boolean },
  id?: string
): Promise<MailTemplate> {
  return request<MailTemplate>(id ? `/mail/templates/${id}` : "/mail/templates", {
    method: id ? "PUT" : "POST",
    body: JSON.stringify(payload),
  })
}

export function deleteMailTemplate(id: string): Promise<void> {
  return request<void>(`/mail/templates/${id}`, { method: "DELETE" })
}

export function getCampaigns(): Promise<Campaign[]> {
  return request<Campaign[]>("/mail/campaigns")
}

export function getCampaign(id: string): Promise<Campaign> {
  return request<Campaign>(`/mail/campaigns/${id}`)
}

export function saveCampaign(
  payload: {
    name: string
    subject: string
    body: string
    template_id: string | null
    from_address: string
    lists: string[]
    send_at: string | null
  },
  id?: string
): Promise<Campaign> {
  return request<Campaign>(id ? `/mail/campaigns/${id}` : "/mail/campaigns", {
    method: id ? "PUT" : "POST",
    body: JSON.stringify(payload),
  })
}

export function deleteCampaign(id: string): Promise<void> {
  return request<void>(`/mail/campaigns/${id}`, { method: "DELETE" })
}

export function startCampaign(id: string): Promise<Campaign> {
  return request<Campaign>(`/mail/campaigns/${id}/send`, { method: "POST" })
}

export function pauseCampaign(id: string): Promise<Campaign> {
  return request<Campaign>(`/mail/campaigns/${id}/pause`, { method: "POST" })
}

export function cancelCampaign(id: string): Promise<Campaign> {
  return request<Campaign>(`/mail/campaigns/${id}/cancel`, { method: "POST" })
}

export function testCampaign(id: string, to: string): Promise<void> {
  return request<void>(`/mail/campaigns/${id}/test`, {
    method: "POST",
    body: JSON.stringify({ to }),
  })
}

export function getMailLog(): Promise<MailLogEntry[]> {
  return request<MailLogEntry[]>("/mail/log")
}

export interface SesHealth {
  delivery_attempts: number
  bounces: number
  complaints: number
  rejects: number
  bounce_rate: number
  complaint_rate: number
  /** "healthy", "watch" or "danger", against Amazon's own thresholds. */
  bounce_standing: string
  complaint_standing: string
  bounce_review_at: number
  bounce_pause_at: number
  complaint_review_at: number
  complaint_pause_at: number
  days: { day: string; delivery_attempts: number; bounces: number; complaints: number }[]
}

export interface SesRequest {
  id: string
  subject: string
  status: string
  created_at: string
  latest: string
}

export interface QuotaIncreasePayload {
  daily_limit: number
  send_rate: number
  website_url: string
  use_case_description: string
  language: string
}

export function getSesHealth(siteId?: string): Promise<SesHealth> {
  return request<SesHealth>(`${mailBase(siteId)}/health`)
}

export function setSesSending(
  enabled: boolean,
  siteId?: string
): Promise<void> {
  return request<void>(`${mailBase(siteId)}/sending`, {
    method: "POST",
    body: JSON.stringify({ enabled }),
  })
}

export function getSesConfigurationSets(siteId?: string): Promise<string[]> {
  return request<string[]>(`${mailBase(siteId)}/configuration-sets`)
}

export function requestQuotaIncrease(
  payload: QuotaIncreasePayload,
  siteId?: string
): Promise<SesRequest> {
  return request<SesRequest>(`${mailBase(siteId)}/quota-increase`, {
    method: "POST",
    body: JSON.stringify(payload),
  })
}

export function getSesRequests(siteId?: string): Promise<SesRequest[]> {
  return request<SesRequest[]>(`${mailBase(siteId)}/requests`)
}

export interface SesPipeline {
  configuration_set: string
  topic_arn: string
  endpoint: string
  confirmed: boolean
}

export interface SesDeliverability {
  sent: number
  delivered: number
  opened: number
  clicked: number
  permanent_bounces: number
  transient_bounces: number
  complaints: number
  delivery_rate: number
  open_rate: number
  click_rate: number
}

export interface MailEvent {
  id: string
  kind: string
  address: string
  campaign_id: string | null
  detail: string
  created_at: string
}

export interface MailEventSummary {
  counts: Record<string, number>
  recent: MailEvent[]
}

export function setupSesEvents(siteId?: string): Promise<SesPipeline> {
  return request<SesPipeline>(`${mailBase(siteId)}/events/setup`, {
    method: "POST",
  })
}

export function getSesDeliverability(
  siteId?: string,
  days = 30
): Promise<SesDeliverability> {
  return request<SesDeliverability>(
    `${mailBase(siteId)}/deliverability${siteId ? "" : `?days=${days}`}`
  )
}

export function getMailEvents(): Promise<MailEventSummary> {
  return request<MailEventSummary>("/mail/events")
}

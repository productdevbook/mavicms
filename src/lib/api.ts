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
  description = ""
): Promise<Category> {
  return request<Category>("/categories", {
    method: "POST",
    body: JSON.stringify({ name, description, locale }),
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
  locale?: string
  /** Id of an existing post this is a translation of. */
  translation_of?: string
}

export function getPosts(locale?: string): Promise<Post[]> {
  return request<Post[]>(`/posts${locale ? `?locale=${locale}` : ""}`)
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
  payload: Partial<Pick<Language, "name" | "native_name" | "direction" | "is_active" | "is_default">>
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

export function saveS3Settings(payload: S3SettingsPayload): Promise<S3Settings> {
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

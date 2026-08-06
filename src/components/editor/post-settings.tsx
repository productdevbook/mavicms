import * as React from "react"
import { useLingui } from "@lingui/react/macro"

import { ImageOff, Plus, Sparkles, Upload, X } from "lucide-react"
import { toast } from "sonner"

import { cn } from "@/lib/utils"
import { slugify } from "@/lib/editor-utils"
import {
  getSlug,
  ApiError,
  createCategory,
  createTag,
  getCategories,
  getTags,
  uploadMedia,
  type Category,
  type Tag,
} from "@/lib/api"
import { toCategoryTree } from "@/lib/category-tree"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"
import { Textarea } from "@/components/ui/textarea"
import {
  useStatusLabels,
  type PostMeta,
  type PostStatus,
} from "@/components/editor/types"

interface PostSettingsProps {
  meta: PostMeta
  onChange: (patch: Partial<PostMeta>) => void
  /** Categories and tags are offered only in the post's own language. */
  locale: string
  plainText: string
}

export function PostSettings({
  meta,
  onChange,
  locale,
  plainText,
}: PostSettingsProps) {
  const { t } = useLingui()
  const [tagDraft, setTagDraft] = React.useState("")
  const [categories, setCategories] = React.useState<Category[]>([])
  const categoryRows = React.useMemo(
    () => toCategoryTree(categories),
    [categories]
  )
  const [tags, setTags] = React.useState<Tag[]>([])
  const [newCategory, setNewCategory] = React.useState("")
  const coverInputRef = React.useRef<HTMLInputElement>(null)

  const STATUS_LABELS = useStatusLabels()

  React.useEffect(() => {
    if (!locale) return
    getCategories(locale)
      .then(setCategories)
      .catch(() => {})
    getTags(locale)
      .then(setTags)
      .catch(() => {})
  }, [locale])

  const addCategory = async () => {
    const name = newCategory.trim()
    if (!name) return
    try {
      const created = await createCategory(name, locale)
      setCategories((current) =>
        current.some((category) => category.id === created.id)
          ? current
          : [...current, created]
      )
      onChange({ category: created.name, categoryId: created.id })
      setNewCategory("")
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not create category`
      )
    }
  }

  const addTag = async () => {
    const value = tagDraft.trim()
    if (!value || meta.tags.includes(value)) return
    setTagDraft("")
    onChange({ tags: [...meta.tags, value] })
    try {
      const created = await createTag(value, locale)
      setTags((current) =>
        current.some((tag) => tag.id === created.id)
          ? current
          : [...current, created]
      )
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not save tag`
      )
    }
  }

  const seoTitle = meta.seoTitle || meta.title
  const seoDescription = meta.seoDescription || meta.excerpt

  return (
    <div className="flex flex-col gap-5">
      <Field label={t`Status`} htmlFor="meta-status">
        <Select
          value={meta.status}
          onValueChange={(value) => onChange({ status: value as PostStatus })}
        >
          <SelectTrigger id="meta-status" className="w-full">
            <SelectValue>
              {(value: PostStatus | null) =>
                value ? STATUS_LABELS[value] : t`Select`
              }
            </SelectValue>
          </SelectTrigger>
          <SelectContent>
            {Object.entries(STATUS_LABELS).map(([value, label]) => (
              <SelectItem key={value} value={value}>
                {label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </Field>

      <Field label={t`Publish date`} htmlFor="meta-date">
        <Input
          id="meta-date"
          type="datetime-local"
          value={meta.publishAt}
          onChange={(event) => onChange({ publishAt: event.target.value })}
        />
      </Field>

      <Field
        label={t`Permalink`}
        htmlFor="meta-slug"
        hint={`/blog/${meta.slug || t`post-url`}`}
      >
        <div className="flex gap-1.5">
          <Input
            id="meta-slug"
            value={meta.slug}
            onChange={(event) => onChange({ slug: event.target.value })}
            placeholder={t`post-url`}
          />
          <Button
            variant="outline"
            size="icon"
            aria-label={t`Generate from title`}
            onClick={() => {
              void getSlug(meta.title)
                .then((slug) => onChange({ slug }))
                .catch(() => onChange({ slug: slugify(meta.title) }))
            }}
          >
            <Sparkles />
          </Button>
        </div>
      </Field>

      <Field label={t`Category`} htmlFor="meta-category">
        <Select
          value={meta.category}
          onValueChange={(value) => {
            const found = categories.find((category) => category.name === value)
            onChange({ category: value ?? "", categoryId: found?.id ?? null })
          }}
        >
          <SelectTrigger id="meta-category" className="w-full">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {categoryRows.map(({ category, depth }) => (
              <SelectItem key={category.id} value={category.name}>
                {"\u00a0".repeat(depth * 3)}
                {category.name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <div className="flex gap-1.5">
          <Input
            value={newCategory}
            placeholder={t`New category`}
            onChange={(event) => setNewCategory(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault()
                void addCategory()
              }
            }}
          />
          <Button
            type="button"
            variant="outline"
            size="icon"
            aria-label={t`Add category`}
            onClick={() => void addCategory()}
          >
            <Plus />
          </Button>
        </div>
      </Field>

      <Field label={t`Tags`} htmlFor="meta-tags">
        <div className="flex gap-1.5">
          <Input
            id="meta-tags"
            value={tagDraft}
            list="meta-tags-suggestions"
            placeholder={t`Add a tag and press Enter`}
            onChange={(event) => setTagDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault()
                void addTag()
              }
            }}
          />
          <datalist id="meta-tags-suggestions">
            {tags.map((tag) => (
              <option key={tag.id} value={tag.name} />
            ))}
          </datalist>
        </div>
        {meta.tags.length > 0 && (
          <div className="flex flex-wrap gap-1.5 pt-2">
            {meta.tags.map((tag) => (
              <Badge key={tag} variant="secondary" className="gap-1 pr-1">
                {tag}
                <button
                  type="button"
                  aria-label={t`Remove ${tag} tag`}
                  onClick={() =>
                    onChange({ tags: meta.tags.filter((item) => item !== tag) })
                  }
                  className="rounded-full p-0.5 hover:bg-foreground/10"
                >
                  <X className="size-3" />
                </button>
              </Badge>
            ))}
          </div>
        )}
      </Field>

      <Field
        label={t`Excerpt`}
        htmlFor="meta-excerpt"
        hint={`${meta.excerpt.length}/160`}
      >
        <Textarea
          id="meta-excerpt"
          value={meta.excerpt}
          maxLength={220}
          rows={3}
          placeholder={t`Short description shown on listing pages`}
          onChange={(event) => onChange({ excerpt: event.target.value })}
          className="resize-none"
        />
        <Button
          variant="ghost"
          size="sm"
          className="mt-1 self-start text-muted-foreground"
          onClick={() =>
            onChange({ excerpt: plainText.slice(0, 155).trim() + "…" })
          }
        >
          <Sparkles /> {t`Generate from post`}
        </Button>
      </Field>

      <Field label={t`Cover image`} htmlFor="meta-cover">
        <div className="flex gap-1.5">
          <Input
            id="meta-cover"
            value={meta.coverUrl}
            placeholder="https://…/cover.jpg"
            onChange={(event) => onChange({ coverUrl: event.target.value })}
          />
          <Button
            type="button"
            variant="outline"
            size="icon"
            aria-label={t`Upload cover image`}
            onClick={() => coverInputRef.current?.click()}
          >
            <Upload />
          </Button>
          <input
            ref={coverInputRef}
            type="file"
            accept="image/png,image/jpeg,image/gif,image/webp"
            hidden
            onChange={async (event) => {
              const file = event.target.files?.[0]
              event.target.value = ""
              if (!file) return
              try {
                const media = await uploadMedia(file)
                onChange({ coverUrl: media.url })
              } catch (error) {
                toast.error(
                  error instanceof ApiError
                    ? error.message
                    : t`Could not upload cover image`
                )
              }
            }}
          />
        </div>
        <div className="mt-2 flex aspect-video items-center justify-center overflow-hidden rounded-lg border border-dashed border-border bg-muted/40">
          {meta.coverUrl ? (
            <img
              src={meta.coverUrl}
              alt={t`Cover preview`}
              className="size-full object-cover"
            />
          ) : (
            <ImageOff className="size-5 text-muted-foreground" />
          )}
        </div>
      </Field>

      <div className="flex flex-col gap-3 rounded-xl border border-border p-3">
        <ToggleRow
          id="meta-featured"
          label={t`Featured`}
          description={t`Highlighted on the home page`}
          checked={meta.featured}
          onCheckedChange={(value) => onChange({ featured: value })}
        />
        <ToggleRow
          id="meta-comments"
          label={t`Comments`}
          description={t`Readers can leave comments`}
          checked={meta.allowComments}
          onCheckedChange={(value) => onChange({ allowComments: value })}
        />
      </div>

      <div className="flex flex-col gap-4 rounded-xl border border-border p-3">
        <p className="text-xs font-medium tracking-wide text-muted-foreground uppercase">
          {t`Search engine`}
        </p>
        <Field
          label={t`SEO title`}
          htmlFor="meta-seo-title"
          hint={`${seoTitle.length}/60`}
        >
          <Input
            id="meta-seo-title"
            value={meta.seoTitle}
            placeholder={meta.title}
            onChange={(event) => onChange({ seoTitle: event.target.value })}
          />
        </Field>
        <Field
          label={t`SEO description`}
          htmlFor="meta-seo-description"
          hint={`${seoDescription.length}/160`}
        >
          <Textarea
            id="meta-seo-description"
            rows={3}
            value={meta.seoDescription}
            placeholder={meta.excerpt || t`Text shown in search results`}
            onChange={(event) =>
              onChange({ seoDescription: event.target.value })
            }
            className="resize-none"
          />
        </Field>
        <Field label={t`Canonical URL`} htmlFor="meta-canonical">
          <Input
            id="meta-canonical"
            value={meta.canonical}
            placeholder="https://mavicms.dev/blog/…"
            onChange={(event) => onChange({ canonical: event.target.value })}
          />
        </Field>

        <div className="rounded-lg border border-border bg-muted/40 p-3">
          <p className="truncate text-xs text-muted-foreground">
            mavicms.dev › blog › {meta.slug || t`post`}
          </p>
          <p className="truncate text-sm font-medium text-primary">
            {seoTitle || t`Post title`}
          </p>
          <p className="line-clamp-2 text-xs text-muted-foreground">
            {seoDescription || t`Description text shown in search results.`}
          </p>
        </div>
      </div>
    </div>
  )
}

function Field({
  label,
  htmlFor,
  hint,
  children,
  className,
}: {
  label: string
  htmlFor: string
  hint?: string
  children: React.ReactNode
  className?: string
}) {
  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      <div className="flex items-baseline justify-between gap-2">
        <Label htmlFor={htmlFor}>{label}</Label>
        {hint && (
          <span className="truncate text-[0.7rem] text-muted-foreground">
            {hint}
          </span>
        )}
      </div>
      {children}
    </div>
  )
}

function ToggleRow({
  id,
  label,
  description,
  checked,
  onCheckedChange,
}: {
  id: string
  label: string
  description: string
  checked: boolean
  onCheckedChange: (value: boolean) => void
}) {
  return (
    <div className="flex items-start justify-between gap-3">
      <div className="flex flex-col">
        <Label htmlFor={id}>{label}</Label>
        <span className="text-xs text-muted-foreground">{description}</span>
      </div>
      <Switch id={id} checked={checked} onCheckedChange={onCheckedChange} />
    </div>
  )
}

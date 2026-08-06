import * as React from "react"
import { EditorContent, useEditor } from "@tiptap/react"
import type { TableOfContentData } from "@tiptap/extension-table-of-contents"
import { useLingui } from "@lingui/react/macro"
import { useNavigate } from "@tanstack/react-router"
import {
  Eye,
  EyeOff,
  FileDown,
  Focus as FocusIcon,
  Keyboard,
  Languages,
  LayoutDashboard,
  ListTree,
  Loader2,
  LogOut,
  Maximize,
  Minimize,
  PanelRightClose,
  PanelRightOpen,
  Save,
  Send,
} from "lucide-react"
import { toast } from "sonner"

import { cn } from "@/lib/utils"
import { shortcut, slugify } from "@/lib/editor-utils"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Kbd } from "@/components/ui/kbd"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Separator } from "@/components/ui/separator"
import { Toaster } from "@/components/ui/sonner"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import {
  ApiError,
  createPost,
  getPost,
  logout,
  updatePost,
  type Post,
  type PostPayload,
  type PostTranslation,
} from "@/lib/api"
import { useLanguages } from "@/lib/use-languages"
import { ModeToggle } from "@/components/mode-toggle"
import { LocaleToggle } from "@/components/locale-toggle"
import { BlockHandle } from "@/components/editor/block-handle"
import {
  ImageBubbleMenu,
  LinkBubbleMenu,
  TableBubbleMenu,
  TextBubbleMenu,
} from "@/components/editor/bubble-menus"
import { EditorDialogs } from "@/components/editor/dialogs"
import {
  openEditorDialog,
  onEditorDialog,
} from "@/components/editor/editor-events"
import { buildExtensions } from "@/components/editor/extensions"
import { FindReplacePanel } from "@/components/editor/find-replace"
import { PostSettings } from "@/components/editor/post-settings"
import { StatusBar, type SaveState } from "@/components/editor/status-bar"
import { TocPanel } from "@/components/editor/toc-panel"
import { Toolbar } from "@/components/editor/toolbar"
import { useStatusLabels, type PostMeta } from "@/components/editor/types"

const SCROLL_CONTAINER_ID = "mavi-editor-scroll"
// No cap. Posts are as long as they are, and a limit here does not merely
// refuse new typing: Tiptap rejects the whole transaction, so a post over the
// limit opens empty — and the next keystroke would autosave that emptiness
// over the real thing.
const CHARACTER_LIMIT = null
const BLANK_CONTENT = "<p></p>"

const BLANK_META: PostMeta = {
  title: "",
  slug: "",
  excerpt: "",
  status: "draft",
  publishAt: "",
  author: "",
  category: "",
  categoryId: null,
  tags: [],
  coverUrl: "",
  seoTitle: "",
  seoDescription: "",
  canonical: "",
  featured: false,
  allowComments: true,
}

function toLocalDateTimeInput(iso: string): string {
  const date = new Date(iso)
  const pad = (value: number) => String(value).padStart(2, "0")
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`
}

function postToMeta(post: Post): PostMeta {
  return {
    title: post.title,
    slug: post.slug,
    excerpt: post.excerpt,
    status: post.status,
    publishAt: post.publish_at ? toLocalDateTimeInput(post.publish_at) : "",
    author: post.author,
    category: post.category,
    categoryId: post.category_ids[0] ?? null,
    tags: post.tags,
    coverUrl: post.cover_url,
    seoTitle: post.seo_title,
    seoDescription: post.seo_description,
    canonical: post.canonical,
    featured: post.featured,
    allowComments: post.allow_comments,
  }
}

function metaToPayload(meta: PostMeta, contentHtml: string): PostPayload {
  return {
    title: meta.title,
    slug: meta.slug.trim() || slugify(meta.title),
    excerpt: meta.excerpt,
    status: meta.status,
    publish_at: meta.publishAt ? new Date(meta.publishAt).toISOString() : null,
    author: meta.author,
    category: meta.category,
    category_ids: meta.categoryId ? [meta.categoryId] : [],
    tags: meta.tags,
    cover_url: meta.coverUrl,
    seo_title: meta.seoTitle,
    seo_description: meta.seoDescription,
    canonical: meta.canonical,
    featured: meta.featured,
    allow_comments: meta.allowComments,
    content_html: contentHtml,
  }
}

export function MaviEditor({
  postId,
  locale: initialLocale,
  translationOf,
}: {
  postId: string | null
  locale?: string
  translationOf?: string
}) {
  const { t } = useLingui()
  const navigate = useNavigate()
  const STATUS_LABELS = useStatusLabels()
  const [meta, setMeta] = React.useState<PostMeta>(BLANK_META)
  const [currentPostId, setCurrentPostId] = React.useState<string | null>(
    postId
  )
  const { languages, defaultCode, label: languageLabel } = useLanguages()
  // Fixed for the lifetime of the post: a saved post's language never changes,
  // and a new one takes it from the URL so autosave can't guess wrong. Derived
  // so the default arriving from the API doesn't trigger a cascading render.
  const [loadedLocale, setLoadedLocale] = React.useState<string | null>(null)
  const locale = loadedLocale ?? initialLocale ?? defaultCode
  const [translations, setTranslations] = React.useState<PostTranslation[]>([])
  const [loading, setLoading] = React.useState(postId !== null)
  const [toc, setToc] = React.useState<TableOfContentData>([])
  const [saveState, setSaveState] = React.useState<SaveState>("idle")
  const [savedAt, setSavedAt] = React.useState<Date | null>(null)
  const [showToc, setShowToc] = React.useState(true)
  const [showSettings, setShowSettings] = React.useState(true)
  const [focusMode, setFocusMode] = React.useState(false)
  const [preview, setPreview] = React.useState(false)
  const [fullscreen, setFullscreen] = React.useState(false)
  const [findOpen, setFindOpen] = React.useState(false)

  const saveTimer = React.useRef<number | null>(null)

  const extensions = React.useMemo(
    () =>
      buildExtensions({
        characterLimit: CHARACTER_LIMIT,
        onTocUpdate: setToc,
        scrollParent: () =>
          document.getElementById(SCROLL_CONTAINER_ID) ?? window,
      }),
    []
  )

  const editor = useEditor({
    extensions,
    content: BLANK_CONTENT,
    autofocus: "start",
    editorProps: {
      attributes: {
        class: "mavi-prose focus:outline-none",
        spellcheck: "true",
      },
    },
  })

  React.useEffect(() => {
    if (!postId || !editor) return
    let cancelled = false
    getPost(postId)
      .then((post) => {
        if (cancelled) return
        setMeta(postToMeta(post))
        // Loading is not an edit: an update event here would schedule an
        // autosave against the still-empty initial meta and PATCH a blank title.
        editor.commands.setContent(post.content_html, { emitUpdate: false })

        // If the post had content and the editor ended up empty, something
        // refused it. Autosaving from here would write that emptiness over the
        // real thing, so the editor stays shut instead.
        if (post.content_html.trim() !== "" && editor.isEmpty) {
          toast.error(t`This post could not be opened, so it has been left untouched.`)
          navigate({ to: "/dashboard" })
          return
        }

        setSavedAt(new Date(post.updated_at))
        setLoadedLocale(post.locale)
        setTranslations(post.translations)
        setLoading(false)
      })
      .catch(() => {
        if (cancelled) return
        toast.error(t`Could not load post`)
        navigate({ to: "/dashboard" })
      })
    return () => {
      cancelled = true
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- runs once per postId once the editor instance is ready
  }, [postId, editor])

  const persist = React.useCallback(
    async (nextMeta: PostMeta, options?: { notify?: boolean }) => {
      if (!editor) return
      // A brand-new, untitled post has nothing worth saving yet (and the
      // backend rejects an empty title) — wait for the user to type one
      // instead of surfacing a validation error on every keystroke-free tick.
      if (!currentPostId && !nextMeta.title.trim()) {
        if (options?.notify) toast.error(t`Give your post a title first`)
        return
      }
      setSaveState("saving")
      const payload = metaToPayload(nextMeta, editor.getHTML())
      try {
        if (currentPostId) {
          const saved = await updatePost(currentPostId, payload)
          setSavedAt(new Date(saved.updated_at))
          setTranslations(saved.translations)
        } else {
          const saved = await createPost({
            ...payload,
            locale,
            translation_of: translationOf,
          })
          setCurrentPostId(saved.id)
          setSavedAt(new Date(saved.updated_at))
          setTranslations(saved.translations)
          void navigate({
            to: "/editor/$postId",
            params: { postId: saved.id },
            replace: true,
          })
        }
        setSaveState("saved")
        if (options?.notify) toast.success(t`Draft saved`)
      } catch (error) {
        setSaveState("idle")
        toast.error(
          error instanceof ApiError ? error.message : t`Could not save post`
        )
      }
    },
    [editor, currentPostId, navigate, t, locale, translationOf]
  )

  const scheduleSave = React.useCallback(
    (nextMeta: PostMeta) => {
      if (saveTimer.current) window.clearTimeout(saveTimer.current)
      setSaveState("idle")
      saveTimer.current = window.setTimeout(() => void persist(nextMeta), 900)
    },
    [persist]
  )

  const metaRef = React.useRef(meta)

  React.useEffect(() => {
    metaRef.current = meta
  }, [meta])

  React.useEffect(() => {
    // Only edits made after the post is in place count: extensions stamp ids
    // onto the blank document at startup, which would otherwise autosave the
    // empty editor over the post being loaded.
    if (!editor || loading) return
    const handler = () => scheduleSave(metaRef.current)
    editor.on("update", handler)
    return () => {
      editor.off("update", handler)
    }
  }, [editor, loading, scheduleSave])

  const updateMeta = React.useCallback(
    (patch: Partial<PostMeta>) => {
      setMeta((current) => {
        const next = { ...current, ...patch }
        scheduleSave(next)
        return next
      })
    },
    [scheduleSave]
  )

  // Slug tracks the title automatically until the user edits it directly
  // (via post settings) — same "auto until touched" behavior as WordPress's
  // permalink field. Loaded posts already have a real slug, so this starts
  // off for them.
  const autoSlugRef = React.useRef(postId === null)

  const handleSettingsChange = React.useCallback(
    (patch: Partial<PostMeta>) => {
      if (patch.slug !== undefined) autoSlugRef.current = false
      updateMeta(patch)
    },
    [updateMeta]
  )

  React.useEffect(() => {
    editor?.setEditable(!preview)
  }, [editor, preview])

  React.useEffect(
    () =>
      onEditorDialog((name) => {
        if (name === "find-replace") setFindOpen(true)
      }),
    []
  )

  React.useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const mod = event.metaKey || event.ctrlKey
      if (!mod) return

      if (event.key.toLowerCase() === "s") {
        event.preventDefault()
        void persist(metaRef.current, { notify: true })
      } else if (event.shiftKey && event.key.toLowerCase() === "f") {
        event.preventDefault()
        setFindOpen(true)
      } else if (event.key === "/") {
        event.preventDefault()
        openEditorDialog("shortcuts")
      } else if (event.shiftKey && event.key.toLowerCase() === "o") {
        event.preventDefault()
        setFocusMode((value) => !value)
      } else if (event.shiftKey && event.key === "Enter") {
        event.preventDefault()
        setFullscreen((value) => !value)
      }
    }
    window.addEventListener("keydown", onKeyDown)
    return () => window.removeEventListener("keydown", onKeyDown)
  }, [persist, t])

  React.useEffect(() => {
    if (fullscreen) {
      document.documentElement.requestFullscreen?.().catch(() => undefined)
    } else if (document.fullscreenElement) {
      document.exitFullscreen?.().catch(() => undefined)
    }
  }, [fullscreen])

  if (!editor || loading) {
    return (
      <div className="flex min-h-svh items-center justify-center">
        <Loader2 className="size-6 animate-spin text-muted-foreground" />
      </div>
    )
  }

  return (
    <div className="flex h-svh flex-col overflow-hidden bg-background">
      <header className="flex items-center gap-3 border-b border-border px-4 py-2">
        <div className="flex items-center gap-2">
          <span className="flex size-7 items-center justify-center rounded-lg bg-primary text-sm font-bold text-primary-foreground">
            M
          </span>
          <span className="text-sm font-semibold">Mavi CMS</span>
        </div>

        <HeaderButton
          label={t`Dashboard`}
          onClick={() => void navigate({ to: "/dashboard" })}
        >
          <LayoutDashboard />
        </HeaderButton>

        <Separator orientation="vertical" className="h-5" />

        <span className="min-w-0 flex-1 truncate text-sm font-medium text-muted-foreground">
          {meta.title || t`Untitled post`}
        </span>

        <Badge variant={meta.status === "published" ? "default" : "secondary"}>
          {STATUS_LABELS[meta.status]}
        </Badge>

        {languages.length > 1 && locale && (
          <DropdownMenu>
            <DropdownMenuTrigger
              render={
                <Button variant="outline" size="sm">
                  <Languages /> {languageLabel(locale)}
                </Button>
              }
            />
            <DropdownMenuContent align="end" className="w-56">
              <DropdownMenuGroup>
                <DropdownMenuLabel>{t`Translations`}</DropdownMenuLabel>
                {languages
                  .filter((language) => language.code !== locale)
                  .map((language) => {
                    const sibling = translations.find(
                      (item) => item.locale === language.code
                    )
                    return (
                      <DropdownMenuItem
                        key={language.code}
                        // A translation can only be started once the post itself
                        // exists — otherwise there is nothing to link it to.
                        disabled={!sibling && !currentPostId}
                        onClick={() =>
                          sibling
                            ? navigate({
                                to: "/editor/$postId",
                                params: { postId: sibling.id },
                              })
                            : navigate({
                                to: "/editor/new",
                                search: {
                                  locale: language.code,
                                  translationOf: currentPostId ?? undefined,
                                },
                              })
                        }
                      >
                        <span className="flex-1">{language.native_name}</span>
                        <span className="text-xs text-muted-foreground">
                          {sibling ? t`Edit` : t`Create`}
                        </span>
                      </DropdownMenuItem>
                    )
                  })}
              </DropdownMenuGroup>
            </DropdownMenuContent>
          </DropdownMenu>
        )}

        <div className="flex items-center gap-0.5">
          <HeaderButton
            label={t`Table of contents`}
            active={showToc}
            onClick={() => setShowToc((value) => !value)}
          >
            <ListTree />
          </HeaderButton>
          <HeaderButton
            label={t`Focus mode`}
            keys="Ctrl+Shift+O"
            active={focusMode}
            onClick={() => setFocusMode((value) => !value)}
          >
            <FocusIcon />
          </HeaderButton>
          <HeaderButton
            label={preview ? t`Back to editing` : t`Preview`}
            active={preview}
            onClick={() => setPreview((value) => !value)}
          >
            {preview ? <EyeOff /> : <Eye />}
          </HeaderButton>
          <HeaderButton
            label={t`Fullscreen`}
            keys="Ctrl+Shift+Enter"
            active={fullscreen}
            onClick={() => setFullscreen((value) => !value)}
          >
            {fullscreen ? <Minimize /> : <Maximize />}
          </HeaderButton>
          <HeaderButton
            label={t`Import / export`}
            onClick={() => openEditorDialog("export")}
          >
            <FileDown />
          </HeaderButton>
          <HeaderButton
            label={t`Shortcuts`}
            keys="Ctrl+/"
            onClick={() => openEditorDialog("shortcuts")}
          >
            <Keyboard />
          </HeaderButton>
          <HeaderButton
            label={t`Settings panel`}
            active={showSettings}
            onClick={() => setShowSettings((value) => !value)}
          >
            {showSettings ? <PanelRightClose /> : <PanelRightOpen />}
          </HeaderButton>
          <LocaleToggle />
          <ModeToggle />
          <HeaderButton
            label={t`Sign out`}
            onClick={() => {
              void logout().finally(() => navigate({ to: "/login" }))
            }}
          >
            <LogOut />
          </HeaderButton>
        </div>

        <Separator orientation="vertical" className="h-5" />

        <Button
          variant="outline"
          size="sm"
          onClick={() => void persist(meta, { notify: true })}
        >
          <Save /> {t`Save`}
        </Button>
        <Button
          size="sm"
          onClick={() => {
            updateMeta({ status: "published" })
            toast.success(t`Post published`, {
              description: `/blog/${meta.slug}`,
            })
          }}
        >
          <Send /> {t`Publish`}
        </Button>
      </header>

      {!preview && (
        <div className="border-b border-border bg-background/80 backdrop-blur">
          <Toolbar editor={editor} />
        </div>
      )}

      <div className="flex min-h-0 flex-1">
        {showToc && (
          <aside className="hidden w-64 shrink-0 border-r border-border lg:block">
            <ScrollArea className="h-full">
              <div className="p-3">
                <p className="px-2 pb-2 text-xs font-medium tracking-wide text-muted-foreground uppercase">
                  {t`Table of contents`}
                </p>
                <TocPanel editor={editor} items={toc} />
              </div>
            </ScrollArea>
          </aside>
        )}

        <main
          id={SCROLL_CONTAINER_ID}
          className={cn(
            "relative min-w-0 flex-1 overflow-y-auto",
            focusMode && "mavi-focus-mode"
          )}
        >
          {findOpen && (
            <FindReplacePanel
              editor={editor}
              onClose={() => setFindOpen(false)}
            />
          )}

          <article className="mx-auto w-full max-w-3xl px-6 py-10 sm:px-10">
            <textarea
              value={meta.title}
              onChange={(event) => {
                const title = event.target.value
                updateMeta(
                  autoSlugRef.current
                    ? { title, slug: slugify(title) }
                    : { title }
                )
              }}
              placeholder={t`Untitled post`}
              rows={1}
              aria-label={t`Post title`}
              readOnly={preview}
              ref={autoSizeTitle}
              onInput={(event) => autoSizeTitle(event.currentTarget)}
              className="mb-6 w-full resize-none bg-transparent text-4xl leading-tight font-bold tracking-tight outline-none placeholder:text-muted-foreground/40"
            />
            <EditorContent editor={editor} />
          </article>
        </main>

        {showSettings && !preview && (
          <aside className="hidden w-80 shrink-0 border-l border-border xl:block">
            <ScrollArea className="h-full">
              <div className="p-4">
                <p className="pb-4 text-xs font-medium tracking-wide text-muted-foreground uppercase">
                  {t`Post settings`}
                </p>
                <PostSettings
                  meta={meta}
                  onChange={handleSettingsChange}
                  locale={locale}
                  plainText={editor.getText()}
                />
              </div>
            </ScrollArea>
          </aside>
        )}
      </div>

      <StatusBar
        editor={editor}
        saveState={saveState}
        savedAt={savedAt}
        characterLimit={CHARACTER_LIMIT}
      />

      {!preview && (
        <>
          <BlockHandle editor={editor} />
          <TextBubbleMenu editor={editor} />
          <LinkBubbleMenu editor={editor} />
          <ImageBubbleMenu editor={editor} />
          <TableBubbleMenu editor={editor} />
        </>
      )}

      <EditorDialogs editor={editor} />
      <Toaster position="bottom-center" />
    </div>
  )
}

function autoSizeTitle(element: HTMLTextAreaElement | null) {
  if (!element) return
  element.style.height = "auto"
  element.style.height = `${element.scrollHeight}px`
}

function HeaderButton({
  label,
  keys,
  active,
  children,
  onClick,
}: {
  label: string
  keys?: string
  active?: boolean
  children: React.ReactNode
  onClick: () => void
}) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={label}
            aria-pressed={active}
            data-active={active || undefined}
            onClick={onClick}
            className="text-muted-foreground data-[active]:bg-primary/10 data-[active]:text-primary"
          />
        }
      >
        {children}
      </TooltipTrigger>
      <TooltipContent>
        {label}
        {keys ? <Kbd>{shortcut(keys)}</Kbd> : null}
      </TooltipContent>
    </Tooltip>
  )
}

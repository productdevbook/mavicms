import * as React from "react"
import { Link, useNavigate } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { Loader2, Pencil, Trash2 } from "lucide-react"
import { toast } from "sonner"

import {
  ApiError,
  deletePost,
  getPosts,
  type PostKind,
  type PostPage,
  type PostStatus,
  type PostSummary,
} from "@/lib/api"
import { useContentTypes } from "@/lib/use-content-types"
import { useLanguages } from "@/lib/use-languages"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { useStatusLabels } from "@/components/editor/types"

/**
 * The list behind both Posts and Pages.
 *
 * They differ in one word and in nothing else — the same statuses, the same
 * languages, the same editor — so they are one screen told which kind it is
 * rather than two that would drift apart a fix at a time.
 */
export function ContentList({ kind }: { kind: PostKind }) {
  const { t } = useLingui()
  const { find } = useContentTypes()
  const of_kind = find(kind)
  // Until the list of kinds arrives, the slug is a better label than nothing.
  const one = of_kind?.name ?? kind
  const many = of_kind?.plural ?? kind
  const navigate = useNavigate()
  const STATUS_LABELS = useStatusLabels()
  const [page, setPage] = React.useState<PostPage | null>(null)
  const [offset, setOffset] = React.useState(0)
  const posts = page?.items ?? null
  const [pendingDelete, setPendingDelete] = React.useState<PostSummary | null>(
    null
  )
  const { languages, defaultCode, label } = useLanguages()
  const [selectedLocale, setSelectedLocale] = React.useState("")
  // Counts and the list are always scoped to one language: totalling every
  // language would report "36 posts" for 12 posts in three languages. Derived
  // rather than synced through an effect so the default landing doesn't cause
  // a cascading render.
  const locale = selectedLocale || defaultCode

  const load = React.useCallback(() => {
    if (!locale) return
    getPosts(locale, { offset, kind })
      .then(setPage)
      .catch(() => setPage(null))
  }, [locale, offset, kind])

  React.useEffect(() => load(), [load])

  // Straight from the server: counting the rows on screen would report the
  // size of the page rather than the archive.
  const counts = React.useMemo(() => {
    const base: Record<PostStatus, number> = {
      draft: 0,
      review: 0,
      scheduled: 0,
      published: 0,
    }
    return page?.counts ?? base
  }, [page])

  const confirmDelete = async () => {
    if (!pendingDelete) return
    try {
      await deletePost(pendingDelete.id)
      // Reloaded rather than filtered out: the counts and the page below it
      // both move when a post goes.
      load()
      toast.success(t`${one} deleted`)
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not delete it`
      )
    } finally {
      setPendingDelete(null)
    }
  }

  return (
    <>
      {languages.length > 1 && (
        <div className="mb-4 flex items-center gap-2">
          <span className="text-sm text-muted-foreground">{t`Language`}</span>
          <Select
            value={locale}
            onValueChange={(value) => {
              setSelectedLocale(value ?? "")
              setOffset(0)
            }}
          >
            <SelectTrigger className="w-52">
              <SelectValue>{(code: string) => label(code)}</SelectValue>
            </SelectTrigger>
            <SelectContent>
              {languages.map((language) => (
                <SelectItem key={language.code} value={language.code}>
                  {language.native_name} ({language.code})
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      )}

      <div className="mb-6 grid grid-cols-2 gap-3 sm:grid-cols-4">
        {(Object.keys(counts) as PostStatus[]).map((status) => (
          <Card key={status}>
            <CardContent className="pt-6">
              <p className="text-2xl font-semibold">{counts[status]}</p>
              <p className="text-sm text-muted-foreground">
                {STATUS_LABELS[status]}
              </p>
            </CardContent>
          </Card>
        ))}
      </div>

      {posts === null ? (
        <div className="flex justify-center py-16">
          <Loader2 className="size-6 animate-spin text-muted-foreground" />
        </div>
      ) : posts.length === 0 ? (
        <div className="flex flex-col items-center gap-3 rounded-xl border border-dashed border-border py-16 text-center">
          <p className="text-sm text-muted-foreground">{t`No ${many} yet`}</p>
          <Button
            onClick={() =>
              navigate({
                to: "/editor/new",
                search: { locale, translationOf: undefined, kind },
              })
            }
          >
            {t`New ${one}`}
          </Button>
        </div>
      ) : (
        <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
          {posts.map((post) => (
            <div key={post.id} className="flex items-center gap-3 px-4 py-3">
              <div className="min-w-0 flex-1">
                <Link
                  to="/editor/$postId"
                  params={{ postId: post.id }}
                  className="truncate text-sm font-medium hover:underline"
                >
                  {post.title || t`Untitled`}
                </Link>
                <p className="truncate text-xs text-muted-foreground">
                  {post.category || t`Uncategorized`} ·{" "}
                  {new Date(
                    post.publish_at ?? post.updated_at
                  ).toLocaleString()}
                  {post.locales.length > 1 && (
                    <>
                      {" · "}
                      {post.locales
                        .filter((code) => code !== post.locale)
                        .map(label)
                        .join(", ")}
                    </>
                  )}
                </p>
              </div>
              <Badge
                variant={post.status === "published" ? "default" : "secondary"}
              >
                {STATUS_LABELS[post.status]}
              </Badge>
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={t`Edit`}
                onClick={() =>
                  navigate({
                    to: "/editor/$postId",
                    params: { postId: post.id },
                  })
                }
              >
                <Pencil />
              </Button>
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={t`Delete`}
                onClick={() => setPendingDelete(post)}
              >
                <Trash2 />
              </Button>
            </div>
          ))}
        </div>
      )}

      {page && page.total > page.limit && (
        <div className="mt-4 flex items-center justify-between text-sm">
          <span className="text-muted-foreground">
            {t`${page.offset + 1}–${Math.min(page.offset + page.limit, page.total)} of ${page.total}`}
          </span>
          <div className="flex gap-2">
            <Button
              variant="outline"
              size="sm"
              disabled={page.offset === 0}
              onClick={() => setOffset(Math.max(0, page.offset - page.limit))}
            >
              {t`Previous`}
            </Button>
            <Button
              variant="outline"
              size="sm"
              disabled={page.offset + page.limit >= page.total}
              onClick={() => setOffset(page.offset + page.limit)}
            >
              {t`Next`}
            </Button>
          </div>
        </div>
      )}

      <AlertDialog
        open={pendingDelete !== null}
        onOpenChange={(open) => !open && setPendingDelete(null)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t`Delete this post?`}</AlertDialogTitle>
            <AlertDialogDescription>
              {t`"${pendingDelete?.title}" will be permanently deleted. This cannot be undone.`}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t`Cancel`}</AlertDialogCancel>
            <AlertDialogAction onClick={() => void confirmDelete()}>
              {t`Delete`}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
}

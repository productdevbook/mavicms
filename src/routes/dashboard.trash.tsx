/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import {
  FileText,
  Image as ImageIcon,
  Inbox,
  Mails,
  RotateCcw,
  Tags,
  Trash2,
} from "lucide-react"
import { toast } from "sonner"

import {
  ApiError,
  getTrash,
  purgeFromTrash,
  restoreFromTrash,
  type TrashEntry,
} from "@/lib/api"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty"
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemMedia,
  ItemTitle,
} from "@/components/ui/item"
import { Spinner } from "@/components/ui/spinner"

export const Route = createFileRoute("/dashboard/trash")({
  component: TrashRoute,
})

function TrashRoute() {
  const { t } = useLingui()

  const [entries, setEntries] = React.useState<TrashEntry[] | null>(null)
  const [busy, setBusy] = React.useState<string | null>(null)

  const load = React.useCallback(() => {
    getTrash()
      .then(setEntries)
      .catch(() => setEntries([]))
  }, [])

  React.useEffect(load, [load])

  const named: Record<string, string> = {
    post: t`post`,
    page: t`page`,
    media: t`image`,
    form: t`form`,
    form_submission: t`what somebody sent`,
    category: t`category`,
    tag: t`tag`,
    mail_list: t`mailing list`,
    subscriber: t`subscriber`,
    mail_template: t`letterhead`,
    campaign: t`campaign`,
  }

  const icon = (kind: string) => {
    const glyph =
      kind === "media"
        ? ImageIcon
        : kind === "form" || kind === "form_submission"
          ? Inbox
          : kind === "category" || kind === "tag"
            ? Tags
            : kind.startsWith("mail_") ||
                kind === "subscriber" ||
                kind === "campaign"
              ? Mails
              : FileText
    return React.createElement(glyph, {
      className: "size-4 text-muted-foreground",
    })
  }

  const restore = (entry: TrashEntry) => {
    setBusy(entry.id)
    restoreFromTrash(entry.id)
      .then((answer) => {
        toast.success(t`${answer.what} is back`)
        load()
      })
      .catch((error) =>
        toast.error(
          error instanceof ApiError ? error.message : t`Could not put it back`
        )
      )
      .finally(() => setBusy(null))
  }

  const purge = (entry: TrashEntry) => {
    setBusy(entry.id)
    purgeFromTrash(entry.id)
      .then(load)
      .catch((error) =>
        toast.error(
          error instanceof ApiError ? error.message : t`Could not throw it away`
        )
      )
      .finally(() => setBusy(null))
  }

  return (
    <>
      <div className="mb-6">
        <h1 className="text-lg font-semibold">{t`Bin`}</h1>
        <p className="text-sm text-muted-foreground">
          {t`Nothing deleted here goes straight away. It waits thirty days, whether a person deleted it or an assistant did, and until then it can be put back exactly as it was.`}
        </p>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>{t`Waiting to be thrown away`}</CardTitle>
          <CardDescription>
            {t`An image keeps its file while it is here, so a restored post still has its pictures. The file goes when the entry does.`}
          </CardDescription>
        </CardHeader>
        <CardContent>
          {!entries ? (
            <div className="flex justify-center py-8">
              <Spinner className="size-5 text-muted-foreground" />
            </div>
          ) : entries.length === 0 ? (
            <Empty className="border">
              <EmptyHeader>
                <EmptyMedia variant="icon">
                  <Trash2 />
                </EmptyMedia>
                <EmptyTitle>{t`Nothing has been deleted`}</EmptyTitle>
                <EmptyDescription>
                  {t`When something is, it appears here first.`}
                </EmptyDescription>
              </EmptyHeader>
            </Empty>
          ) : (
            <ItemGroup className="rounded-xl border">
              {entries.map((entry) => (
                <Item key={entry.id} size="sm">
                  <ItemMedia>{icon(entry.kind)}</ItemMedia>
                  <ItemContent>
                    <ItemTitle>{entry.title}</ItemTitle>
                    <ItemDescription>
                      {named[entry.kind] ?? entry.kind} ·{" "}
                      {t`deleted by ${entry.deleted_by}`} ·{" "}
                      {new Date(entry.deleted_at).toLocaleString()}
                    </ItemDescription>
                    <ItemDescription>
                      {t`Goes for good on ${new Date(entry.purges_at).toLocaleDateString()}`}
                    </ItemDescription>
                  </ItemContent>
                  <ItemActions>
                    <Button
                      variant="outline"
                      size="sm"
                      disabled={busy === entry.id}
                      onClick={() => restore(entry)}
                    >
                      <RotateCcw /> {t`Put it back`}
                    </Button>
                    <Button
                      variant="ghost"
                      size="icon-sm"
                      aria-label={t`Throw it away now`}
                      disabled={busy === entry.id}
                      onClick={() => purge(entry)}
                    >
                      <Trash2 />
                    </Button>
                  </ItemActions>
                </Item>
              ))}
            </ItemGroup>
          )}
        </CardContent>
      </Card>
    </>
  )
}

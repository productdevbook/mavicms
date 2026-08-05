/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { Link2, Loader2, Trash2, Unlink } from "lucide-react"
import { toast } from "sonner"

import {
  ApiError,
  createTag,
  deleteTag,
  getTags,
  setTagTranslationGroup,
  type Tag,
} from "@/lib/api"
import { useLanguages } from "@/lib/use-languages"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
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

export const Route = createFileRoute("/dashboard/tags")({
  component: TagsRoute,
})

function TagsRoute() {
  const { t } = useLingui()
  const { languages, defaultCode, label } = useLanguages()
  const [tags, setTags] = React.useState<Tag[] | null>(null)
  const [pendingDelete, setPendingDelete] = React.useState<Tag | null>(null)
  const [newName, setNewName] = React.useState("")
  const [newLocale, setNewLocale] = React.useState("")
  const [linking, setLinking] = React.useState<{
    group: string
    locale: string
  } | null>(null)

  const load = React.useCallback(() => {
    getTags()
      .then(setTags)
      .catch(() => setTags([]))
  }, [])

  React.useEffect(() => load(), [load])

  // One row per translation group, one column per language — the only view
  // that makes an unlinked tag visible as a gap.
  const groups = React.useMemo(() => {
    const byGroup = new Map<string, Map<string, Tag>>()
    for (const tag of tags ?? []) {
      let row = byGroup.get(tag.translation_group_id)
      if (!row) {
        row = new Map()
        byGroup.set(tag.translation_group_id, row)
      }
      row.set(tag.locale, tag)
    }
    return [...byGroup.entries()]
      .map(([id, row]) => ({ id, row }))
      .sort((a, b) => {
        const nameOf = (g: typeof a) =>
          g.row.get(defaultCode)?.name ?? [...g.row.values()][0]?.name ?? ""
        return nameOf(a).localeCompare(nameOf(b))
      })
  }, [tags, defaultCode])

  const add = async () => {
    const name = newName.trim()
    if (!name) return
    try {
      await createTag(name, newLocale || defaultCode)
      setNewName("")
      load()
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not add tag`
      )
    }
  }

  const link = async (id: string, targetId: string) => {
    try {
      await setTagTranslationGroup(id, { join: targetId })
      setLinking(null)
      load()
      toast.success(t`Tags linked`)
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not link tags`
      )
    }
  }

  const unlink = async (id: string) => {
    try {
      await setTagTranslationGroup(id, { detach: true })
      load()
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not unlink tag`
      )
    }
  }

  const confirmDelete = async () => {
    if (!pendingDelete) return
    try {
      await deleteTag(pendingDelete.id)
      setTags(
        (current) =>
          current?.filter((tag) => tag.id !== pendingDelete.id) ?? null
      )
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not delete tag`
      )
    } finally {
      setPendingDelete(null)
    }
  }

  const columns = languages.length > 0 ? languages : [{ code: defaultCode }]

  return (
    <>
      <div className="mb-6">
        <h1 className="text-lg font-semibold">{t`Tags`}</h1>
        <p className="text-sm text-muted-foreground">
          {t`Each language has its own tags. Link them so a post's tag can point at its counterpart in another language.`}
        </p>
      </div>

      <div className="mb-6 flex gap-2">
        <Input
          value={newName}
          onChange={(event) => setNewName(event.target.value)}
          onKeyDown={(event) => event.key === "Enter" && void add()}
          placeholder={t`New tag`}
          className="max-w-xs"
        />
        {languages.length > 1 && (
          <Select
            value={newLocale || defaultCode}
            onValueChange={(value) => setNewLocale(value ?? "")}
          >
            <SelectTrigger className="w-44">
              <SelectValue>{(code: string) => label(code)}</SelectValue>
            </SelectTrigger>
            <SelectContent>
              {languages.map((language) => (
                <SelectItem key={language.code} value={language.code}>
                  {language.native_name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        )}
        <Button onClick={() => void add()} disabled={!newName.trim()}>
          {t`Add`}
        </Button>
      </div>

      {tags === null ? (
        <div className="flex justify-center py-16">
          <Loader2 className="size-6 animate-spin text-muted-foreground" />
        </div>
      ) : groups.length === 0 ? (
        <p className="rounded-xl border border-dashed border-border py-16 text-center text-sm text-muted-foreground">
          {t`No tags yet`}
        </p>
      ) : (
        <div className="overflow-x-auto rounded-xl border border-border">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-border bg-muted/50">
                {columns.map((column) => (
                  <th
                    key={column.code}
                    className="px-4 py-2 text-left font-medium text-muted-foreground"
                  >
                    {languages.find((l) => l.code === column.code)
                      ?.native_name ?? column.code}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody className="divide-y divide-border">
              {groups.map((group) => (
                <tr key={group.id}>
                  {columns.map((column) => {
                    const tag = group.row.get(column.code)
                    const isLinking =
                      linking?.group === group.id &&
                      linking.locale === column.code
                    const orphans = (tags ?? []).filter(
                      (candidate) =>
                        candidate.locale === column.code &&
                        candidate.translation_group_id !== group.id
                    )
                    return (
                      <td key={column.code} className="px-4 py-2 align-top">
                        {tag ? (
                          <span className="group flex items-center gap-1">
                            <span className="flex-1">{tag.name}</span>
                            {group.row.size > 1 && (
                              <Button
                                variant="ghost"
                                size="icon-sm"
                                aria-label={t`Unlink`}
                                className="opacity-0 group-hover:opacity-100"
                                onClick={() => void unlink(tag.id)}
                              >
                                <Unlink />
                              </Button>
                            )}
                            <Button
                              variant="ghost"
                              size="icon-sm"
                              aria-label={t`Delete`}
                              className="opacity-0 group-hover:opacity-100"
                              onClick={() => setPendingDelete(tag)}
                            >
                              <Trash2 />
                            </Button>
                          </span>
                        ) : isLinking ? (
                          <Select
                            value=""
                            onValueChange={(value) => {
                              const target = [...group.row.values()][0]
                              if (value && target) void link(value, target.id)
                            }}
                          >
                            <SelectTrigger className="w-full">
                              <SelectValue placeholder={t`Pick a tag`} />
                            </SelectTrigger>
                            <SelectContent>
                              {orphans.map((candidate) => (
                                <SelectItem
                                  key={candidate.id}
                                  value={candidate.id}
                                >
                                  {candidate.name}
                                </SelectItem>
                              ))}
                            </SelectContent>
                          </Select>
                        ) : (
                          <Button
                            variant="ghost"
                            size="sm"
                            className="text-muted-foreground"
                            disabled={orphans.length === 0}
                            onClick={() =>
                              setLinking({
                                group: group.id,
                                locale: column.code,
                              })
                            }
                          >
                            <Link2 /> {t`Link translation`}
                          </Button>
                        )}
                      </td>
                    )
                  })}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      <AlertDialog
        open={pendingDelete !== null}
        onOpenChange={(open) => !open && setPendingDelete(null)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t`Delete this tag?`}</AlertDialogTitle>
            <AlertDialogDescription>
              {t`"${pendingDelete?.name}" will be removed from every post that uses it.`}
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

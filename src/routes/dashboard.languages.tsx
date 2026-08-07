/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { Loader2, Plus, Star, Trash2 } from "lucide-react"
import { toast } from "sonner"

import {
  ApiError,
  createLanguage,
  deleteLanguage,
  getLanguages,
  updateLanguage,
  type Language,
} from "@/lib/api"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"
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

export const Route = createFileRoute("/dashboard/languages")({
  component: LanguagesRoute,
})

function LanguagesRoute() {
  const { t } = useLingui()
  const [languages, setLanguages] = React.useState<Language[] | null>(null)
  const [code, setCode] = React.useState("")
  const [name, setName] = React.useState("")
  const [pendingDelete, setPendingDelete] = React.useState<Language | null>(null)

  const load = React.useCallback(() => {
    getLanguages()
      .then(setLanguages)
      .catch(() => setLanguages([]))
  }, [])

  React.useEffect(() => load(), [load])

  const fail = (error: unknown, fallback: string) =>
    toast.error(error instanceof ApiError ? error.message : fallback)

  const add = async () => {
    if (!code.trim()) return
    try {
      await createLanguage({ code: code.trim(), name: name.trim() })
      setCode("")
      setName("")
      load()
    } catch (error) {
      fail(error, t`Could not add language`)
    }
  }

  const patch = async (
    language: Language,
    changes: Parameters<typeof updateLanguage>[1]
  ) => {
    try {
      await updateLanguage(language.code, changes)
      load()
    } catch (error) {
      fail(error, t`Could not update language`)
    }
  }

  const confirmDelete = async () => {
    if (!pendingDelete) return
    try {
      await deleteLanguage(pendingDelete.code)
      load()
    } catch (error) {
      fail(error, t`Could not remove language`)
    } finally {
      setPendingDelete(null)
    }
  }

  return (
    <>
      <div className="mb-6">
        <h1 className="text-lg font-semibold">{t`Languages`}</h1>
        <p className="text-sm text-muted-foreground">
          {t`The languages your content can be written in. This is separate from the language of this admin panel.`}
        </p>
      </div>

      <form
        onSubmit={(event) => {
          event.preventDefault()
          void add()
        }}
        className="mb-6 flex flex-wrap gap-2"
      >
        <Input
          value={code}
          onChange={(event) => setCode(event.target.value)}
          placeholder={t`Code (en, de, pt-BR)`}
          className="w-44"
        />
        <Input
          value={name}
          onChange={(event) => setName(event.target.value)}
          placeholder={t`Name (optional)`}
          className="w-52"
        />
        <Button type="submit">
          <Plus /> {t`Add language`}
        </Button>
      </form>

      {languages === null ? (
        <div className="flex justify-center py-16">
          <Loader2 className="size-6 animate-spin text-muted-foreground" />
        </div>
      ) : (
        <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
          {languages.map((language) => (
            <div
              key={language.code}
              className="flex flex-wrap items-center gap-x-3 gap-y-2 px-4 py-3"
            >
              <div className="min-w-0 basis-full sm:flex-1 sm:basis-0">
                <div className="flex flex-wrap items-center gap-2">
                  <p className="truncate text-sm font-medium">
                    {language.native_name || language.name}
                  </p>
                  <Badge variant="secondary">{language.code}</Badge>
                  {language.is_default && (
                    <Badge>
                      <Star className="size-3" /> {t`Default`}
                    </Badge>
                  )}
                </div>
                <p className="truncate text-xs text-muted-foreground">
                  {language.name} · {language.direction}
                </p>
              </div>

              <div className="flex shrink-0 items-center gap-2">
                <span className="text-xs text-muted-foreground">{t`Active`}</span>
                <Switch
                  checked={language.is_active}
                  // The default language must stay usable, so it can't be
                  // switched off without promoting another one first.
                  disabled={language.is_default}
                  onCheckedChange={(value) =>
                    void patch(language, { is_active: value })
                  }
                />
              </div>

              <Button
                variant="ghost"
                size="sm"
                className="ml-auto shrink-0"
                disabled={language.is_default}
                onClick={() => void patch(language, { is_default: true })}
              >
                {t`Make default`}
              </Button>
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={t`Delete`}
                disabled={language.is_default}
                onClick={() => setPendingDelete(language)}
              >
                <Trash2 />
              </Button>
            </div>
          ))}
        </div>
      )}

      <AlertDialog
        open={pendingDelete !== null}
        onOpenChange={(open) => !open && setPendingDelete(null)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t`Remove this language?`}</AlertDialogTitle>
            <AlertDialogDescription>
              {t`Content already written in this language blocks removal — you'll be told how much there is.`}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t`Cancel`}</AlertDialogCancel>
            <AlertDialogAction onClick={() => void confirmDelete()}>
              {t`Remove`}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
}

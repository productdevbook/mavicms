/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { Loader2, Plus, Shapes, Trash2 } from "lucide-react"
import { toast } from "sonner"

import {
  ApiError,
  createContentType,
  deleteContentType,
  updateContentType,
  type ContentType,
  type FormField,
} from "@/lib/api"
import { useContentTypes } from "@/lib/use-content-types"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { FormFieldsEditor } from "@/components/form-fields-editor"

export const Route = createFileRoute("/dashboard/content-types")({
  component: ContentTypesRoute,
})

/**
 * What this site publishes.
 *
 * A blog has posts and pages. A training company also has courses, and a
 * course has a price and a level — facts a front end can lay out, rather than
 * numbers typed into a paragraph where nothing can find them. This is where a
 * site says what its own kinds of thing are made of.
 */
function ContentTypesRoute() {
  const { t } = useLingui()
  const { types, loading, reload } = useContentTypes()
  const [editing, setEditing] = React.useState<ContentType | null>(null)
  const [adding, setAdding] = React.useState(false)

  const remove = async (kind: ContentType) => {
    try {
      await deleteContentType(kind.id)
      reload()
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not remove it`
      )
    }
  }

  if (editing || adding) {
    return (
      <Editor
        kind={editing}
        onDone={() => {
          setEditing(null)
          setAdding(false)
          reload()
        }}
      />
    )
  }

  return (
    <>
      <div className="mb-6 flex items-start justify-between gap-4">
        <div>
          <h1 className="text-lg font-semibold">{t`What this site publishes`}</h1>
          <p className="text-sm text-muted-foreground">
            {t`Every site has posts and pages. Add a kind of your own when what you publish has facts of its own — a course with a price and a level, a property with rooms — so a page can lay them out instead of hunting for them in a paragraph.`}
          </p>
        </div>
        <Button onClick={() => setAdding(true)}>
          <Plus /> {t`Add a kind`}
        </Button>
      </div>

      {loading ? (
        <div className="flex justify-center py-16">
          <Loader2 className="size-6 animate-spin text-muted-foreground" />
        </div>
      ) : (
        <div className="flex max-w-3xl flex-col divide-y divide-border rounded-xl border border-border">
          {types.map((kind) => (
            <div key={kind.id} className="flex items-center gap-3 px-4 py-3">
              <Shapes className="size-4 shrink-0 text-muted-foreground" />
              <div className="min-w-0 flex-1">
                <p className="truncate text-sm font-medium">{kind.plural}</p>
                <p className="truncate text-xs text-muted-foreground">
                  {kind.slug} ·{" "}
                  {kind.fields.length === 0
                    ? t`no fields of its own`
                    : kind.fields.map((field) => field.label).join(", ")}
                </p>
              </div>
              <span className="text-xs text-muted-foreground">
                {t`${kind.count} written`}
              </span>
              <Button variant="outline" size="sm" onClick={() => setEditing(kind)}>
                {t`Fields`}
              </Button>
              {!kind.built_in && (
                <Button
                  variant="ghost"
                  size="icon-sm"
                  aria-label={t`Remove`}
                  onClick={() => void remove(kind)}
                >
                  <Trash2 />
                </Button>
              )}
            </div>
          ))}
        </div>
      )}
    </>
  )
}

function Editor({
  kind,
  onDone,
}: {
  kind: ContentType | null
  onDone: () => void
}) {
  const { t } = useLingui()
  const [name, setName] = React.useState(kind?.name ?? "")
  const [plural, setPlural] = React.useState(kind?.plural ?? "")
  const [fields, setFields] = React.useState<FormField[]>(kind?.fields ?? [])
  const [saving, setSaving] = React.useState(false)

  const save = async () => {
    setSaving(true)
    try {
      const payload = { name, plural, fields }
      if (kind) {
        await updateContentType(kind.id, payload)
      } else {
        await createContentType(payload)
      }
      onDone()
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not save it`
      )
      setSaving(false)
    }
  }

  return (
    <div className="flex max-w-3xl flex-col gap-6">
      <div>
        <h1 className="text-lg font-semibold">
          {kind ? kind.plural : t`A new kind`}
        </h1>
        <p className="text-sm text-muted-foreground">
          {kind
            ? t`What one of these is made of, beyond its title and its text.`
            : t`Give it a name and say what one is made of. The address it answers on is made from the name and never changes afterwards, because a front end will be asking for it.`}
        </p>
      </div>

      <div className="grid gap-4 sm:grid-cols-2">
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="kind-name">{t`One of them is called`}</Label>
          <Input
            id="kind-name"
            value={name}
            onChange={(event) => setName(event.target.value)}
            placeholder={t`Course`}
          />
        </div>
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="kind-plural">{t`Several are called`}</Label>
          <Input
            id="kind-plural"
            value={plural}
            onChange={(event) => setPlural(event.target.value)}
            placeholder={t`Courses`}
          />
        </div>
      </div>

      {kind && (
        <p className="text-sm text-muted-foreground">
          {t`A front end asks for these with ?kind=${kind.slug}`}
        </p>
      )}

      <div className="flex flex-col gap-2">
        <h2 className="text-sm font-medium">{t`Fields`}</h2>
        <p className="text-sm text-muted-foreground">
          {t`Beyond the title, the text, the picture and the SEO, which everything here already has. A field marked as needed has to be filled in before the thing can be published — a draft may be half-written.`}
        </p>
        <FormFieldsEditor fields={fields} onChange={setFields} />
      </div>

      <div className="flex gap-2">
        <Button onClick={() => void save()} disabled={!name.trim() || saving}>
          {saving ? <Loader2 className="animate-spin" /> : null}
          {t`Save`}
        </Button>
        <Button variant="outline" onClick={onDone}>
          {t`Cancel`}
        </Button>
      </div>
    </div>
  )
}

/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { Link, createFileRoute } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { Inbox, Loader2, Plus, Trash2 } from "lucide-react"
import { toast } from "sonner"

import {
  ApiError,
  createForm,
  deleteForm,
  getForms,
  getSlug,
  updateForm,
  type FormField,
  type SiteForm,
} from "@/lib/api"
import { FormFieldsEditor } from "@/components/form-fields-editor"
import { emptyField } from "@/lib/form-fields"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"
import { Textarea } from "@/components/ui/textarea"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
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

export const Route = createFileRoute("/dashboard/forms")({
  component: FormsRoute,
})

interface Draft {
  id: string | null
  name: string
  slug: string
  description: string
  fields: FormField[]
  active: boolean
  notify: string
}

function blank(label: string): Draft {
  return {
    id: null,
    name: "",
    slug: "",
    description: "",
    fields: [{ ...emptyField(), name: "name", label, required: true }],
    active: true,
    notify: "",
  }
}

function FormsRoute() {
  const { t } = useLingui()

  const [forms, setForms] = React.useState<SiteForm[] | null>(null)
  const [draft, setDraft] = React.useState<Draft | null>(null)
  const [saving, setSaving] = React.useState(false)
  const [removing, setRemoving] = React.useState<SiteForm | null>(null)

  // Which name the address on screen belongs to. The server decides what a
  // slug looks like — it knows that "İ" is an "i" — so the answer is asked
  // for rather than guessed, and a slow reply for an old name is dropped.
  const asked = React.useRef(0)

  const load = React.useCallback(() => {
    getForms()
      .then(setForms)
      .catch(() => toast.error(t`Could not load the forms`))
  }, [t])

  React.useEffect(load, [load])

  const save = async () => {
    if (!draft) return
    setSaving(true)
    try {
      const payload = {
        name: draft.name.trim(),
        slug: draft.slug.trim() || undefined,
        description: draft.description.trim(),
        fields: draft.fields,
        active: draft.active,
        notify: draft.notify.trim(),
      }
      if (draft.id) {
        await updateForm(draft.id, payload)
      } else {
        await createForm(payload)
      }
      setDraft(null)
      load()
      toast.success(t`Saved`)
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not save the form`
      )
    } finally {
      setSaving(false)
    }
  }

  const remove = async () => {
    if (!removing) return
    try {
      await deleteForm(removing.id)
      load()
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not remove it`
      )
    } finally {
      setRemoving(null)
    }
  }

  const edit = (form: SiteForm) =>
    setDraft({
      id: form.id,
      name: form.name,
      slug: form.slug,
      description: form.description,
      fields: form.fields,
      active: form.active,
      notify: form.notify,
    })

  const ready =
    draft !== null &&
    draft.name.trim().length > 0 &&
    draft.fields.length > 0 &&
    draft.fields.every((field) => field.name.length > 0)

  return (
    <>
      <div className="mb-6 flex items-start justify-between gap-4">
        <div>
          <h1 className="text-lg font-semibold">{t`Forms`}</h1>
          <p className="text-sm text-muted-foreground">
            {t`Describe what a form accepts here, then have your own pages post to it.`}
          </p>
        </div>
        <Button onClick={() => setDraft(blank(t`Name`))}>
          <Plus /> {t`New form`}
        </Button>
      </div>

      {!forms ? (
        <div className="flex justify-center py-16">
          <Loader2 className="size-6 animate-spin text-muted-foreground" />
        </div>
      ) : forms.length === 0 ? (
        <p className="rounded-xl border border-dashed border-border py-12 text-center text-sm text-muted-foreground">
          {t`No forms yet`}
        </p>
      ) : (
        <div className="flex max-w-3xl flex-col divide-y divide-border rounded-xl border border-border">
          {forms.map((form) => (
            <div key={form.id} className="flex items-center gap-3 px-4 py-3">
              <Inbox className="size-4 shrink-0 text-muted-foreground" />
              <div className="min-w-0 flex-1">
                <p className="truncate text-sm font-medium">
                  {form.name}
                  {!form.active && (
                    <span className="ml-2 text-xs font-normal text-muted-foreground">
                      {t`switched off`}
                    </span>
                  )}
                </p>
                <p className="truncate font-mono text-xs text-muted-foreground">
                  /api/forms/{form.slug}/submit
                </p>
              </div>

              <Link
                to="/dashboard/forms/$formId"
                params={{ formId: form.id }}
                className="flex items-center gap-1.5 rounded-md px-2.5 py-1.5 text-sm font-medium text-muted-foreground hover:bg-muted hover:text-foreground"
              >
                {form.unseen > 0 ? (
                  <span className="surface-mark rounded-full px-1.5 py-0.5 text-xs font-semibold text-white">
                    {form.unseen}
                  </span>
                ) : null}
                {t`${form.submissions} received`}
              </Link>

              <Button variant="outline" size="sm" onClick={() => edit(form)}>
                {t`Edit`}
              </Button>
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={t`Remove`}
                onClick={() => setRemoving(form)}
              >
                <Trash2 />
              </Button>
            </div>
          ))}
        </div>
      )}

      <Dialog
        open={draft !== null}
        onOpenChange={(open) => !open && setDraft(null)}
      >
        <DialogContent className="max-h-[90svh] overflow-y-auto sm:max-w-2xl">
          <DialogHeader>
            <DialogTitle>
              {draft?.id ? t`Edit the form` : t`New form`}
            </DialogTitle>
            <DialogDescription>
              {t`What comes in is kept as it was sent, so changing the fields later never rewrites it.`}
            </DialogDescription>
          </DialogHeader>

          {draft && (
            <div className="flex flex-col gap-5">
              <div className="flex flex-col gap-2">
                <Label htmlFor="form-name">{t`Name`}</Label>
                <Input
                  id="form-name"
                  value={draft.name}
                  onChange={(event) => {
                    const name = event.target.value
                    setDraft({ ...draft, name })
                    // Only while it is new: changing the address of a form
                    // already in use would stop whatever posts to it.
                    if (draft.id) return
                    const mine = ++asked.current
                    void getSlug(name)
                      .then((slug) => {
                        if (mine === asked.current) {
                          setDraft((current) =>
                            current && !current.id ? { ...current, slug } : current
                          )
                        }
                      })
                      .catch(() => undefined)
                  }}
                  autoFocus
                />
              </div>

              <div className="flex flex-col gap-2">
                <Label htmlFor="form-slug">{t`Address`}</Label>
                <Input
                  id="form-slug"
                  value={draft.slug}
                  onChange={(event) =>
                    setDraft({ ...draft, slug: event.target.value })
                  }
                  className="font-mono"
                  placeholder="contact"
                />
                <p className="text-sm text-muted-foreground">
                  {t`Answers are posted to /api/forms/${draft.slug || "…"}/submit. Changing it stops whatever already posts there.`}
                </p>
              </div>

              <div className="flex flex-col gap-2">
                <Label htmlFor="form-description">{t`Note`}</Label>
                <Textarea
                  id="form-description"
                  value={draft.description}
                  onChange={(event) =>
                    setDraft({ ...draft, description: event.target.value })
                  }
                  placeholder={t`What this form is for — only you see this`}
                  rows={2}
                />
              </div>

              <FormFieldsEditor
                fields={draft.fields}
                onChange={(fields) => setDraft({ ...draft, fields })}
              />

              <div className="flex flex-col gap-2">
                <Label htmlFor="form-notify">{t`Tell somebody`}</Label>
                <Input
                  id="form-notify"
                  type="email"
                  value={draft.notify}
                  onChange={(event) =>
                    setDraft({ ...draft, notify: event.target.value })
                  }
                  placeholder={t`Leave empty to tell nobody`}
                />
                <p className="text-sm text-muted-foreground">
                  {t`An email each time somebody fills this in. Needs Amazon SES switched on under Plugins — what comes in is kept either way.`}
                </p>
              </div>

              <Label className="flex items-center gap-3 font-normal">
                <Switch
                  checked={draft.active}
                  onCheckedChange={(checked) =>
                    setDraft({ ...draft, active: checked === true })
                  }
                />
                {t`Accepting answers`}
              </Label>
            </div>
          )}

          <DialogFooter>
            <Button variant="outline" onClick={() => setDraft(null)}>
              {t`Cancel`}
            </Button>
            <Button onClick={() => void save()} disabled={saving || !ready}>
              {saving ? <Loader2 className="animate-spin" /> : null}
              {t`Save`}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <AlertDialog
        open={removing !== null}
        onOpenChange={(open) => !open && setRemoving(null)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t`Remove this form?`}</AlertDialogTitle>
            <AlertDialogDescription>
              {t`${removing?.name} and the ${removing?.submissions ?? 0} answers it holds go with it. Switching it off instead keeps them.`}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t`Cancel`}</AlertDialogCancel>
            <AlertDialogAction onClick={() => void remove()}>
              {t`Remove`}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
}

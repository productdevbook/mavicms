/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { ArrowLeft, Check, Copy, Loader2, RefreshCw, Trash2 } from "lucide-react"
import { toast } from "sonner"

import {
  ApiError,
  deleteFormSubmission,
  getForm,
  getFormSubmissions,
  markFormSeen,
  type FormSubmission,
  type SiteForm,
} from "@/lib/api"
import { Button } from "@/components/ui/button"

export const Route = createFileRoute("/dashboard/forms_/$formId")({
  component: FormSubmissionsRoute,
})

/** What a value looks like in a table cell. */
function shown(value: unknown): string {
  if (value === null || value === undefined) return "—"
  if (typeof value === "boolean") return value ? "✓" : "✗"
  if (typeof value === "object") return JSON.stringify(value)
  return String(value)
}

function Snippet({ code }: { code: string }) {
  const { t } = useLingui()
  const [copied, setCopied] = React.useState(false)

  const copy = () => {
    void navigator.clipboard.writeText(code).then(
      () => {
        setCopied(true)
        setTimeout(() => setCopied(false), 1500)
      },
      () => toast.error(t`Could not copy it`)
    )
  }

  return (
    <div className="relative">
      <pre className="overflow-x-auto rounded-xl border border-border bg-muted/40 px-4 py-3 pr-12 text-xs">
        {code}
      </pre>
      <Button
        variant="ghost"
        size="icon-sm"
        aria-label={t`Copy`}
        className="absolute top-2 right-2"
        onClick={copy}
      >
        {copied ? <Check /> : <Copy />}
      </Button>
    </div>
  )
}

function FormSubmissionsRoute() {
  const { t } = useLingui()
  const navigate = useNavigate()
  const { formId } = Route.useParams()

  const [form, setForm] = React.useState<SiteForm | null>(null)
  const [rows, setRows] = React.useState<FormSubmission[] | null>(null)
  const [busy, setBusy] = React.useState(false)

  const load = React.useCallback(() => {
    Promise.all([getForm(formId), getFormSubmissions(formId)])
      .then(([one, submissions]) => {
        setForm(one)
        setRows(submissions)
      })
      .catch(() => toast.error(t`Could not load the form`))
  }, [formId, t])

  React.useEffect(load, [load])

  const markRead = async () => {
    setBusy(true)
    try {
      await markFormSeen(formId)
      load()
    } catch (error) {
      toast.error(error instanceof ApiError ? error.message : t`Could not do it`)
    } finally {
      setBusy(false)
    }
  }

  const remove = async (submission: FormSubmission) => {
    try {
      await deleteFormSubmission(formId, submission.id)
      setRows((current) =>
        (current ?? []).filter((row) => row.id !== submission.id)
      )
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not remove it`
      )
    }
  }

  if (!form || !rows) {
    return (
      <div className="flex justify-center py-16">
        <Loader2 className="size-6 animate-spin text-muted-foreground" />
      </div>
    )
  }

  const endpoint = `${window.location.origin}/api/forms/${form.slug}/submit`
  const example = JSON.stringify(
    Object.fromEntries(
      form.fields.map((field) => [
        field.name,
        field.type === "checkbox"
          ? true
          : field.type === "number"
            ? 1
            : field.type === "date"
              ? "2026-01-31"
              : field.type === "select"
                ? (field.options[0] ?? "")
                : field.label,
      ])
    ),
    null,
    2
  )

  return (
    <>
      <Button
        variant="ghost"
        size="sm"
        className="mb-4 -ml-2"
        onClick={() => void navigate({ to: "/dashboard/forms" })}
      >
        <ArrowLeft /> {t`Forms`}
      </Button>

      <div className="mb-6 flex items-start justify-between gap-4">
        <div>
          <h1 className="text-lg font-semibold">{form.name}</h1>
          <p className="text-sm text-muted-foreground">
            {t`${rows.length} received, ${form.unseen} not yet opened.`}
          </p>
        </div>
        <div className="flex gap-2">
          <Button variant="outline" size="sm" onClick={load}>
            <RefreshCw /> {t`Refresh`}
          </Button>
          <Button
            size="sm"
            disabled={busy || form.unseen === 0}
            onClick={() => void markRead()}
          >
            <Check /> {t`Mark all read`}
          </Button>
        </div>
      </div>

      <div className="mb-8 flex flex-col gap-2">
        <h2 className="text-sm font-medium">{t`How to send to it`}</h2>
        <p className="text-sm text-muted-foreground">
          {t`Post JSON to this address from your own pages or software. No account is needed, and only the fields above are accepted.`}
        </p>
        <Snippet
          code={`curl -X POST ${endpoint} \\\n  -H 'Content-Type: application/json' \\\n  -d '${example.replace(/\n\s*/g, " ")}'`}
        />
      </div>

      {rows.length === 0 ? (
        <p className="rounded-xl border border-dashed border-border py-12 text-center text-sm text-muted-foreground">
          {t`Nothing has come in yet`}
        </p>
      ) : (
        <div className="overflow-x-auto rounded-xl border border-border">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-border text-left">
                <th className="px-4 py-2 font-medium whitespace-nowrap">
                  {t`When`}
                </th>
                {form.fields.map((field) => (
                  <th
                    key={field.name}
                    className="px-4 py-2 font-medium whitespace-nowrap"
                  >
                    {field.label}
                  </th>
                ))}
                <th className="px-4 py-2" />
              </tr>
            </thead>
            <tbody>
              {rows.map((row) => (
                <tr
                  key={row.id}
                  className="border-b border-border last:border-0"
                >
                  <td className="px-4 py-2 whitespace-nowrap text-muted-foreground">
                    {!row.seen && (
                      <span className="surface-mark mr-2 inline-block size-1.5 rounded-full align-middle" />
                    )}
                    {new Date(row.created_at).toLocaleString()}
                  </td>
                  {form.fields.map((field) => (
                    <td key={field.name} className="max-w-xs px-4 py-2">
                      <span className="block truncate">
                        {shown(row.data[field.name])}
                      </span>
                    </td>
                  ))}
                  <td className="px-4 py-2">
                    <Button
                      variant="ghost"
                      size="icon-sm"
                      aria-label={t`Remove`}
                      onClick={() => void remove(row)}
                    >
                      <Trash2 />
                    </Button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </>
  )
}

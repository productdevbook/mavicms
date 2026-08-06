/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { Trans, useLingui } from "@lingui/react/macro"
import { ArrowLeft, CheckCircle2, Loader2, XCircle } from "lucide-react"
import { toast } from "sonner"

import {
  ApiError,
  getS3Settings,
  saveS3Settings,
  testS3Settings,
  type ConnectionTest,
  type S3SettingsPayload,
} from "@/lib/api"
import { Button } from "@/components/ui/button"
import { S3Fields } from "@/components/plugin-forms"

export const Route = createFileRoute("/dashboard/plugins_/s3")({
  component: S3SettingsRoute,
})

const EMPTY: S3SettingsPayload = {
  enabled: false,
  endpoint: "",
  region: "",
  bucket: "",
  access_key_id: "",
  secret_access_key: "",
  public_base_url: "",
  path_prefix: "",
}

function S3SettingsRoute() {
  const { t } = useLingui()
  const navigate = useNavigate()
  const [form, setForm] = React.useState<S3SettingsPayload>(EMPTY)
  const [hasStoredSecret, setHasStoredSecret] = React.useState(false)
  const [loading, setLoading] = React.useState(true)
  const [busy, setBusy] = React.useState<"save" | "test" | null>(null)
  const [testResult, setTestResult] = React.useState<ConnectionTest | null>(null)

  React.useEffect(() => {
    getS3Settings()
      .then((settings) => {
        setForm({ ...settings, secret_access_key: "" })
        setHasStoredSecret(settings.has_secret_access_key)
      })
      .catch(() => {})
      .finally(() => setLoading(false))
  }, [])

  const patch = (fields: Partial<S3SettingsPayload>) =>
    setForm((current) => ({ ...current, ...fields }))

  // An empty secret field means "keep the stored one", so it must not be sent.
  const payload = (): S3SettingsPayload => ({
    ...form,
    secret_access_key: form.secret_access_key?.trim()
      ? form.secret_access_key
      : undefined,
  })

  const runTest = async () => {
    setBusy("test")
    setTestResult(null)
    try {
      setTestResult(await testS3Settings(payload()))
    } catch (error) {
      setTestResult({
        ok: false,
        message: error instanceof ApiError ? error.message : t`Request failed`,
      })
    } finally {
      setBusy(null)
    }
  }

  const save = async () => {
    setBusy("save")
    try {
      const saved = await saveS3Settings(payload())
      setForm({ ...saved, secret_access_key: "" })
      setHasStoredSecret(saved.has_secret_access_key)
      toast.success(t`Settings saved`)
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not save settings`
      )
    } finally {
      setBusy(null)
    }
  }

  if (loading) {
    return (
      <div className="flex justify-center py-16">
        <Loader2 className="size-6 animate-spin text-muted-foreground" />
      </div>
    )
  }

  return (
    <>
      <Button
        variant="ghost"
        size="sm"
        className="mb-4 -ml-2"
        onClick={() => navigate({ to: "/dashboard/plugins" })}
      >
        <ArrowLeft /> {t`Plugins`}
      </Button>

      <div className="mb-6">
        <h1 className="text-lg font-semibold">{t`S3 compatible storage`}</h1>
        <p className="text-sm text-muted-foreground">
          {t`Works with AWS S3, Cloudflare R2, MinIO and DigitalOcean Spaces. Existing local files stay where they are; only new uploads go to the bucket.`}
        </p>
      </div>

      <form
        onSubmit={(event) => {
          event.preventDefault()
          void save()
        }}
        className="flex max-w-xl flex-col gap-4"
      >
        <S3Fields
          form={form}
          hasStoredSecret={hasStoredSecret}
          onChange={patch}
        />

        {testResult && (
          <p
            className={
              testResult.ok
                ? "flex items-start gap-2 rounded-md bg-emerald-500/10 px-3 py-2 text-xs text-emerald-600 dark:text-emerald-400"
                : "flex items-start gap-2 rounded-md bg-destructive/10 px-3 py-2 text-xs text-destructive"
            }
          >
            {testResult.ok ? (
              <CheckCircle2 className="mt-px size-3.5 shrink-0" />
            ) : (
              <XCircle className="mt-px size-3.5 shrink-0" />
            )}
            {testResult.message}
          </p>
        )}

        <p className="text-xs text-muted-foreground">
          <Trans>
            Credentials are encrypted before they are stored and are never sent
            back to the browser.
          </Trans>
        </p>

        <div className="flex gap-2">
          <Button
            type="button"
            variant="outline"
            disabled={busy !== null}
            onClick={() => void runTest()}
          >
            {busy === "test" ? (
              <Loader2 className="animate-spin" />
            ) : null}
            {t`Test connection`}
          </Button>
          <Button type="submit" disabled={busy !== null}>
            {busy === "save" ? <Loader2 className="animate-spin" /> : null}
            {t`Save`}
          </Button>
        </div>
      </form>
    </>
  )
}

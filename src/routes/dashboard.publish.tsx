/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { Loader2 } from "lucide-react"
import { toast } from "sonner"

import {
  ApiError,
  getPublish,
  requestPublish,
  savePublish,
  type BuildConfig,
  type PublishStatus,
} from "@/lib/api"
import {
  BuildHistory,
  ManagedSummary,
  PublishButton,
  PublishForm,
} from "@/components/publish-panel"
import { EMPTY_CONFIG, isBusy, parseEnvironment } from "@/lib/publish"

export const Route = createFileRoute("/dashboard/publish")({
  component: PublishRoute,
})

/** How often to look again while a build is running. */
const WATCH_INTERVAL = 3000

function PublishRoute() {
  const { t } = useLingui()
  const [status, setStatus] = React.useState<PublishStatus | null>(null)
  const [config, setConfig] = React.useState<BuildConfig>(EMPTY_CONFIG)
  const [token, setToken] = React.useState("")
  const [environment, setEnvironment] = React.useState("")
  const [saving, setSaving] = React.useState(false)
  const [publishing, setPublishing] = React.useState(false)

  const apply = React.useCallback((next: PublishStatus) => {
    setStatus(next)
    setConfig(next.config ?? EMPTY_CONFIG)
  }, [])

  React.useEffect(() => {
    getPublish()
      .then(apply)
      .catch(() => toast.error(t`Could not load the publishing settings`))
  }, [apply, t])

  // A build in flight is the only reason to keep asking; when none is, this
  // stops on its own rather than polling an idle site forever.
  const busy = isBusy(status?.builds ?? [])
  React.useEffect(() => {
    if (!busy) return
    const timer = setInterval(() => {
      getPublish()
        .then(apply)
        .catch(() => undefined)
    }, WATCH_INTERVAL)
    return () => clearInterval(timer)
  }, [busy, apply])

  const save = async () => {
    setSaving(true)
    try {
      const saved = await savePublish({
        repository: config.repository,
        branch: config.branch,
        build_command: config.build_command,
        output_dir: config.output_dir,
        // Left out unless typed, so saving the form does not clear what is
        // already stored.
        ...(token ? { token } : {}),
        ...(environment.trim()
          ? { environment: parseEnvironment(environment) }
          : {}),
      })
      setConfig(saved)
      setToken("")
      setEnvironment("")
      toast.success(t`Saved`)
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not save the settings`
      )
    } finally {
      setSaving(false)
    }
  }

  const publish = async () => {
    setPublishing(true)
    try {
      await requestPublish()
      apply(await getPublish())
      toast.success(t`Publishing — this takes a minute or two`)
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not start the build`
      )
    } finally {
      setPublishing(false)
    }
  }

  if (!status) {
    return (
      <div className="flex justify-center py-16">
        <Loader2 className="size-6 animate-spin text-muted-foreground" />
      </div>
    )
  }

  return (
    <>
      <div className="mb-6 flex items-start justify-between gap-4">
        <div>
          <h1 className="text-lg font-semibold">{t`Publish`}</h1>
          <p className="text-sm text-muted-foreground">
            {t`Your pages are built from your own project. Publishing builds them again with what is in the CMS now.`}
          </p>
        </div>
        <PublishButton
          onPublish={() => void publish()}
          publishing={publishing}
          busy={busy}
          disabled={!config.repository}
        />
      </div>

      <div className="flex max-w-2xl flex-col gap-6">
        {status.managed_by !== null ? (
          <ManagedSummary status={status} />
        ) : (
          <PublishForm
            config={config}
            onChange={(values) =>
              setConfig((current) => ({ ...current, ...values }))
            }
            token={token}
            onTokenChange={setToken}
            environment={environment}
            onEnvironmentChange={setEnvironment}
            onSave={() => void save()}
            saving={saving}
          />
        )}

        <div>
          <h2 className="mb-2 text-sm font-medium">{t`Recent builds`}</h2>
          <BuildHistory builds={status.builds} />
        </div>
      </div>
    </>
  )
}

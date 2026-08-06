/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { ArrowLeft, ExternalLink, Loader2 } from "lucide-react"
import { toast } from "sonner"

import {
  ApiError,
  createSiteEntry,
  getConsoleAccount,
  getConsoleSites,
  getSitePublish,
  requestSitePublish,
  saveSitePublish,
  type BuildConfig,
  type ConsoleSite,
  type PublishStatus,
} from "@/lib/api"
import { Button } from "@/components/ui/button"
import {
  BuildHistory,
  PublishButton,
  PublishForm,
} from "@/components/publish-panel"
import { EMPTY_CONFIG, isBusy, parseEnvironment } from "@/lib/publish"

export const Route = createFileRoute("/console/sites/$siteId")({
  beforeLoad: async ({ params }) => {
    await getConsoleAccount().catch(() => {
      throw redirect({ to: "/console/login" })
    })
    // The list is the agency's own, so a site that is not in it is one this
    // page has no business showing.
    const site = (await getConsoleSites()).find((s) => s.id === params.siteId)
    if (!site) throw redirect({ to: "/console" })
    return { site }
  },
  component: ConsoleSiteRoute,
})

const WATCH_INTERVAL = 3000

function ConsoleSiteRoute() {
  const { t } = useLingui()
  const navigate = useNavigate()
  const { siteId } = Route.useParams()
  const { site } = Route.useRouteContext() as { site: ConsoleSite }

  const [status, setStatus] = React.useState<PublishStatus | null>(null)
  const [config, setConfig] = React.useState<BuildConfig>(EMPTY_CONFIG)
  const [token, setToken] = React.useState("")
  const [environment, setEnvironment] = React.useState("")
  const [saving, setSaving] = React.useState(false)
  const [publishing, setPublishing] = React.useState(false)
  const [opening, setOpening] = React.useState(false)

  const apply = React.useCallback((next: PublishStatus) => {
    setStatus(next)
    setConfig(next.config ?? EMPTY_CONFIG)
  }, [])

  React.useEffect(() => {
    getSitePublish(siteId)
      .then(apply)
      .catch(() => toast.error(t`Could not load the publishing settings`))
  }, [siteId, apply, t])

  const busy = isBusy(status?.builds ?? [])
  React.useEffect(() => {
    if (!busy) return
    const timer = setInterval(() => {
      getSitePublish(siteId)
        .then(apply)
        .catch(() => undefined)
    }, WATCH_INTERVAL)
    return () => clearInterval(timer)
  }, [busy, siteId, apply])

  const save = async () => {
    setSaving(true)
    try {
      setConfig(
        await saveSitePublish(siteId, {
          repository: config.repository,
          branch: config.branch,
          build_command: config.build_command,
          output_dir: config.output_dir,
          ...(token ? { token } : {}),
          ...(environment.trim()
            ? { environment: parseEnvironment(environment) }
            : {}),
        })
      )
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
      await requestSitePublish(siteId)
      apply(await getSitePublish(siteId))
      toast.success(t`Publishing — this takes a minute or two`)
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not start the build`
      )
    } finally {
      setPublishing(false)
    }
  }

  const open = async () => {
    setOpening(true)
    try {
      const { url } = await createSiteEntry(siteId)
      window.open(url, "_blank", "noopener")
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not open the site`
      )
    } finally {
      setOpening(false)
    }
  }

  return (
    <div className="min-h-svh bg-background">
      <main className="mx-auto w-full max-w-3xl px-6 py-8">
        <Button
          variant="ghost"
          size="sm"
          className="mb-4 -ml-2"
          onClick={() => void navigate({ to: "/console" })}
        >
          <ArrowLeft /> {t`Your sites`}
        </Button>

        <div className="mb-6 flex items-start justify-between gap-4">
          <div>
            <h1 className="text-lg font-semibold">{site.host}</h1>
            <p className="text-sm text-muted-foreground">
              {t`Where this site's pages come from, and how the last builds went.`}
            </p>
          </div>
          <div className="flex gap-2">
            <Button
              variant="outline"
              onClick={() => void open()}
              disabled={opening}
            >
              {opening ? (
                <Loader2 className="animate-spin" />
              ) : (
                <ExternalLink />
              )}
              {t`Manage`}
            </Button>
            <PublishButton
              onPublish={() => void publish()}
              publishing={publishing}
              busy={busy}
              disabled={!config.repository}
            />
          </div>
        </div>

        {!status ? (
          <div className="flex justify-center py-16">
            <Loader2 className="size-6 animate-spin text-muted-foreground" />
          </div>
        ) : (
          <div className="flex flex-col gap-6">
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

            <div>
              <h2 className="mb-2 text-sm font-medium">{t`Recent builds`}</h2>
              <BuildHistory builds={status.builds} />
            </div>
          </div>
        )}
      </main>
    </div>
  )
}

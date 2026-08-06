/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { CheckCircle2, Clock, Loader2, Rocket, XCircle } from "lucide-react"
import { toast } from "sonner"

import {
  ApiError,
  getPublish,
  requestPublish,
  savePublish,
  type Build,
  type BuildConfig,
  type PublishStatus,
} from "@/lib/api"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"

export const Route = createFileRoute("/dashboard/publish")({
  component: PublishRoute,
})

/** How often to look again while a build is running. */
const WATCH_INTERVAL = 3000

const EMPTY: BuildConfig = {
  repository: "",
  branch: "main",
  build_command: "bun install --frozen-lockfile && bun run build",
  output_dir: "dist",
  has_token: false,
  environment_keys: [],
}

/** "NAME=value" lines, as the object the API takes. */
function parseEnvironment(text: string): Record<string, string> {
  const values: Record<string, string> = {}
  for (const line of text.split("\n")) {
    const trimmed = line.trim()
    // A blank line is spacing and a "#" line is a note; neither is a variable.
    if (!trimmed || trimmed.startsWith("#")) continue
    const at = trimmed.indexOf("=")
    if (at <= 0) continue
    values[trimmed.slice(0, at).trim()] = trimmed.slice(at + 1).trim()
  }
  return values
}

function StatusIcon({ status }: { status: Build["status"] }) {
  if (status === "succeeded") return <CheckCircle2 className="size-4 text-emerald-600" />
  if (status === "failed") return <XCircle className="size-4 text-destructive" />
  if (status === "running") return <Loader2 className="size-4 animate-spin" />
  return <Clock className="size-4 text-muted-foreground" />
}

function PublishRoute() {
  const { t } = useLingui()
  const [config, setConfig] = React.useState<BuildConfig | null>(null)
  const [builds, setBuilds] = React.useState<Build[]>([])
  const [token, setToken] = React.useState("")
  // One "NAME=value" per line, which is how anyone who has set these before
  // expects to type them. Left untouched, the stored ones stay as they are.
  const [environment, setEnvironment] = React.useState("")
  const [loading, setLoading] = React.useState(true)
  const [saving, setSaving] = React.useState(false)
  const [publishing, setPublishing] = React.useState(false)
  const [open, setOpen] = React.useState<string | null>(null)

  const apply = React.useCallback((status: PublishStatus) => {
    setConfig(status.config ?? EMPTY)
    setBuilds(status.builds)
    setLoading(false)
  }, [])

  React.useEffect(() => {
    getPublish()
      .then(apply)
      .catch(() => toast.error(t`Could not load the publishing settings`))
  }, [apply, t])

  // A build in flight is the only reason to keep asking; when none is, this
  // stops on its own rather than polling an idle site forever.
  const busy = builds.some(
    (build) => build.status === "queued" || build.status === "running"
  )
  React.useEffect(() => {
    if (!busy) return
    const timer = setInterval(() => {
      getPublish().then(apply).catch(() => undefined)
    }, WATCH_INTERVAL)
    return () => clearInterval(timer)
  }, [busy, apply])

  const patch = (values: Partial<BuildConfig>) =>
    setConfig((current) => (current ? { ...current, ...values } : current))

  const save = async () => {
    if (!config) return
    setSaving(true)
    try {
      setConfig(
        await savePublish({
          repository: config.repository,
          branch: config.branch,
          build_command: config.build_command,
          output_dir: config.output_dir,
          // Left out unless typed, so saving the form does not clear what is
          // already stored.
          ...(token ? { token } : {}),
          ...(environment.trim() ? { environment: parseEnvironment(environment) } : {}),
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

  if (loading || !config) {
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
        <Button
          onClick={() => void publish()}
          disabled={publishing || busy || !config.repository}
        >
          {publishing || busy ? <Loader2 className="animate-spin" /> : <Rocket />}
          {busy ? t`Publishing…` : t`Publish`}
        </Button>
      </div>

      <div className="flex max-w-2xl flex-col gap-6">
        <div className="flex flex-col gap-2">
          <Label htmlFor="repository">{t`Repository`}</Label>
          <Input
            id="repository"
            value={config.repository}
            onChange={(event) => patch({ repository: event.target.value })}
            placeholder="https://github.com/example/site"
          />
        </div>

        <div className="grid gap-4 sm:grid-cols-2">
          <div className="flex flex-col gap-2">
            <Label htmlFor="branch">{t`Branch`}</Label>
            <Input
              id="branch"
              value={config.branch}
              onChange={(event) => patch({ branch: event.target.value })}
            />
          </div>
          <div className="flex flex-col gap-2">
            <Label htmlFor="output">{t`Output folder`}</Label>
            <Input
              id="output"
              value={config.output_dir}
              onChange={(event) => patch({ output_dir: event.target.value })}
            />
          </div>
        </div>

        <div className="flex flex-col gap-2">
          <Label htmlFor="command">{t`Build command`}</Label>
          <Input
            id="command"
            value={config.build_command}
            onChange={(event) => patch({ build_command: event.target.value })}
          />
          <p className="text-sm text-muted-foreground">
            {t`It runs with CMS_API_URL and SITE_URL set to this site's address.`}
          </p>
        </div>

        <div className="flex flex-col gap-2">
          <Label htmlFor="token">{t`Access token`}</Label>
          <Input
            id="token"
            type="password"
            value={token}
            onChange={(event) => setToken(event.target.value)}
            placeholder={
              config.has_token ? t`Stored — type to replace` : t`Only for a private repository`
            }
          />
        </div>

        <div className="flex flex-col gap-2">
          <Label htmlFor="environment">{t`Build variables`}</Label>
          <textarea
            id="environment"
            rows={4}
            value={environment}
            onChange={(event) => setEnvironment(event.target.value)}
            placeholder={"CMS_USERNAME=…\nCMS_PASSWORD=…"}
            className="rounded-md border border-input bg-transparent px-3 py-2 font-mono text-sm shadow-xs outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50"
          />
          <p className="text-sm text-muted-foreground">
            {config.environment_keys.length > 0
              ? t`Stored: ${config.environment_keys.join(", ")}. Typing here replaces all of them.`
              : t`One NAME=value per line. This is where the build's credentials go.`}
          </p>
        </div>

        <div>
          <Button onClick={() => void save()} disabled={saving}>
            {saving ? <Loader2 className="animate-spin" /> : null}
            {t`Save`}
          </Button>
        </div>

        <div>
          <h2 className="mb-2 text-sm font-medium">{t`Recent builds`}</h2>
          {builds.length === 0 ? (
            <p className="rounded-xl border border-dashed border-border py-10 text-center text-sm text-muted-foreground">
              {t`Nothing published yet`}
            </p>
          ) : (
            <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
              {builds.map((build) => (
                <div key={build.id} className="flex flex-col">
                  <button
                    type="button"
                    className="flex items-center gap-3 px-4 py-2.5 text-left hover:bg-muted/50"
                    onClick={() =>
                      setOpen((current) => (current === build.id ? null : build.id))
                    }
                  >
                    <StatusIcon status={build.status} />
                    <div className="min-w-0 flex-1">
                      <p className="text-sm">
                        {new Date(build.requested_at).toLocaleString()}
                      </p>
                      {build.finished_at && build.started_at && (
                        <p className="text-xs text-muted-foreground">
                          {t`took ${Math.max(
                            1,
                            Math.round(
                              (new Date(build.finished_at).getTime() -
                                new Date(build.started_at).getTime()) /
                                1000
                            )
                          )} seconds`}
                        </p>
                      )}
                    </div>
                  </button>
                  {open === build.id && build.log && (
                    <pre className="max-h-96 overflow-auto border-t border-border bg-muted/40 px-4 py-3 text-xs">
                      {build.log}
                    </pre>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </>
  )
}

import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { CheckCircle2, Clock, Loader2, Rocket, XCircle } from "lucide-react"

import type { Build, BuildConfig, PublishStatus } from "@/lib/api"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"

/**
 * Publishing, from either side of it.
 *
 * The agency sees the form: which repository, which branch, what builds it.
 * The site sees the button and what happened, because a repository address is
 * the agency's work and someone writing posts should not be able to stop the
 * site publishing by editing it.
 */

function StatusIcon({ status }: { status: Build["status"] }) {
  if (status === "succeeded")
    return <CheckCircle2 className="size-4 text-emerald-600" />
  if (status === "failed") return <XCircle className="size-4 text-destructive" />
  if (status === "running") return <Loader2 className="size-4 animate-spin" />
  return <Clock className="size-4 text-muted-foreground" />
}

export function BuildHistory({ builds }: { builds: Build[] }) {
  const { t } = useLingui()
  const [open, setOpen] = React.useState<string | null>(null)

  if (builds.length === 0) {
    return (
      <p className="rounded-xl border border-dashed border-border py-10 text-center text-sm text-muted-foreground">
        {t`Nothing published yet`}
      </p>
    )
  }

  return (
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
  )
}

export function PublishForm({
  config,
  onChange,
  token,
  onTokenChange,
  environment,
  onEnvironmentChange,
  onSave,
  saving,
}: {
  config: BuildConfig
  onChange: (values: Partial<BuildConfig>) => void
  token: string
  onTokenChange: (value: string) => void
  environment: string
  onEnvironmentChange: (value: string) => void
  onSave: () => void
  saving: boolean
}) {
  const { t } = useLingui()

  return (
    <>
      <div className="flex flex-col gap-2">
        <Label htmlFor="repository">{t`Repository`}</Label>
        <Input
          id="repository"
          value={config.repository}
          onChange={(event) => onChange({ repository: event.target.value })}
          placeholder="https://github.com/example/site"
        />
      </div>

      <div className="grid gap-4 sm:grid-cols-2">
        <div className="flex flex-col gap-2">
          <Label htmlFor="branch">{t`Branch`}</Label>
          <Input
            id="branch"
            value={config.branch}
            onChange={(event) => onChange({ branch: event.target.value })}
          />
        </div>
        <div className="flex flex-col gap-2">
          <Label htmlFor="output">{t`Output folder`}</Label>
          <Input
            id="output"
            value={config.output_dir}
            onChange={(event) => onChange({ output_dir: event.target.value })}
          />
        </div>
      </div>

      <div className="flex flex-col gap-2">
        <Label htmlFor="command">{t`Build command`}</Label>
        <Input
          id="command"
          value={config.build_command}
          onChange={(event) => onChange({ build_command: event.target.value })}
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
          onChange={(event) => onTokenChange(event.target.value)}
          placeholder={
            config.has_token
              ? t`Stored — type to replace`
              : t`Only for a private repository`
          }
        />
      </div>

      <div className="flex flex-col gap-2">
        <Label htmlFor="environment">{t`Build variables`}</Label>
        <textarea
          id="environment"
          rows={4}
          value={environment}
          onChange={(event) => onEnvironmentChange(event.target.value)}
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
        <Button onClick={onSave} disabled={saving}>
          {saving ? <Loader2 className="animate-spin" /> : null}
          {t`Save`}
        </Button>
      </div>
    </>
  )
}

/** What a site is shown when its agency looks after the wiring. */
export function ManagedSummary({
  status,
}: {
  status: PublishStatus
}) {
  const { t } = useLingui()

  return (
    <div className="rounded-xl border border-border px-4 py-3 text-sm">
      <p className="text-muted-foreground">
        {t`${status.managed_by} looks after how this site is built.`}
      </p>
      {status.config?.repository && (
        <p className="mt-1 truncate font-mono text-xs text-muted-foreground">
          {status.config.repository} · {status.config.branch}
        </p>
      )}
    </div>
  )
}

export function PublishButton({
  onPublish,
  publishing,
  busy,
  disabled,
}: {
  onPublish: () => void
  publishing: boolean
  busy: boolean
  disabled: boolean
}) {
  const { t } = useLingui()

  return (
    <Button onClick={onPublish} disabled={publishing || busy || disabled}>
      {publishing || busy ? <Loader2 className="animate-spin" /> : <Rocket />}
      {busy ? t`Publishing…` : t`Publish`}
    </Button>
  )
}

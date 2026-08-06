/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { ArrowLeft, ExternalLink, Loader2, Play, RotateCcw } from "lucide-react"
import { toast } from "sonner"

import {
  ApiError,
  createSiteEntry,
  getConsoleAccount,
  getConsoleSites,
  getSiteBackup,
  getSitePublish,
  getSiteS3,
  requestSitePublish,
  restoreSiteBackup,
  runSiteBackup,
  saveSiteBackup,
  saveSitePublish,
  saveSiteS3,
  type BackupConfig,
  type BackupSettings,
  type BuildConfig,
  type ConsoleSite,
  type PublishStatus,
  type S3Settings,
  type S3SettingsPayload,
  type SavePublish,
} from "@/lib/api"
import { Button } from "@/components/ui/button"
import { applySurface } from "@/lib/surface"
import { cn } from "@/lib/utils"
import {
  BuildHistory,
  PublishButton,
  PublishForm,
} from "@/components/publish-panel"
import { BackupFields, S3Fields } from "@/components/plugin-forms"
import { EMPTY_CONFIG, isBusy } from "@/lib/publish"

type Tab = "publish" | "storage" | "backups"

/** The server never sends the secret back; empty means keep what is stored. */
function payloadOf(settings: S3Settings): S3SettingsPayload {
  return {
    enabled: settings.enabled,
    endpoint: settings.endpoint,
    region: settings.region,
    bucket: settings.bucket,
    access_key_id: settings.access_key_id,
    secret_access_key: "",
    public_base_url: settings.public_base_url,
    path_prefix: settings.path_prefix,
  }
}

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
  const [saving, setSaving] = React.useState(false)
  const [publishing, setPublishing] = React.useState(false)
  const [opening, setOpening] = React.useState(false)

  const [tab, setTab] = React.useState<Tab>("publish")
  const [s3, setS3] = React.useState<S3Settings | null>(null)
  const [s3Form, setS3Form] = React.useState<S3SettingsPayload | null>(null)
  const [backup, setBackup] = React.useState<BackupSettings | null>(null)
  const [backupConfig, setBackupConfig] = React.useState<BackupConfig | null>(
    null
  )
  const [backupEnabled, setBackupEnabled] = React.useState(false)

  React.useEffect(() => {
    applySurface({ kind: "console", name: site.host })
  }, [site.host])

  const apply = React.useCallback((next: PublishStatus) => {
    setStatus(next)
    setConfig(next.config ?? EMPTY_CONFIG)
  }, [])

  const applyS3 = React.useCallback((next: S3Settings) => {
    setS3(next)
    setS3Form(payloadOf(next))
  }, [])

  const applyBackup = React.useCallback((next: BackupSettings) => {
    setBackup(next)
    setBackupConfig(next.config)
    setBackupEnabled(next.enabled)
  }, [])

  // Fetched when the tab is first opened rather than all at once: three
  // requests for a page somebody came to for one of them is three requests
  // more than the page needs.
  React.useEffect(() => {
    if (tab === "storage" && !s3) {
      getSiteS3(siteId)
        .then(applyS3)
        .catch(() => toast.error(t`Could not load the storage settings`))
    }
    if (tab === "backups" && !backup) {
      getSiteBackup(siteId)
        .then(applyBackup)
        .catch(() => toast.error(t`Could not load the backup settings`))
    }
  }, [tab, siteId, s3, backup, applyS3, applyBackup, t])

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

  const saveWith = async (extra: Partial<SavePublish>) => {
    setSaving(true)
    try {
      const saved = await saveSitePublish.bind(null, siteId)({
        repository: config.repository,
        branch: config.branch,
        build_command: config.build_command,
        output_dir: config.output_dir,
        // Left out unless typed, so saving the form does not clear a token
        // that is already stored.
        ...(token ? { token } : {}),
        ...extra,
      })
      setConfig(saved)
      setToken("")
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

  const saveStorage = async () => {
    if (!s3Form) return
    setSaving(true)
    try {
      applyS3(await saveSiteS3(siteId, s3Form))
      toast.success(t`Saved`)
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not save the settings`
      )
    } finally {
      setSaving(false)
    }
  }

  const saveBackups = async () => {
    if (!backupConfig) return
    setSaving(true)
    try {
      applyBackup(await saveSiteBackup(siteId, backupEnabled, backupConfig))
      toast.success(t`Saved`)
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not save the settings`
      )
    } finally {
      setSaving(false)
    }
  }

  const backUpNow = async () => {
    setSaving(true)
    try {
      const file = await runSiteBackup(siteId)
      toast.success(t`Backup written: ${file.name}`)
      applyBackup(await getSiteBackup(siteId))
    } catch (error) {
      toast.error(error instanceof ApiError ? error.message : t`The backup failed`)
    } finally {
      setSaving(false)
    }
  }

  const restore = async (name: string) => {
    setSaving(true)
    try {
      const report = await restoreSiteBackup(siteId, name)
      const rows = Object.values(report.tables).reduce((sum, n) => sum + n, 0)
      toast.success(t`Restored: ${rows} rows and ${report.media_files} files`)
      applyBackup(await getSiteBackup(siteId))
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`The restore failed`
      )
    } finally {
      setSaving(false)
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
    <div className="surface-bar min-h-svh bg-background">
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

        <div className="mb-6 flex gap-1 border-b border-border">
          {(
            [
              ["publish", t`Publish`],
              ["storage", t`Storage`],
              ["backups", t`Backups`],
            ] as const
          ).map(([id, label]) => (
            <button
              key={id}
              type="button"
              onClick={() => setTab(id)}
              className={cn(
                "-mb-px border-b-2 px-3 py-2 text-sm font-medium",
                tab === id
                  ? "border-primary text-foreground"
                  : "border-transparent text-muted-foreground hover:text-foreground"
              )}
            >
              {label}
            </button>
          ))}
        </div>

        {tab === "publish" &&
          (!status ? (
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
                onAddVariable={(name, value) =>
                  void saveWith({ environment_set: { [name]: value } })
                }
                onRemoveVariable={(name) =>
                  void saveWith({ environment_remove: [name] })
                }
                onSave={() => void saveWith({})}
                saving={saving}
              />

              <div>
                <h2 className="mb-2 text-sm font-medium">{t`Recent builds`}</h2>
                <BuildHistory builds={status.builds} />
              </div>
            </div>
          ))}

        {tab === "storage" &&
          (!s3Form || !s3 ? (
            <div className="flex justify-center py-16">
              <Loader2 className="size-6 animate-spin text-muted-foreground" />
            </div>
          ) : (
            <div className="flex flex-col gap-4">
              <S3Fields
                form={s3Form}
                hasStoredSecret={s3.has_secret_access_key}
                onChange={(values) =>
                  setS3Form((current) =>
                    current ? { ...current, ...values } : current
                  )
                }
              />
              <div>
                <Button onClick={() => void saveStorage()} disabled={saving}>
                  {saving ? <Loader2 className="animate-spin" /> : null}
                  {t`Save`}
                </Button>
              </div>
            </div>
          ))}

        {tab === "backups" &&
          (!backup || !backupConfig ? (
            <div className="flex justify-center py-16">
              <Loader2 className="size-6 animate-spin text-muted-foreground" />
            </div>
          ) : (
            <div className="flex flex-col gap-6">
              <BackupFields
                settings={backup}
                config={backupConfig}
                enabled={backupEnabled}
                onEnabledChange={setBackupEnabled}
                onChange={(values) =>
                  setBackupConfig((current) =>
                    current ? { ...current, ...values } : current
                  )
                }
              />

              <div className="flex flex-wrap gap-2">
                <Button onClick={() => void saveBackups()} disabled={saving}>
                  {saving ? <Loader2 className="animate-spin" /> : null}
                  {t`Save`}
                </Button>
                <Button
                  variant="outline"
                  onClick={() => void backUpNow()}
                  disabled={saving}
                >
                  <Play /> {t`Back up now`}
                </Button>
              </div>

              <div>
                <h2 className="mb-2 text-sm font-medium">{t`Archives`}</h2>
                {backup.backups.length === 0 ? (
                  <p className="rounded-xl border border-dashed border-border py-10 text-center text-sm text-muted-foreground">
                    {t`No backups yet`}
                  </p>
                ) : (
                  <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
                    {backup.backups.map((file) => (
                      <div
                        key={file.name}
                        className="flex items-center gap-3 px-4 py-2.5"
                      >
                        <div className="min-w-0 flex-1">
                          <p className="truncate text-sm">{file.name}</p>
                          <p className="text-xs text-muted-foreground">
                            {(file.size_bytes / 1024).toFixed(0)} KB ·{" "}
                            {new Date(file.created_at).toLocaleString()}
                          </p>
                        </div>
                        <Button
                          variant="ghost"
                          size="sm"
                          disabled={saving}
                          onClick={() => void restore(file.name)}
                        >
                          <RotateCcw /> {t`Restore`}
                        </Button>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            </div>
          ))}
      </main>
    </div>
  )
}

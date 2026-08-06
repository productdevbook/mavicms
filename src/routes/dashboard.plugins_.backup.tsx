/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { ArrowLeft, Download, Loader2, Play, Trash2 } from "lucide-react"
import { toast } from "sonner"

import {
  ApiError,
  deleteBackup,
  getBackupSettings,
  runBackup,
  saveBackupSettings,
  type BackupConfig,
  type BackupSettings,
} from "@/lib/api"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
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

export const Route = createFileRoute("/dashboard/plugins_/backup")({
  component: BackupSettingsRoute,
})

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}

function BackupSettingsRoute() {
  const { t } = useLingui()
  const navigate = useNavigate()
  const [settings, setSettings] = React.useState<BackupSettings | null>(null)
  const [enabled, setEnabled] = React.useState(false)
  const [config, setConfig] = React.useState<BackupConfig | null>(null)
  const [saving, setSaving] = React.useState(false)
  const [running, setRunning] = React.useState(false)
  const [pendingDelete, setPendingDelete] = React.useState<string | null>(null)

  const apply = React.useCallback((next: BackupSettings) => {
    setSettings(next)
    setEnabled(next.enabled)
    setConfig(next.config)
  }, [])

  React.useEffect(() => {
    getBackupSettings()
      .then(apply)
      .catch(() => toast.error(t`Could not load the backup settings`))
  }, [apply, t])

  const SCHEDULE_LABELS: Record<string, string> = {
    off: t`Only by hand`,
    hourly: t`Every hour`,
    daily: t`Every day`,
    weekly: t`Every week`,
  }

  const patch = (values: Partial<BackupConfig>) =>
    setConfig((current) => (current ? { ...current, ...values } : current))

  const save = async () => {
    if (!config) return
    setSaving(true)
    try {
      apply(await saveBackupSettings(enabled, config))
      toast.success(t`Backup settings saved`)
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not save the settings`
      )
    } finally {
      setSaving(false)
    }
  }

  const backUpNow = async () => {
    setRunning(true)
    try {
      const file = await runBackup()
      toast.success(t`Backup written: ${file.name}`)
      apply(await getBackupSettings())
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`The backup failed`
      )
    } finally {
      setRunning(false)
    }
  }

  const confirmDelete = async () => {
    if (!pendingDelete) return
    try {
      await deleteBackup(pendingDelete)
      apply(await getBackupSettings())
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not delete the backup`
      )
    } finally {
      setPendingDelete(null)
    }
  }

  if (!settings || !config) {
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
        <h1 className="text-lg font-semibold">{t`Backups`}</h1>
        <p className="text-sm text-muted-foreground">
          {t`An archive holding the database, and the uploaded files if you ask for them.`}
        </p>
      </div>

      <div className="flex max-w-2xl flex-col gap-6">
        <div className="flex items-center justify-between rounded-xl border border-border px-4 py-3">
          <div>
            <p className="text-sm font-medium">{t`Enabled`}</p>
            <p className="text-sm text-muted-foreground">
              {t`Scheduled backups only run while this is on. You can always back up by hand below.`}
            </p>
          </div>
          <Switch checked={enabled} onCheckedChange={setEnabled} />
        </div>

        <div className="flex flex-col gap-2">
          <Label htmlFor="destination">{t`Where to keep them`}</Label>
          <Select
            value={config.destination}
            onValueChange={(value) =>
              patch({ destination: (value as BackupConfig["destination"]) ?? "local" })
            }
          >
            <SelectTrigger id="destination">
              <SelectValue>
                {(value: string) =>
                  value === "s3"
                    ? settings.s3_bucket
                      ? t`S3 bucket (${settings.s3_bucket})`
                      : t`S3 bucket`
                    : t`On the server's disk`
                }
              </SelectValue>
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="local">{t`On the server's disk`}</SelectItem>
              {/* Offered only once the bucket is set up, so the choice cannot
                  be made before it can work. */}
              {settings.s3_available && (
                <SelectItem value="s3">
                  {settings.s3_bucket
                    ? t`S3 bucket (${settings.s3_bucket})`
                    : t`S3 bucket`}
                </SelectItem>
              )}
            </SelectContent>
          </Select>
          {!settings.s3_available && (
            <p className="text-sm text-muted-foreground">
              {t`Set up the S3 plugin first if you want backups kept off this machine.`}
            </p>
          )}
        </div>

        <div className="flex flex-col gap-2">
          <Label htmlFor="folder">{t`Folder`}</Label>
          <Input
            id="folder"
            value={config.folder}
            onChange={(event) => patch({ folder: event.target.value })}
            placeholder="backups"
          />
          <p className="text-sm text-muted-foreground">
            {config.destination === "s3"
              ? t`A prefix inside the bucket.`
              : t`A folder inside the data directory.`}
          </p>
        </div>

        <div className="flex items-center justify-between rounded-xl border border-border px-4 py-3">
          <div>
            <p className="text-sm font-medium">{t`Include the uploaded files`}</p>
            <p className="text-sm text-muted-foreground">
              {t`Makes the archive much larger. Worth it when media sits on this machine; less so when it is already in a bucket.`}
            </p>
          </div>
          <Switch
            checked={config.include_media}
            onCheckedChange={(checked) => patch({ include_media: checked })}
          />
        </div>

        <div className="grid gap-4 sm:grid-cols-2">
          <div className="flex flex-col gap-2">
            <Label htmlFor="schedule">{t`How often`}</Label>
            <Select
              value={config.schedule}
              onValueChange={(value) =>
                patch({ schedule: (value as BackupConfig["schedule"]) ?? "off" })
              }
            >
              <SelectTrigger id="schedule">
                <SelectValue>
                  {(value: string) => SCHEDULE_LABELS[value] ?? value}
                </SelectValue>
              </SelectTrigger>
              <SelectContent>
                {Object.entries(SCHEDULE_LABELS).map(([value, label]) => (
                  <SelectItem key={value} value={value}>
                    {label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          <div className="flex flex-col gap-2">
            <Label htmlFor="keep">{t`How many to keep`}</Label>
            <Input
              id="keep"
              type="number"
              min={1}
              max={365}
              value={config.keep}
              onChange={(event) =>
                patch({ keep: Math.max(1, Number(event.target.value) || 1) })
              }
            />
            <p className="text-sm text-muted-foreground">
              {t`Older archives are removed once a new one is written.`}
            </p>
          </div>
        </div>

        {config.last_error && (
          <p className="rounded-lg border border-destructive/40 bg-destructive/5 px-3 py-2 text-sm text-destructive">
            {t`The last backup failed: ${config.last_error}`}
          </p>
        )}

        <div className="flex flex-wrap items-center gap-2">
          <Button onClick={() => void save()} disabled={saving}>
            {saving ? <Loader2 className="animate-spin" /> : null}
            {t`Save`}
          </Button>
          <Button variant="outline" onClick={() => void backUpNow()} disabled={running}>
            {running ? <Loader2 className="animate-spin" /> : <Play />}
            {t`Back up now`}
          </Button>
          {config.last_run_at && (
            <span className="text-sm text-muted-foreground">
              {t`Last run ${new Date(config.last_run_at).toLocaleString()}`}
            </span>
          )}
        </div>

        <div>
          <h2 className="mb-2 text-sm font-medium">{t`Archives`}</h2>
          {settings.backups.length === 0 ? (
            <p className="rounded-xl border border-dashed border-border py-10 text-center text-sm text-muted-foreground">
              {t`No backups yet`}
            </p>
          ) : (
            <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
              {settings.backups.map((file) => (
                <div key={file.name} className="flex items-center gap-3 px-4 py-2.5">
                  <Download className="size-4 shrink-0 text-muted-foreground" />
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-sm">{file.name}</p>
                    <p className="text-xs text-muted-foreground">
                      {formatSize(file.size_bytes)} ·{" "}
                      {new Date(file.created_at).toLocaleString()}
                    </p>
                  </div>
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    aria-label={t`Delete`}
                    onClick={() => setPendingDelete(file.name)}
                  >
                    <Trash2 />
                  </Button>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>

      <AlertDialog
        open={pendingDelete !== null}
        onOpenChange={(open) => !open && setPendingDelete(null)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t`Delete this backup?`}</AlertDialogTitle>
            <AlertDialogDescription>
              {t`"${pendingDelete}" will be permanently deleted. This cannot be undone.`}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t`Cancel`}</AlertDialogCancel>
            <AlertDialogAction onClick={() => void confirmDelete()}>
              {t`Delete`}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
}

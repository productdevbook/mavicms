/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { Trans, useLingui } from "@lingui/react/macro"
import { ArrowLeft, CheckCircle2, Loader2, XCircle } from "lucide-react"
import { toast } from "sonner"

import {
  ApiError,
  getVideoSettings,
  saveVideoSettings,
  testVideoSettings,
  type ConnectionTest,
  type VideoHost,
  type VideoSettingsPayload,
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

export const Route = createFileRoute("/dashboard/plugins_/video")({
  component: VideoSettingsRoute,
})

const EMPTY: VideoSettingsPayload = {
  enabled: false,
  host: "",
  library_id: "",
  cdn_hostname: "",
  account_id: "",
  customer_subdomain: "",
  api_key: "",
  token_key: "",
  api_token: "",
}

function VideoSettingsRoute() {
  const { t } = useLingui()
  const navigate = useNavigate()
  const [form, setForm] = React.useState<VideoSettingsPayload>(EMPTY)
  const [held, setHeld] = React.useState({ key: false, token: false, api: false })
  const [eventsUrl, setEventsUrl] = React.useState("")
  const [loading, setLoading] = React.useState(true)
  const [busy, setBusy] = React.useState<"save" | "test" | null>(null)
  const [result, setResult] = React.useState<ConnectionTest | null>(null)

  React.useEffect(() => {
    getVideoSettings()
      .then((settings) => {
        setForm({ ...settings, api_key: "", token_key: "", api_token: "" })
        setHeld({
          key: settings.has_api_key,
          token: settings.has_token_key,
          api: settings.has_api_token,
        })
        setEventsUrl(settings.events_url)
      })
      .catch(() => {})
      .finally(() => setLoading(false))
  }, [])

  const patch = (fields: Partial<VideoSettingsPayload>) =>
    setForm((current) => ({ ...current, ...fields }))

  // An empty secret means "keep the stored one", so it must not be sent.
  const payload = (): VideoSettingsPayload => ({
    ...form,
    api_key: form.api_key?.trim() ? form.api_key : undefined,
    token_key: form.token_key?.trim() ? form.token_key : undefined,
    api_token: form.api_token?.trim() ? form.api_token : undefined,
  })

  const runTest = async () => {
    setBusy("test")
    setResult(null)
    try {
      setResult(await testVideoSettings(payload()))
    } catch (error) {
      setResult({
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
      const saved = await saveVideoSettings(payload())
      setForm({ ...saved, api_key: "", token_key: "", api_token: "" })
      setHeld({
        key: saved.has_api_key,
        token: saved.has_token_key,
        api: saved.has_api_token,
      })
      setEventsUrl(saved.events_url)
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

  const kept = (has: boolean) => (has ? t`Stored — leave blank to keep it` : "")

  const HOSTS: Record<string, string> = {
    bunny: t`Bunny Stream`,
    cloudflare: t`Cloudflare Stream`,
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
        <h1 className="text-lg font-semibold">{t`Video`}</h1>
        <p className="text-sm text-muted-foreground">
          {t`Where this site's lesson videos live. Files go straight from the browser to the host — never through this server — and every playback address expires a few hours after it is made.`}
        </p>
      </div>

      <form
        onSubmit={(event) => {
          event.preventDefault()
          void save()
        }}
        className="flex max-w-xl flex-col gap-4"
      >
        <div className="flex flex-col gap-2">
          <Label htmlFor="video-host">{t`Where the videos live`}</Label>
          <Select
            value={form.host}
            onValueChange={(value) => patch({ host: (value ?? "") as VideoHost })}
          >
            <SelectTrigger id="video-host">
              <SelectValue placeholder={t`Choose one`}>
                {(value: string) => HOSTS[value] ?? value}
              </SelectValue>
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="bunny">{HOSTS.bunny}</SelectItem>
              <SelectItem value="cloudflare">{HOSTS.cloudflare}</SelectItem>
            </SelectContent>
          </Select>
          <p className="text-xs text-muted-foreground">
            {t`Bunny is the cheaper of the two, and cheaper again to Turkey. Cloudflare costs more and asks less of you. Both transcode for you and both make addresses that expire.`}
          </p>
        </div>

        {form.host === "bunny" && (
          <>
            <div className="flex flex-col gap-2">
              <Label htmlFor="library">{t`Library id`}</Label>
              <Input
                id="library"
                value={form.library_id}
                onChange={(event) => patch({ library_id: event.target.value })}
                placeholder="123456"
              />
            </div>
            <div className="flex flex-col gap-2">
              <Label htmlFor="cdn">{t`Pull zone hostname`}</Label>
              <Input
                id="cdn"
                value={form.cdn_hostname}
                onChange={(event) => patch({ cdn_hostname: event.target.value })}
                placeholder="vz-abc12345-678.b-cdn.net"
              />
            </div>
            <div className="flex flex-col gap-2">
              <Label htmlFor="api-key">{t`API key`}</Label>
              <Input
                id="api-key"
                type="password"
                autoComplete="off"
                value={form.api_key ?? ""}
                onChange={(event) => patch({ api_key: event.target.value })}
                placeholder={kept(held.key)}
              />
            </div>
            <div className="flex flex-col gap-2">
              <Label htmlFor="token-key">{t`Token authentication key`}</Label>
              <Input
                id="token-key"
                type="password"
                autoComplete="off"
                value={form.token_key ?? ""}
                onChange={(event) => patch({ token_key: event.target.value })}
                placeholder={kept(held.token)}
              />
              <p className="text-xs text-muted-foreground">
                {t`A different key from the one above, in the library's own settings. Without it every video address works for ever and for anybody who has it.`}
              </p>
            </div>
          </>
        )}

        {form.host === "cloudflare" && (
          <>
            <div className="flex flex-col gap-2">
              <Label htmlFor="account">{t`Account id`}</Label>
              <Input
                id="account"
                value={form.account_id}
                onChange={(event) => patch({ account_id: event.target.value })}
              />
            </div>
            <div className="flex flex-col gap-2">
              <Label htmlFor="api-token">{t`API token`}</Label>
              <Input
                id="api-token"
                type="password"
                autoComplete="off"
                value={form.api_token ?? ""}
                onChange={(event) => patch({ api_token: event.target.value })}
                placeholder={kept(held.api)}
              />
              <p className="text-xs text-muted-foreground">
                {t`Needs Stream read and edit, and nothing else.`}
              </p>
            </div>
            <div className="flex flex-col gap-2">
              <Label htmlFor="subdomain">{t`Delivery hostname`}</Label>
              <Input
                id="subdomain"
                value={form.customer_subdomain}
                onChange={(event) =>
                  patch({ customer_subdomain: event.target.value })
                }
                placeholder="customer-abc123.cloudflarestream.com"
              />
              <p className="text-xs text-muted-foreground">
                {t`Shown beside any video in the Stream dashboard. It is the same for every video on the account and never changes.`}
              </p>
            </div>
          </>
        )}

        {eventsUrl && (
          <div className="flex flex-col gap-1 rounded-lg border border-border px-3 py-2.5">
            <p className="text-sm font-medium">{t`Tell the host where to report`}</p>
            <p className="text-xs text-muted-foreground">
              {t`Paste this into the host's webhook setting. Without it a video still uploads and still transcodes — the panel just never notices that it finished.`}
            </p>
            <code className="mt-1 overflow-x-auto font-mono text-xs">
              {eventsUrl}
            </code>
          </div>
        )}

        <div className="flex items-center justify-between gap-4 rounded-lg border border-border px-3 py-2.5">
          <div>
            <p className="text-sm font-medium">{t`Switched on`}</p>
            <p className="text-xs text-muted-foreground">
              {t`Saving this on tries the credentials first.`}
            </p>
          </div>
          <Switch
            checked={form.enabled}
            onCheckedChange={(value) => patch({ enabled: value })}
          />
        </div>

        {result && (
          <p
            className={
              result.ok
                ? "flex items-start gap-2 rounded-md bg-emerald-500/10 px-3 py-2 text-xs text-emerald-600 dark:text-emerald-400"
                : "flex items-start gap-2 rounded-md bg-destructive/10 px-3 py-2 text-xs text-destructive"
            }
          >
            {result.ok ? (
              <CheckCircle2 className="mt-px size-3.5 shrink-0" />
            ) : (
              <XCircle className="mt-px size-3.5 shrink-0" />
            )}
            {result.message}
          </p>
        )}

        <p className="text-xs text-muted-foreground">
          <Trans>
            Credentials are encrypted before they are stored and are never sent
            back to the browser.
          </Trans>
        </p>

        <div className="flex flex-wrap gap-2">
          <Button
            type="button"
            variant="outline"
            disabled={busy !== null || !form.host}
            onClick={() => void runTest()}
          >
            {busy === "test" ? <Loader2 className="animate-spin" /> : null}
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

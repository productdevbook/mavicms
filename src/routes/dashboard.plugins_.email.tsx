/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { ArrowLeft, CheckCircle2, Loader2, Send, XCircle } from "lucide-react"
import { toast } from "sonner"

import {
  ApiError,
  getEmailSettings,
  testEmailSettings,
  updateEmailSettings,
  type ConnectionTest,
  type EmailSettingsPayload,
  getSendingAllowance,
  type SendingAllowance,
} from "@/lib/api"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"
import { EmailFields } from "@/components/plugin-forms"
import { SesAccountPanel, SesSendersPanel } from "@/components/ses-account"
import { SesHealthPanel } from "@/components/ses-health"
import { SesSetupGuide } from "@/components/ses-setup"

export const Route = createFileRoute("/dashboard/plugins_/email")({
  component: EmailSettingsRoute,
})

const EMPTY: EmailSettingsPayload = {
  enabled: false,
  region: "",
  access_key_id: "",
  secret_access_key: "",
  from_address: "",
  from_name: "",
  reply_to: "",
  configuration_set: "",
  senders: [],
}

function EmailSettingsRoute() {
  const { t } = useLingui()
  const navigate = useNavigate()

  const [form, setForm] = React.useState<EmailSettingsPayload>(EMPTY)
  const [hasStoredSecret, setHasStoredSecret] = React.useState(false)
  const [loading, setLoading] = React.useState(true)
  const [busy, setBusy] = React.useState<"save" | "test" | null>(null)
  const [testTo, setTestTo] = React.useState("")
  const [result, setResult] = React.useState<ConnectionTest | null>(null)
  const [allowance, setAllowance] = React.useState<SendingAllowance | null>(null)

  // Its own keys beat anything lent to it, which is the same order the
  // sending path uses.
  const ownAccount = Boolean(form.region.trim() && hasStoredSecret)

  React.useEffect(() => {
    getEmailSettings()
      .then((settings) => {
        setForm({ ...settings, secret_access_key: "" })
        setHasStoredSecret(settings.has_secret_access_key)
      })
      .catch(() => toast.error(t`Could not load the mail settings`))
      .finally(() => setLoading(false))
    getSendingAllowance()
      .then(setAllowance)
      .catch(() => setAllowance(null))
  }, [t])

  // An empty box means "keep the stored one", so it is left out rather than
  // sent as an empty string.
  const payload = (): EmailSettingsPayload => ({
    ...form,
    ...(form.secret_access_key?.trim()
      ? { secret_access_key: form.secret_access_key.trim() }
      : { secret_access_key: undefined }),
  })

  const save = async () => {
    setBusy("save")
    setResult(null)
    try {
      const saved = await updateEmailSettings(payload())
      setForm({ ...saved, secret_access_key: "" })
      setHasStoredSecret(saved.has_secret_access_key)
      toast.success(t`Saved`)
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not save it`
      )
    } finally {
      setBusy(null)
    }
  }

  const test = async () => {
    setBusy("test")
    try {
      setResult(await testEmailSettings(testTo.trim()))
    } catch (error) {
      setResult({
        ok: false,
        message: error instanceof ApiError ? error.message : t`Could not do it`,
      })
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
        onClick={() => void navigate({ to: "/dashboard/plugins" })}
      >
        <ArrowLeft /> {t`Plugins`}
      </Button>

      <div className="mb-6">
        <h1 className="text-lg font-semibold">{t`Amazon SES`}</h1>
        <p className="text-sm text-muted-foreground">
          {t`Mail this site sends — a notification when somebody fills in one of your forms.`}
        </p>
      </div>

      <div className="flex max-w-2xl flex-col gap-6">
        <SesSetupGuide region={form.region} />

        <Label className="flex items-center gap-3 font-normal">
          <Switch
            checked={form.enabled}
            onCheckedChange={(checked) =>
              setForm({ ...form, enabled: checked === true })
            }
          />
          {t`Send mail through SES`}
        </Label>

        <EmailFields
          form={form}
          hasStoredSecret={hasStoredSecret}
          onChange={(values) => setForm({ ...form, ...values })}
        />

        <div>
          <Button onClick={() => void save()} disabled={busy !== null}>
            {busy === "save" ? <Loader2 className="animate-spin" /> : null}
            {t`Save`}
          </Button>
        </div>

        <div className="flex flex-col gap-3 border-t border-border pt-6">
          <div>
            <h2 className="text-sm font-medium">{t`Send a test`}</h2>
            <p className="text-sm text-muted-foreground">
              {t`The only honest check. A key without permission, an address SES has not verified and an account still in the sandbox all look the same until something is actually sent.`}
            </p>
          </div>

          <div className="flex flex-wrap gap-2">
            <Input
              type="email"
              value={testTo}
              onChange={(event) => setTestTo(event.target.value)}
              placeholder={t`Where to send it`}
              className="max-w-xs"
            />
            <Button
              variant="outline"
              onClick={() => void test()}
              disabled={busy !== null || !testTo.includes("@")}
            >
              {busy === "test" ? (
                <Loader2 className="animate-spin" />
              ) : (
                <Send />
              )}
              {t`Send`}
            </Button>
          </div>

          {result && (
            <div className="flex items-start gap-2 text-sm">
              {result.ok ? (
                <CheckCircle2 className="mt-0.5 size-4 shrink-0 text-emerald-600" />
              ) : (
                <XCircle className="mt-0.5 size-4 shrink-0 text-destructive" />
              )}
              <span className={result.ok ? "" : "text-destructive"}>
                {result.message}
              </span>
            </div>
          )}
        </div>

        {allowance?.sends === "shared" ? (
          <div className="rounded-xl border border-border bg-muted/40 p-4">
            <p className="text-sm font-medium">{t`This site sends through the server's own account`}</p>
            <p className="mt-1 text-sm text-muted-foreground">
              {t`${allowance.sent_today} of ${allowance.a_day ?? 0} messages used today. Whoever runs the server sets that number and can raise it.`}
            </p>
            <p className="mt-2 text-sm text-muted-foreground">
              {allowance.as_the_server
                ? t`Until you name a sender of your own, mail leaves as ${allowance.sender} — the server's address, not yours. Replies still come to you. Add your domain under Senders below, publish the records it gives you, then set it as the sender here.`
                : t`Mail leaves as ${allowance.sender}, your own address. The records under Senders have to stay published — they are what tells a receiving server that this domain agrees.`}
            </p>
            <p className="mt-2 text-sm text-muted-foreground">
              {t`Putting your own Amazon keys in above replaces this arrangement entirely.`}
            </p>
          </div>
        ) : null}

        <div className="flex flex-col gap-8 border-t border-border pt-6">
          {ownAccount ? (
            <>
              <SesAccountPanel ready />
              <SesHealthPanel />
            </>
          ) : allowance?.sends === "shared" ? (
            // Borrowing the server's account: the senders are this site's to
            // manage and everything else on that account is not.
            <SesSendersPanel borrowed />
          ) : (
            <SesAccountPanel ready={false} />
          )}
        </div>
      </div>
    </>
  )
}

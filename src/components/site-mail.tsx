import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { CheckCircle2, Loader2, Send, XCircle } from "lucide-react"
import { toast } from "sonner"

import {
  ApiError,
  getSiteEmailSettings,
  saveSiteEmailSettings,
  testSiteEmailSettings,
  type ConnectionTest,
  type EmailSettingsPayload,
} from "@/lib/api"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"
import { EmailFields } from "@/components/plugin-forms"
import { SesAccountPanel } from "@/components/ses-account"
import { SesHealthPanel } from "@/components/ses-health"

/**
 * One site's mail settings, from the agency's side.
 *
 * The same fields the site's own people see under Plugins, because these are
 * the site's credentials either way — an agency setting them up on a
 * customer's behalf is doing the same job from a different chair, not a
 * different job.
 */

const EMPTY: EmailSettingsPayload = {
  enabled: false,
  region: "",
  access_key_id: "",
  secret_access_key: "",
  from_address: "",
  from_name: "",
  reply_to: "",
  configuration_set: "",
}

export function SiteMail({ siteId }: { siteId: string }) {
  const { t } = useLingui()

  const [form, setForm] = React.useState<EmailSettingsPayload | null>(null)
  const [hasStoredSecret, setHasStoredSecret] = React.useState(false)
  const [busy, setBusy] = React.useState<"save" | "test" | null>(null)
  const [testTo, setTestTo] = React.useState("")
  const [result, setResult] = React.useState<ConnectionTest | null>(null)

  React.useEffect(() => {
    getSiteEmailSettings(siteId)
      .then((settings) => {
        setForm({ ...settings, secret_access_key: "" })
        setHasStoredSecret(settings.has_secret_access_key)
      })
      .catch(() => {
        setForm(EMPTY)
        toast.error(t`Could not load the mail settings`)
      })
  }, [siteId, t])

  const save = async () => {
    if (!form) return
    setBusy("save")
    setResult(null)
    try {
      const saved = await saveSiteEmailSettings(siteId, {
        ...form,
        // Empty means "keep the stored one", so it is left out rather than
        // sent as an empty string.
        secret_access_key: form.secret_access_key?.trim() || undefined,
      })
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
      setResult(await testSiteEmailSettings(siteId, testTo.trim()))
    } catch (error) {
      setResult({
        ok: false,
        message: error instanceof ApiError ? error.message : t`Could not do it`,
      })
    } finally {
      setBusy(null)
    }
  }

  if (!form) {
    return (
      <div className="flex justify-center py-16">
        <Loader2 className="size-6 animate-spin text-muted-foreground" />
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-6">
      <p className="text-sm text-muted-foreground">
        {t`What this site sends mail with. A form can name an address to tell, and this is how it gets there.`}
      </p>

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
            {busy === "test" ? <Loader2 className="animate-spin" /> : <Send />}
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

      <div className="flex flex-col gap-8 border-t border-border pt-6">
        <SesAccountPanel
          siteId={siteId}
          ready={Boolean(form.region.trim() && hasStoredSecret)}
        />
        {form.region.trim() && hasStoredSecret && (
          <SesHealthPanel siteId={siteId} />
        )}
      </div>
    </div>
  )
}

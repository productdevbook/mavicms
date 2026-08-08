/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import {
  ArrowLeft,
  CheckCircle2,
  Loader2,
  Plus,
  Send,
  XCircle,
} from "lucide-react"
import { toast } from "sonner"

import {
  ApiError,
  addSesIdentity,
  deleteEmailCredentials,
  getEmailSettings,
  getSendingAllowance,
  getSesIdentities,
  testEmailSettings,
  updateEmailSettings,
  type ConnectionTest,
  type EmailSettingsPayload,
  type SendingAllowance,
  type SesIdentity,
} from "@/lib/api"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"
import { EmailFields } from "@/components/plugin-forms"
import { SenderRow, SesAccountPanel } from "@/components/ses-account"
import { SesHealthPanel } from "@/components/ses-health"
import { SesSetupGuide } from "@/components/ses-setup"
import { Step } from "@/components/mail/step"

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

/** Whether a verified sender answers for this address. */
function covered(address: string, identities: SesIdentity[]): boolean {
  const at = address.trim().toLowerCase()
  const domain = at.split("@")[1] ?? ""
  if (!domain) return false
  return identities.some((identity) => {
    if (!identity.verified) return false
    const name = identity.name.trim().toLowerCase()
    return name === at || name === domain || domain.endsWith(`.${name}`)
  })
}

function EmailSettingsRoute() {
  const { t } = useLingui()
  const navigate = useNavigate()

  const [form, setForm] = React.useState<EmailSettingsPayload>(EMPTY)
  const [hasStoredSecret, setHasStoredSecret] = React.useState(false)
  const [identities, setIdentities] = React.useState<SesIdentity[]>([])
  const [allowance, setAllowance] = React.useState<SendingAllowance | null>(null)
  const [loading, setLoading] = React.useState(true)
  const [busy, setBusy] = React.useState<"save" | "test" | "add" | "give-back" | null>(null)
  const [newDomain, setNewDomain] = React.useState("")
  const [testTo, setTestTo] = React.useState("")
  const [result, setResult] = React.useState<ConnectionTest | null>(null)

  const loadIdentities = React.useCallback(() => {
    getSesIdentities()
      .then(setIdentities)
      .catch(() => setIdentities([]))
  }, [])

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
    loadIdentities()
  }, [t, loadIdentities])

  // Its own keys beat anything lent to it, which is the order the sending path
  // uses too.
  const own = Boolean(form.region.trim() && hasStoredSecret)
  const lent = !own && allowance?.sends === "shared"
  const nothing = !own && !lent

  const keysDone = own
  const domainDone = identities.some((identity) => identity.verified)
  const senderDone =
    form.from_address.trim().length > 0 && covered(form.from_address, identities)

  // The first unfinished one, which is the only one open by default.
  const step = (n: number) => (own ? n : n - 1)
  const current = !keysDone && own ? 1 : !domainDone ? 2 : !senderDone ? 3 : 4

  const payload = (): EmailSettingsPayload => ({
    ...form,
    // A site sending on the server's account has nothing to switch off — the
    // allowance is what stops it — so the switch is not shown, and this keeps
    // the stored value true so that adding keys later does not silently stop
    // the sending it was already doing.
    enabled: lent ? true : form.enabled,
    // An empty box means "keep the stored one".
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
      setAllowance(await getSendingAllowance().catch(() => allowance))
      loadIdentities()
      toast.success(t`Saved`)
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not save it`
      )
    } finally {
      setBusy(null)
    }
  }

  const addDomain = async () => {
    setBusy("add")
    try {
      await addSesIdentity(newDomain.trim())
      setNewDomain("")
      loadIdentities()
      toast.success(t`Added. Publish the records below and it verifies itself.`)
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not add it`
      )
    } finally {
      setBusy(null)
    }
  }

  const giveBack = async () => {
    setBusy("give-back")
    setResult(null)
    try {
      const saved = await deleteEmailCredentials()
      setForm({ ...saved, secret_access_key: "" })
      setHasStoredSecret(saved.has_secret_access_key)
      setAllowance(await getSendingAllowance().catch(() => allowance))
      loadIdentities()
      toast.success(t`Your keys are gone. This site sends by the server now.`)
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not do it`
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
        <h1 className="text-lg font-semibold">{t`Sending mail`}</h1>
        <p className="text-sm text-muted-foreground">
          {senderDone
            ? t`Your mail goes out as ${form.from_address}.`
            : allowance?.as_the_server
              ? t`Your mail works, but it goes out as ${allowance.sender} — the server's address. Three steps put your own on it.`
              : t`Three steps and your forms can email you.`}
        </p>
      </div>

      <div className="flex max-w-2xl flex-col gap-3">
        {nothing ? (
          <div className="rounded-xl border border-border bg-muted/40 p-4 text-sm text-muted-foreground">
            {t`Nobody has set this site up to send yet. Either put in your own Amazon keys below, or ask whoever runs this server to lend you theirs — then you need no Amazon account at all.`}
          </div>
        ) : null}

        {own || nothing ? (
          <Step
            number={1}
            title={t`Your Amazon keys`}
            summary={form.region || undefined}
            done={keysDone}
            current={current === 1}
          >
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
              onlyCredentials
              onChange={(values) => setForm({ ...form, ...values })}
            />
            <div className="flex flex-wrap items-center gap-2">
              <Button onClick={() => void save()} disabled={busy !== null}>
                {busy === "save" ? <Loader2 className="animate-spin" /> : null}
                {t`Save`}
              </Button>

              {own && allowance?.sends !== "none" ? (
                <Button
                  variant="ghost"
                  onClick={() => void giveBack()}
                  disabled={busy !== null}
                >
                  {busy === "give-back" ? (
                    <Loader2 className="animate-spin" />
                  ) : null}
                  {t`Use the server's account instead`}
                </Button>
              ) : null}
            </div>

            {own ? (
              <p className="text-sm text-muted-foreground">
                {t`Handing the account back keeps the address you send from and where replies go. It does not move your verified domains: those were verified in your Amazon account, so you add the domain again here and publish the new records the server's account gives you.`}
              </p>
            ) : null}
          </Step>
        ) : null}

        <Step
          number={step(2)}
          title={t`The domain you send from`}
          summary={identities
            .filter((identity) => identity.verified)
            .map((identity) => identity.name)
            .join(", ")}
          done={domainDone}
          current={current === 2}
        >
          <p className="text-sm text-muted-foreground">
            {lent
              ? t`You do not need an Amazon account — the server lends you its. Add your domain here and it gives you a few DNS records to publish. Once they are live your mail goes out as your own domain, which is what stops it landing in spam.`
              : t`Amazon refuses to send from a domain it has not been shown you own. Add yours and it gives you records to publish, which cover every address on it.`}
          </p>

          <div className="flex flex-wrap gap-2">
            <Input
              value={newDomain}
              onChange={(event) => setNewDomain(event.target.value)}
              placeholder="example.com"
              className="max-w-sm"
            />
            <Button
              variant="outline"
              onClick={() => void addDomain()}
              disabled={busy !== null || newDomain.trim().length < 3}
            >
              {busy === "add" ? <Loader2 className="animate-spin" /> : <Plus />}
              {t`Add`}
            </Button>
          </div>

          {identities.length > 0 ? (
            <div className="flex flex-col gap-3">
              {identities.map((identity) => (
                <SenderRow
                  key={identity.name}
                  identity={identity}
                  busy={busy !== null}
                  onChanged={loadIdentities}
                />
              ))}
            </div>
          ) : null}
        </Step>

        <Step
          number={step(3)}
          title={t`The address it comes from`}
          summary={form.from_address}
          done={senderDone}
          current={current === 3}
        >
          <p className="text-sm text-muted-foreground">
            {domainDone
              ? t`Any address at a domain above works — it does not have to be a real mailbox, though replies go to it unless you give a reply address.`
              : t`Add and verify a domain first, then any address at it can be used here.`}
          </p>
          <EmailFields
            form={form}
            hasStoredSecret={hasStoredSecret}
            onlySender
            onChange={(values) => setForm({ ...form, ...values })}
          />
          <div>
            <Button onClick={() => void save()} disabled={busy !== null}>
              {busy === "save" ? <Loader2 className="animate-spin" /> : null}
              {t`Save`}
            </Button>
          </div>
        </Step>

        <Step
          number={step(4)}
          title={t`Send yourself one`}
          done={Boolean(result?.ok)}
          current={current === 4}
        >
          <p className="text-sm text-muted-foreground">
            {t`The only honest check. A key without permission, a domain that has not finished verifying and an account still in the sandbox all look the same until something is actually sent.`}
          </p>
          <div className="flex flex-wrap gap-2">
            <Input
              type="email"
              value={testTo}
              onChange={(event) => setTestTo(event.target.value)}
              placeholder={t`Your own address`}
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
        </Step>

        {allowance?.sends === "shared" ? (
          <div className="mt-3 rounded-xl border border-border bg-muted/40 p-4">
            <p className="text-sm font-medium">{t`How much you may send`}</p>
            <p className="mt-1 text-sm text-muted-foreground">
              {t`${allowance.sent_today} of ${allowance.a_day ?? 0} messages today. Whoever runs the server sets that number and can raise it.`}
            </p>
          </div>
        ) : null}

        {own ? (
          <div className="mt-6 flex flex-col gap-8 border-t border-border pt-6">
            <SesAccountPanel ready />
            <SesHealthPanel />
          </div>
        ) : null}
      </div>
    </>
  )
}
